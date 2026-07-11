# M5 — Embedded On-Device LLM Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Embed a real llama.cpp (via `llama-cpp-2`) small GGUF model that runs fully offline to (a) give advisory "safe to delete?" verdict badges, (b) pick a class from a candidate list during organize, and (c) summarize the Unknown inventory bucket — with the app degrading gracefully to rule-based behavior when no model is present.

**Architecture:** A new `llm` module splits into **pure decision logic** (prompt construction, forced-JSON parsing, verdict cache, model registry + SHA-256 verify, backend priority selection, candidate-class filtering) which is unit-tested to 100% line coverage, and an **effectful FFI seam** (`InferenceEngine` trait; real `llama-cpp-2` model load + inference; HTTP model download; hardware backend probing) which is entirely behind `#[cfg(not(coverage))]` so it vanishes on the Linux coverage gate. Orchestration functions are generic over `&dyn InferenceEngine` and tested with a fake engine returning canned strings. The LLM is advisory only: it never triggers a destructive op; verdicts are UI badges. This milestone builds **CPU-only** llama.cpp; GPU backends (CUDA/Vulkan/Metal) are M6.

**Tech Stack:** Rust, Tauri 2, `llama-cpp-2` (CPU build via cmake), `sha2` (SHA-256), `ureq` (blocking model download, rustls), Svelte 5 frontend.

## Global Constraints

- **Coverage gate (org CI):** `cargo llvm-cov --all-features --fail-under-lines 100` on ubuntu MUST stay at 100% lines. Every effectful/FFI/GUI path is `#[cfg(not(coverage))]`; only pure logic is measured. Reuse the established patterns: io-helper returns `io::Result` with `?` (no happy-path `map_err` closure) + one boundary `map_err` covered by an error test; single-line `if running_as_root() { return; }` guards; no unreachable `else` branches; platform-neutral test assertions are NOT `#[cfg]`-gated.
- **`--all-features` must build CPU-only:** disksage's `Cargo.toml` declares **NO** `cuda`/`vulkan`/`metal` passthrough features. `llama-cpp-2` is depended on with default features (CPU). GPU backends are enabled only by M6 release builds via `cargo build --features "llama-cpp-2/cuda"` etc. — never as disksage features (else `--all-features` would pull CUDA toolkit and break the gate).
- **Privacy:** file **content is never sent** to the model. Prompts contain only metadata: path, name, size, mtime, parent-dir context, and locally-derived type signals.
- **Advisory only:** an LLM verdict is a badge. No code path lets a verdict trigger trash/move. Deletion stays `safety::trash_delete`; move stays `safety::move_file`.
- **Forced JSON, temp 0:** model output is coerced to `{"verdict":"safe"|"caution"|"keep","reason":"..."}`; classify returns a class id chosen from the candidate list only (free generation rejected).
- **Graceful degradation:** no model / download failure / inference error ⇒ app is fully functional on rules; badge shows "미판정" (Unrated). LLM failure never breaks a feature (spec §8).
- **Conventions:** Tauri command wrappers are `#[cfg(not(coverage))]` with pure `*_inner` helpers that are tested (see `commands.rs`). Frontend adds typed `invoke` wrappers to `src/lib/api.ts`. Rust modules registered in `lib.rs` with `#[cfg_attr(coverage, allow(dead_code))]`.

---

## File Structure

- Create: `src-tauri/src/llm/mod.rs` — module root, re-exports, `InferenceEngine` trait, orchestration (`verdict_for`, `pick_class`, `summarize_unknown`) generic over the trait.
- Create: `src-tauri/src/llm/verdict.rs` — `Verdict` enum + `FileVerdict` struct (serde).
- Create: `src-tauri/src/llm/prompt.rs` — pure prompt builders from metadata.
- Create: `src-tauri/src/llm/parse.rs` — pure forced-JSON extraction + parsing.
- Create: `src-tauri/src/llm/cache.rs` — pure verdict cache keyed by (path,size,mtime).
- Create: `src-tauri/src/llm/model.rs` — model registry + pure SHA-256 verify + `#[cfg(not(coverage))]` download.
- Create: `src-tauri/src/llm/backend.rs` — `Backend` enum + pure `choose_backend`; `#[cfg(not(coverage))]` hardware probe.
- Create: `src-tauri/src/llm/engine.rs` — `#[cfg(not(coverage))]` real `llama-cpp-2` engine implementing `InferenceEngine`.
- Modify: `src-tauri/src/lib.rs` — register `mod llm`; add new commands to the handler.
- Modify: `src-tauri/src/commands.rs` — `#[cfg(not(coverage))]` command wrappers + pure inner helpers for verdict/classify/summary/model-status.
- Modify: `src-tauri/src/organize.rs` — optional LLM class-selection step ② via the engine seam.
- Modify: `src-tauri/Cargo.toml` — add `llama-cpp-2`, `sha2`, `ureq`.
- Modify: `src/lib/api.ts` — types + `invoke` wrappers for verdict/model/summary.
- Modify: `src/lib/Duplicates.svelte`, `src/lib/Cleanup.svelte`, `src/lib/Organize.svelte` — verdict badges.
- Modify: `src/lib/Inventory.svelte` — Unknown-bucket summary + model status.
- Create: `src/lib/verdictBadge.ts` + `src/lib/verdictBadge.test.ts` — pure badge mapping (label/color/title) under the JS coverage gate.

---

## Task 1: Dependencies + llm module skeleton (Verdict, Backend, choose_backend)

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/llm/mod.rs`, `src-tauri/src/llm/verdict.rs`, `src-tauri/src/llm/backend.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Produces: `llm::Verdict` (`Safe|Caution|Keep|Unrated`), `llm::FileVerdict { path, verdict, reason }`, `llm::Backend` (`Cuda|Vulkan|Metal|Cpu`), `llm::choose_backend(available: &[Backend], override_: Option<Backend>) -> Backend`.

**Why first:** adding `llama-cpp-2` forces the native (cmake + libclang) build on CI early. If the coverage/build runners lack the C++ toolchain, this task's PR surfaces it immediately and we patch the org `.github` workflow reactively (as M1 did for GTK) before more code piles up.

- [ ] **Step 1: Add dependencies.** In `src-tauri/Cargo.toml` `[dependencies]` add (pin exact versions at implementation time from crates.io latest stable):

```toml
llama-cpp-2 = "0.1"      # CPU build only — do NOT enable cuda/vulkan/metal features here
sha2 = "0.10"
ureq = { version = "2", features = ["tls"] }
```

Do NOT add any `[features]` mapping to `llama-cpp-2/cuda|vulkan|metal`.

- [ ] **Step 2: Write failing test for `choose_backend`** in `backend.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prefers_cuda_then_vulkan_then_cpu() {
        assert_eq!(choose_backend(&[Backend::Cpu, Backend::Vulkan, Backend::Cuda], None), Backend::Cuda);
        assert_eq!(choose_backend(&[Backend::Cpu, Backend::Vulkan], None), Backend::Vulkan);
        assert_eq!(choose_backend(&[Backend::Cpu], None), Backend::Cpu);
    }
    #[test]
    fn empty_available_falls_back_to_cpu() {
        assert_eq!(choose_backend(&[], None), Backend::Cpu);
    }
    #[test]
    fn honors_override_when_available() {
        assert_eq!(choose_backend(&[Backend::Cpu, Backend::Cuda], Some(Backend::Cpu)), Backend::Cpu);
    }
    #[test]
    fn ignores_override_when_unavailable() {
        // 요청한 백엔드가 없으면 자동 우선순위로 (스펙: 자동 감지 + 수동 보정 노브)
        assert_eq!(choose_backend(&[Backend::Cpu], Some(Backend::Cuda)), Backend::Cpu);
    }
}
```

- [ ] **Step 3: Run test to verify it fails** — `cargo test --lib llm::backend` → FAIL (unresolved `choose_backend`).

- [ ] **Step 4: Implement `backend.rs`:**

```rust
//! GPU 백엔드 선택 — 순수 우선순위 로직(테스트 100%)과 하드웨어 프로브(FFI, cfg(not(coverage)))를 분리.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Cuda,
    Vulkan,
    Metal,
    Cpu,
}

/// 자동 우선순위: CUDA > Metal > Vulkan > CPU. override_가 available에 있으면 그것을,
/// 없으면 자동 우선순위. available이 비면 항상 Cpu(최종 폴백).
pub fn choose_backend(available: &[Backend], override_: Option<Backend>) -> Backend {
    if let Some(b) = override_ {
        if available.contains(&b) {
            return b;
        }
    }
    for pref in [Backend::Cuda, Backend::Metal, Backend::Vulkan] {
        if available.contains(&pref) {
            return pref;
        }
    }
    Backend::Cpu
}
```

- [ ] **Step 5: Implement `verdict.rs`:**

```rust
//! LLM 삭제-안전 판정. 자문(advisory)일 뿐 — 삭제 트리거가 될 수 없음(스펙 §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Safe,
    Caution,
    Keep,
    /// 모델 없음·추론 실패 → 규칙 기반 동작 유지, 배지만 "미판정"
    Unrated,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileVerdict {
    pub path: String,
    pub verdict: Verdict,
    pub reason: String,
}
```

Add a serde round-trip test for `Verdict`/`FileVerdict` (covers derive lines):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verdict_serde_roundtrip() {
        for v in [Verdict::Safe, Verdict::Caution, Verdict::Keep, Verdict::Unrated] {
            let s = serde_json::to_string(&v).unwrap();
            assert_eq!(serde_json::from_str::<Verdict>(&s).unwrap(), v);
        }
        let fv = FileVerdict { path: "/a".into(), verdict: Verdict::Safe, reason: "cache".into() };
        let s = serde_json::to_string(&fv).unwrap();
        assert_eq!(serde_json::from_str::<FileVerdict>(&s).unwrap(), fv);
    }
}
```

- [ ] **Step 6: Create `mod.rs` shell** re-exporting the submodules:

```rust
mod backend;
mod verdict;
pub use backend::{choose_backend, Backend};
pub use verdict::{FileVerdict, Verdict};
```

- [ ] **Step 7: Register in `lib.rs`** — add alongside the other modules:

```rust
#[cfg_attr(coverage, allow(dead_code))]
mod llm;
```

- [ ] **Step 8: Run tests + coverage build** — `cargo test --lib` (all pass) and `cargo build --lib` (compiles llama.cpp — expect a long first build). Then `RUSTFLAGS="--cfg coverage" cargo build --lib` → 0 code warnings.

- [ ] **Step 9: Commit** — `git commit -m "feat(llm): add llama-cpp-2 dep + Verdict/Backend types + choose_backend"`.

---

## Task 2: Prompt builders (metadata-only)

**Files:**
- Create: `src-tauri/src/llm/prompt.rs`
- Modify: `src-tauri/src/llm/mod.rs` (add `mod prompt;`)

**Interfaces:**
- Consumes: nothing external.
- Produces: `FileMeta { path, name, size, mtime_days, parent }`; `prompt::verdict_prompt(&FileMeta) -> String`; `prompt::classify_prompt(&FileMeta, candidates: &[&str]) -> String`; `prompt::summary_prompt(samples: &[FileMeta]) -> String`. All embed a strict "reply with only JSON" instruction and **never** include file content.

- [ ] **Step 1: Write failing tests** asserting: (a) the verdict prompt contains name/size/parent and the exact JSON schema literal `{"verdict":`; (b) content is never included (there is no content field to include — assert the prompt does not contain a sentinel we pass only as a hypothetical, i.e. assert the builder signature takes no content); (c) `classify_prompt` lists every candidate and instructs "choose exactly one id from the list"; (d) `summary_prompt` includes each sample name.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn meta() -> FileMeta {
        FileMeta { path: "/downloads/old_report.pdf".into(), name: "old_report.pdf".into(),
                   size: 2_400_000, mtime_days: 420, parent: "downloads".into() }
    }
    #[test]
    fn verdict_prompt_has_metadata_and_schema() {
        let p = verdict_prompt(&meta());
        assert!(p.contains("old_report.pdf"));
        assert!(p.contains("downloads"));
        assert!(p.contains(r#"{"verdict":"#));
        assert!(p.contains("safe") && p.contains("caution") && p.contains("keep"));
    }
    #[test]
    fn classify_prompt_lists_all_candidates_and_forbids_free_text() {
        let p = classify_prompt(&meta(), &["Image", "Document", "Installer"]);
        for c in ["Image", "Document", "Installer"] { assert!(p.contains(c)); }
        assert!(p.to_lowercase().contains("exactly one"));
    }
    #[test]
    fn summary_prompt_includes_each_sample() {
        let p = summary_prompt(&[meta()]);
        assert!(p.contains("old_report.pdf"));
    }
}
```

- [ ] **Step 2: Run** — `cargo test --lib llm::prompt` → FAIL.

- [ ] **Step 3: Implement `prompt.rs`** with a `FileMeta` struct and three builders. Each prompt is a plain instruction string with metadata interpolated and a hard "Reply with ONLY the JSON, no prose" directive. Keep them short (temp-0 tiny model). No content field exists on `FileMeta` (privacy by construction).

- [ ] **Step 4: Run** — `cargo test --lib llm::prompt` → PASS.

- [ ] **Step 5: Commit** — `git commit -m "feat(llm): metadata-only prompt builders (verdict/classify/summary)"`.

---

## Task 3: Forced-JSON parsing

**Files:**
- Create: `src-tauri/src/llm/parse.rs`
- Modify: `src-tauri/src/llm/mod.rs` (add `mod parse;`)

**Interfaces:**
- Produces: `parse::parse_verdict(raw: &str) -> Verdict` (Unrated on any failure — never panics); `parse::parse_class_pick(raw: &str, candidates: &[&str]) -> Option<String>` (Some only if the parsed id is in candidates); `parse::parse_summary(raw: &str) -> Option<String>`.

**Rationale:** tiny models wrap JSON in prose or ```` ```json ```` fences. Extract the first balanced `{...}` object, then `serde_json`. Fail closed: parse errors ⇒ `Unrated`/`None`, never a panic.

- [ ] **Step 1: Write failing tests:**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_clean_json() {
        assert_eq!(parse_verdict(r#"{"verdict":"safe","reason":"cache file"}"#), Verdict::Safe);
    }
    #[test]
    fn parses_json_with_prose_and_fences() {
        let raw = "Sure!\n```json\n{\"verdict\": \"keep\", \"reason\": \"user doc\"}\n```\n";
        assert_eq!(parse_verdict(raw), Verdict::Keep);
    }
    #[test]
    fn unknown_verdict_value_is_unrated() {
        assert_eq!(parse_verdict(r#"{"verdict":"delete"}"#), Verdict::Unrated);
    }
    #[test]
    fn garbage_is_unrated_not_panic() {
        assert_eq!(parse_verdict("no json here"), Verdict::Unrated);
        assert_eq!(parse_verdict(""), Verdict::Unrated);
    }
    #[test]
    fn class_pick_only_from_candidates() {
        assert_eq!(parse_class_pick(r#"{"class":"Image"}"#, &["Image","Doc"]), Some("Image".into()));
        assert_eq!(parse_class_pick(r#"{"class":"Video"}"#, &["Image","Doc"]), None); // 자유 생성 거부
        assert_eq!(parse_class_pick("junk", &["Image"]), None);
    }
    #[test]
    fn summary_extracted_or_none() {
        assert_eq!(parse_summary(r#"{"summary":"old installers"}"#), Some("old installers".into()));
        assert_eq!(parse_summary("junk"), None);
    }
}
```

- [ ] **Step 2: Run** — FAIL.

- [ ] **Step 3: Implement `parse.rs`.** Provide `fn extract_json(raw: &str) -> Option<&str>` scanning for the first `{` and its matching `}` (brace-depth counter). Then `serde_json::from_str::<serde_json::Value>` and read the field. Map `"safe"/"caution"/"keep"` → variants, anything else → `Unrated`. Ensure every branch (no `{`, unbalanced, serde error, missing field, unknown value) is exercised by the tests above — add a case for unbalanced braces if a line shows uncovered. Use only `?`-free combinators or reachable branches (no unreachable `else`).

- [ ] **Step 4: Run** — PASS. Then `cargo test --lib` full suite PASS.

- [ ] **Step 5: Commit** — `git commit -m "feat(llm): fail-closed forced-JSON parsing (verdict/class/summary)"`.

---

## Task 4: Verdict cache

**Files:**
- Create: `src-tauri/src/llm/cache.rs`
- Modify: `src-tauri/src/llm/mod.rs` (add `mod cache;`)

**Interfaces:**
- Produces: `cache::VerdictCache` with `new()`, `key(path,size,mtime_ms) -> String`, `get(&key) -> Option<Verdict>`, `put(key, Verdict)`. In-memory `HashMap`. (Spec: "판정 결과 로컬 캐시" — in-memory per session is sufficient for v1; disk persistence is YAGNI unless requested.)

- [ ] **Step 1: Write failing tests:** key stability (same inputs → same key; different size/mtime → different key), get-after-put returns the value, get-miss returns None, and putting `Unrated` is allowed but a helper `get_rated` returns None for cached Unrated (so unrated files are re-tried next session — optional; include only if trivial).

- [ ] **Step 2: Run** — FAIL.

- [ ] **Step 3: Implement `cache.rs`** — `key` = `format!("{path}|{size}|{mtime_ms}")`; `HashMap<String, Verdict>`.

- [ ] **Step 4: Run** — PASS.

- [ ] **Step 5: Commit** — `git commit -m "feat(llm): in-memory verdict cache keyed by path|size|mtime"`.

---

## Task 5: Model registry + SHA-256 verify (pure) + download seam

**Files:**
- Create: `src-tauri/src/llm/model.rs`
- Modify: `src-tauri/src/llm/mod.rs` (add `mod model;`)

**Interfaces:**
- Produces: `model::ModelSpec { name, url, sha256_hex, bytes }`; `model::DEFAULT: ModelSpec`; `model::verify_sha256(bytes: &[u8], expected_hex: &str) -> bool` (pure, case-insensitive, fail-closed); `#[cfg(not(coverage))] model::download_to(spec, dest, progress) -> Result<(), String>` (ureq stream → temp file → verify → rename).

**Model choice:** default **Qwen3-1.7B-Instruct GGUF Q4_K_M** (Apache-2.0, clean for an MIT app; downloaded at first use, not bundled). Pin the exact HuggingFace resolve URL + SHA-256 at implementation time from a reputable GGUF repo. Lighter alt Llama-3.2-1B stays a one-line registry swap. Record the chosen URL/SHA in a comment.

- [ ] **Step 1: Write failing tests** for `verify_sha256`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verify_sha256_matches_known_vector() {
        // echo -n "abc" | sha256sum
        let want = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(verify_sha256(b"abc", want));
        assert!(verify_sha256(b"abc", &want.to_uppercase())); // 대소문자 무관
    }
    #[test]
    fn verify_sha256_rejects_mismatch_and_bad_hex() {
        assert!(!verify_sha256(b"abc", "deadbeef"));
        assert!(!verify_sha256(b"xyz", "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"));
    }
    #[test]
    fn default_spec_is_wellformed() {
        assert!(DEFAULT.url.starts_with("https://"));
        assert_eq!(DEFAULT.sha256_hex.len(), 64);
        assert!(DEFAULT.bytes > 0);
    }
}
```

- [ ] **Step 2: Run** — FAIL.

- [ ] **Step 3: Implement `model.rs`.** `verify_sha256`: `let mut h = Sha256::new(); h.update(bytes); let got: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect(); got.eq_ignore_ascii_case(expected_hex)` (no `hex` crate needed). Keep `download_to` behind `#[cfg(not(coverage))]`; stream `ureq::get(url).call()` body to a `dest.with_extension("part")` file via an **io-helper returning `io::Result`** (no happy-path `map_err` closure), then read+verify SHA, then `std::fs::rename` into place; on SHA mismatch delete the part file and return `Err`. The single boundary `map_err(|e| e.to_string())` is fine because `download_to` itself is `cfg(not(coverage))` (excluded from the gate).

- [ ] **Step 4: Run** — `cargo test --lib llm::model` PASS.

- [ ] **Step 5: Commit** — `git commit -m "feat(llm): model registry + pure SHA-256 verify + cfg-gated download"`.

---

## Task 6: Inference engine seam + orchestration

**Files:**
- Create: `src-tauri/src/llm/engine.rs` (`#[cfg(not(coverage))]`)
- Modify: `src-tauri/src/llm/mod.rs` (trait + orchestration + fake-engine tests)

**Interfaces:**
- Produces: `pub trait InferenceEngine { fn infer(&self, prompt: &str) -> Result<String, String>; }`; orchestration on `&dyn InferenceEngine`:
  - `verdict_for(engine, &FileMeta) -> FileVerdict` (build prompt → infer → parse → FileVerdict; infer `Err` ⇒ `Unrated`).
  - `pick_class(engine, &FileMeta, candidates: &[&str]) -> Option<String>`.
  - `summarize_unknown(engine, samples: &[FileMeta]) -> Option<String>`.
- `engine.rs` (`cfg(not(coverage))`) provides `LlamaEngine` implementing `InferenceEngine` via `llama-cpp-2` (CPU): load model once, per-call context, tokenize, greedy/temp-0 decode until EOS or a token cap, return the text.

- [ ] **Step 1: Write failing tests** in `mod.rs` using a fake engine:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    struct Fake(Result<String, String>);
    impl InferenceEngine for Fake {
        fn infer(&self, _p: &str) -> Result<String, String> { self.0.clone() }
    }
    fn meta() -> FileMeta { /* as Task 2 */ }
    #[test]
    fn verdict_for_maps_model_json() {
        let e = Fake(Ok(r#"{"verdict":"safe","reason":"cache"}"#.into()));
        let fv = verdict_for(&e, &meta());
        assert_eq!(fv.verdict, Verdict::Safe);
        assert_eq!(fv.reason, "cache");
    }
    #[test]
    fn verdict_for_infer_error_is_unrated() {
        let e = Fake(Err("no model".into()));
        assert_eq!(verdict_for(&e, &meta()).verdict, Verdict::Unrated);
    }
    #[test]
    fn pick_class_rejects_out_of_list() {
        let e = Fake(Ok(r#"{"class":"Video"}"#.into()));
        assert_eq!(pick_class(&e, &meta(), &["Image"]), None);
    }
    #[test]
    fn pick_class_error_is_none() {
        let e = Fake(Err("x".into()));
        assert_eq!(pick_class(&e, &meta(), &["Image"]), None);
    }
    #[test]
    fn summarize_unknown_error_is_none() {
        let e = Fake(Err("x".into()));
        assert_eq!(summarize_unknown(&e, &[meta()]), None);
    }
    #[test]
    fn summarize_unknown_maps_summary() {
        let e = Fake(Ok(r#"{"summary":"old stuff"}"#.into()));
        assert_eq!(summarize_unknown(&e, &[meta()]), Some("old stuff".into()));
    }
}
```

- [ ] **Step 2: Run** — FAIL.

- [ ] **Step 3: Implement the trait + orchestration** in `mod.rs`. Each orchestration fn: build prompt (Task 2) → `engine.infer()` → on `Ok` parse (Task 3), on `Err` return the fail-closed value. `verdict_for` also passes the `reason` field through when present (extend `parse` with a `parse_verdict_full(raw) -> (Verdict, String)` helper, tested here — keep the earlier `parse_verdict` or fold it in; ensure no line goes uncovered).

- [ ] **Step 4: Implement `engine.rs`** (`#[cfg(not(coverage))]`) — real `llama-cpp-2` CPU engine. Pin the crate version and follow its current API for: `LlamaBackend::init()`, `LlamaModel::load_from_file` with `LlamaModelParams` (n_gpu_layers = 0 for CPU in M5), `model.new_context` with a small `n_ctx`, tokenize the prompt, greedy decode (temperature 0 via a deterministic sampler) up to ~256 tokens or EOS, detokenize. Wrap all fallible calls returning `Result<String, String>`. This file is excluded from coverage; correctness is verified by a `#[ignore]` integration test (Step 5) and manual run, per spec §9.

- [ ] **Step 5: Add an `#[ignore]` real-model smoke test** (runs only with a downloaded model, never on the gate):

```rust
#[cfg(not(coverage))]
#[test]
#[ignore = "requires a downloaded GGUF model; run manually"]
fn real_engine_returns_parseable_verdict() {
    // path from env DISKSAGE_MODEL; load LlamaEngine; verdict_for(...) is Safe|Caution|Keep (not Unrated)
}
```

- [ ] **Step 6: Run** — `cargo test --lib llm` (fake-engine tests PASS; ignored test skipped). Full `cargo test --lib` PASS.

- [ ] **Step 7: Commit** — `git commit -m "feat(llm): InferenceEngine seam + orchestration + real llama-cpp-2 CPU engine (cfg-gated)"`.

---

## Task 7: IPC commands (model status/download, verdicts, summary)

**Files:**
- Modify: `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`

**Interfaces:**
- Produces (pure inner, tested): `model_dir_for(app_data: &Path) -> PathBuf`; `model_status_inner(model_path: &Path) -> ModelStatus { present: bool, name: String }`; `verdicts_from(engine, metas) -> Vec<FileVerdict>` (thin loop, tested with fake engine + cache).
- Produces (`#[cfg(not(coverage))]` wrappers): `model_status`, `download_model` (emits progress events), `file_verdicts(paths) -> Vec<FileVerdict>`, `summarize_unknown_bucket(paths) -> Option<String>`. Register all in `lib.rs` handler.

- [ ] **Step 1: Write failing tests** for `model_status_inner` (present vs absent via a tempdir file) and `verdicts_from` (fake engine + cache: second call for the same file hits cache — assert engine called once via a counter in the fake).

- [ ] **Step 2: Run** — FAIL.

- [ ] **Step 3: Implement inner helpers** (measured) and `#[cfg(not(coverage))]` wrappers (excluded). Wrappers resolve the app data dir via `app.path()`, build/hold a `LlamaEngine` in `AppState` (lazy `OnceCell`/`Mutex<Option<...>>`), and degrade to `Unrated` when the model file is absent (never construct the engine → all `Unrated`). Add the new commands to the `generate_handler!` list in `lib.rs`.

- [ ] **Step 4: Run** — `cargo test --lib commands` PASS.

- [ ] **Step 5: Commit** — `git commit -m "feat(llm): IPC for model status/download, file verdicts, unknown summary"`.

---

## Task 8: Organize LLM class-selection (step ②)

**Files:**
- Modify: `src-tauri/src/organize.rs`

**Interfaces:**
- Produces: `plan_moves_with(files, onto, home, pick: impl Fn(&FileMeta, &[&str]) -> Option<String>) -> Vec<MovePlan>` — the low-cost signals narrow to candidate classes; `pick` (LLM or a pass-through) selects among them; falls back to the existing extension classify when `pick` returns `None`. Keep the current `plan_moves` as `plan_moves_with(.., |_, _| None)` so existing behavior/tests are unchanged.

- [ ] **Step 1: Write failing tests:** (a) with a `pick` that always returns `None`, `plan_moves_with` == current `plan_moves` (delegates to extension classify); (b) with a `pick` that returns a candidate, that class wins over the extension default; (c) `pick` returning an out-of-candidate id is ignored (guarded already by Task 3, but assert at this layer).

- [ ] **Step 2: Run** — FAIL.

- [ ] **Step 3: Implement** `plan_moves_with`; refactor `plan_moves` to delegate. Ensure no uncovered branch (reachable candidate/none/fallback paths all tested). The LLM wiring in the command layer passes a closure calling `llm::pick_class`; the pure planner never links FFI.

- [ ] **Step 4: Run** — `cargo test --lib organize` PASS, full `cargo test --lib` PASS.

- [ ] **Step 5: Commit** — `git commit -m "feat(organize): optional LLM class-selection via injected picker (fallback to rules)"`.

---

## Task 9: Frontend — verdict badges, model status/download, unknown summary

**Files:**
- Create: `src/lib/verdictBadge.ts`, `src/lib/verdictBadge.test.ts`
- Modify: `src/lib/api.ts`, `src/lib/Duplicates.svelte`, `src/lib/Cleanup.svelte`, `src/lib/Organize.svelte`, `src/lib/Inventory.svelte`

**Interfaces:**
- Produces: `verdictBadge(v: Verdict) -> { label: string; cls: string; title: string }` (pure, JS-coverage-gated); `api.ts` wrappers `modelStatus()`, `downloadModel()`, `fileVerdicts(paths)`, `summarizeUnknownBucket(paths)` + `Verdict`/`FileVerdict`/`ModelStatus` types.

- [ ] **Step 1: Write failing vitest** `verdictBadge.test.ts`: each of `safe|caution|keep|unrated` maps to a distinct label/class; unknown input falls back to the `unrated` badge. (100% branch coverage — this is the only new JS logic under the gate.)

- [ ] **Step 2: Run** — `npm run test -- verdictBadge` → FAIL.

- [ ] **Step 3: Implement `verdictBadge.ts`** (a switch with an `unrated` default) and add the `api.ts` types + `invoke` wrappers.

- [ ] **Step 4: Run** — PASS; `npm run coverage` → 100% on the new file.

- [ ] **Step 5: Wire UI (no new gate logic):** a small model-status/download control in `Inventory.svelte` (download button when absent, progress while downloading, "미판정" note when unavailable); verdict badges next to candidate items in `Duplicates.svelte`/`Cleanup.svelte`/`Organize.svelte` (fetch `fileVerdicts` for the visible list, render `verdictBadge`); Unknown-bucket "요약 보기" button in `Inventory.svelte` calling `summarizeUnknownBucket`. Keep badges advisory — they never gate the existing confirm/execute controls.

- [ ] **Step 6: Run** — `npm run test`, `npm run coverage`, `svelte-check`, `npm run build` all clean.

- [ ] **Step 7: Commit** — `git commit -m "feat(ui): verdict badges + model status/download + unknown-bucket summary"`.

---

## Post-implementation

- Final whole-branch review (most capable model) over `git merge-base main HEAD..HEAD`, with attention to: the four Global Constraints (coverage seams, no GPU passthrough features, metadata-only privacy, advisory-only), fail-closed parsing, and graceful degradation when no model is present.
- Open PR; expect the first CI run to reveal whether the ubuntu coverage/build runners have the CPU llama.cpp toolchain (cmake + libclang). If a build step fails on the native toolchain, patch `ContextualWisdomLab/.github` (add the toolchain install to the Rust-building workflows, mirroring the GTK fix in #403) and re-push.
- Verify `cargo llvm-cov --all-features --fail-under-lines 100` is green (only pure logic measured; all FFI `cfg(not(coverage))`).
- GPU backends, 3-OS release artifacts, and macOS safety hardening remain **M6**.
