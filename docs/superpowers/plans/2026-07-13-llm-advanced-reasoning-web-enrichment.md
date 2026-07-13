# LLM Advanced Reasoning + Opt-in Web Enrichment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give unknown files per-file-type reasoning — the local LLM proposes a type + ontology class for each distinct unknown extension, with an opt-in, anonymous web lookup (default off) enriching the type description.

**Architecture:** Extend the existing `InferenceEngine`/prompt/parse pattern for offline extension reasoning; add a new `web` module (pure query/parse + `#[cfg(not(coverage))]` `ureq` impl) for opt-in enrichment; add a `settings.rs` (file-as-state in `app_config_dir`, mirroring the ontology-TTL override) for the online toggle; a new `reasoning.rs` merges the two into advisory `ExtInsight`s surfaced in the Inventory Unknown view.

**Tech Stack:** Rust, existing `ureq`/`serde`/`serde_json`/`oxttl` (NO new deps), Svelte 5 frontend.

## Global Constraints

- **Spec:** `docs/superpowers/specs/2026-07-13-llm-advanced-reasoning-web-enrichment-design.md`.
- **Privacy (the reason this design exists — verbatim from spec §3):** (1) file **contents never leave the machine**; (2) **only the bare extension token** reaches the web (never filename/path/size/mtime); (3) **default offline** — `online_mode` defaults `false`, and with it off **no web request is ever issued**; (4) **anonymous** — generic User-Agent `DiskSage/<version>`, no key, no cookies, no identifiers; (5) **advisory only** — output is display-only, never moves/deletes/gates a file.
- **Coverage gate:** `cargo llvm-cov --all-features --fail-under-lines 100` on Linux. Verify locally with the DEFAULT `cargo llvm-cov --lib --summary-only` (this codebase has `#[cfg(not(coverage))]` blocks — do NOT pass `--no-cfg-coverage`). All pure logic must be 100% line-covered. Run cargo from `src-tauri`.
- **Gating pattern:** FFI/network/Tauri-command code goes in `#[cfg(not(coverage))]` (and the LLM engine additionally behind `feature = "llm-engine"`). Pure logic stays gate-visible and 100% covered. Use compile-time `#[cfg(...)]`, not runtime `cfg!()`, for platform/feature arms so the absent arm is not a coverage gap.
- **No new dependencies.** `ureq` (`Cargo.toml:33`), `serde`, `serde_json` are already present.
- **Determinism:** no `HashMap` iteration in output; sort. `distinct_extensions` and insight lists are sorted.
- **Frontend check:** `npm run check` (0 errors) + `npm run build` clean, from repo root.

## File Structure

- `src-tauri/src/settings.rs` — CREATE. `Settings { online_mode: bool }` (+ `Default`), pure `parse_settings`/`serialize_settings`.
- `src-tauri/src/llm/prompt.rs` — MODIFY. Add `ExtReasoning` struct + `ext_reason_prompt`.
- `src-tauri/src/llm/parse.rs` — MODIFY. Add `parse_ext_reasoning`.
- `src-tauri/src/llm/mod.rs` — MODIFY. Add `reason_extension` + re-exports.
- `src-tauri/src/web/mod.rs` — CREATE. `WebLookup` trait + pure `ddg_query` + `parse_ddg_abstract`.
- `src-tauri/src/web/ureq_lookup.rs` — CREATE (`#[cfg(not(coverage))]`). `DdgLookup` (real `ureq`).
- `src-tauri/src/reasoning.rs` — CREATE. `ExtInsight`, `distinct_extensions`, `merge_insight`, `build_insights` (pure orchestration).
- `src-tauri/src/commands.rs` — MODIFY. `get_settings`/`set_settings`/`reason_unknown_extensions` (`#[cfg(not(coverage))]`).
- `src-tauri/src/lib.rs` — MODIFY. `mod settings; mod web; mod reasoning;` + register 3 commands.
- `src/lib/api.ts` — MODIFY. `Settings`, `ExtInsight` types + `getSettings`/`setSettings`/`reasonUnknownExtensions`.
- `src/lib/Settings.svelte` — CREATE. Online-mode toggle.
- `src/lib/Inventory.svelte` — MODIFY. Surface `ExtInsight`s under Unknown; mount Settings.

---

## Task 1: Settings core + persistence + commands

Behavior: a persisted `online_mode` flag, default `false`, stored as `settings.json` in `app_config_dir` (mirrors `bundled_ontology_ttl`). File-as-state — read on demand, no `AppState` field (matches how the ontology is re-read per call; simpler than the spec's draft `AppState` field, same result).

**Files:** Create `src-tauri/src/settings.rs`; Modify `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`, `src/lib/api.ts`.

**Interfaces:**
- Produces: `Settings { pub online_mode: bool }` (`serde::Serialize + Deserialize + Clone + Default`, default `online_mode:false`); `parse_settings(&str) -> Settings` (malformed → default); `serialize_settings(&Settings) -> String`; commands `get_settings(app) -> Result<Settings,String>`, `set_settings(online_mode: bool, app) -> Result<Settings,String>`.

- [ ] **Step 1: Write the failing tests** — create `src-tauri/src/settings.rs` with only the test module + a stub:

```rust
//! 사용자 설정(현재 online_mode 하나) — app_config_dir/settings.json에 영속. 파싱 실패는 안전측(offline) 기본값.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub online_mode: bool,
}

impl Default for Settings {
    fn default() -> Self { Settings { online_mode: false } }
}

/// JSON → Settings. 손상/부분 JSON은 기본값(offline)으로 fail-safe — 설정 파일이 앱을 깨지 않게.
pub fn parse_settings(json: &str) -> Settings {
    serde_json::from_str(json).unwrap_or_default()
}

/// Settings → JSON(영속용).
pub fn serialize_settings(s: &Settings) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_offline() {
        assert!(!Settings::default().online_mode);
    }
    #[test]
    fn parse_roundtrip() {
        let s = Settings { online_mode: true };
        assert_eq!(parse_settings(&serialize_settings(&s)), s);
    }
    #[test]
    fn parse_corrupt_is_default_offline() {
        assert_eq!(parse_settings("not json"), Settings::default());
        assert_eq!(parse_settings(""), Settings::default());
        // 부분 JSON(필드 없음)도 기본값
        assert_eq!(parse_settings("{}"), Settings { online_mode: false });
    }
    #[test]
    fn parse_explicit_true() {
        assert!(parse_settings(r#"{"online_mode":true}"#).online_mode);
    }
}
```

- [ ] **Step 2: Register the module + run — verify FAIL then PASS.** Add `mod settings;` to `src-tauri/src/lib.rs` (near the other `mod` decls, e.g. after `mod scanner;`). Run `cargo test --lib settings` from `src-tauri`. (The code above already satisfies the tests — this task's "failing" state is the missing module; once added it compiles and passes. If `cargo test --lib settings` PASSES immediately, that's expected here since the impl is inline with the tests.)

- [ ] **Step 3: Add the Tauri commands** to `commands.rs` (after `ontology_coherence`, ~line 235). These mirror `bundled_ontology_ttl`'s `app_config_dir` usage and `journal_file_path`'s `create_dir_all`:

```rust
#[cfg(not(coverage))]
fn settings_file_path(app: &AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

/// 현재 설정 조회. 파일 없으면 기본값(offline). 손상 파일은 parse_settings가 기본값으로 흡수.
#[cfg(not(coverage))]
#[tauri::command]
pub fn get_settings(app: AppHandle) -> Result<crate::settings::Settings, String> {
    let path = settings_file_path(&app)?;
    match std::fs::read_to_string(&path) {
        Ok(s) => Ok(crate::settings::parse_settings(&s)),
        Err(_) => Ok(crate::settings::Settings::default()),
    }
}

/// online_mode 설정 후 영속. 반환은 저장된 설정.
#[cfg(not(coverage))]
#[tauri::command]
pub fn set_settings(online_mode: bool, app: AppHandle) -> Result<crate::settings::Settings, String> {
    let s = crate::settings::Settings { online_mode };
    let path = settings_file_path(&app)?;
    std::fs::write(&path, crate::settings::serialize_settings(&s)).map_err(|e| e.to_string())?;
    Ok(s)
}
```
Register both in `lib.rs` `generate_handler!`: add `commands::get_settings, commands::set_settings,`.

- [ ] **Step 4: Run — PASS.** `cargo test --lib` (whole suite, from `src-tauri`). `cargo llvm-cov --lib --summary-only` → `settings.rs` **100%** (the commands are `#[cfg(not(coverage))]`, excluded). `cargo build --lib` compiles.

- [ ] **Step 5: Add api.ts settings wiring:**
```ts
export interface Settings { online_mode: boolean; }
export const getSettings = () => invoke<Settings>("get_settings");
export const setSettings = (online_mode: boolean) => invoke<Settings>("set_settings", { onlineMode: online_mode });
```
(Tauri maps snake_case Rust params to camelCase JS keys — `online_mode` → `onlineMode`.) Run `npm run check` → 0 errors.

- [ ] **Step 6: Commit** `git commit -m "feat(settings): persisted online_mode toggle (default offline)"`.

---

## Task 2: Offline LLM extension reasoning

Behavior: for a bare extension + the ontology's candidate class names, the local LLM returns a human-readable type + a suggested class (or none). Privacy: only the extension token reaches the LLM. Extends the exact `verdict/classify/summary` prompt+parse+orchestrate triad.

**Files:** Modify `src-tauri/src/llm/prompt.rs`, `src-tauri/src/llm/parse.rs`, `src-tauri/src/llm/mod.rs`.

**Interfaces:**
- Consumes: `InferenceEngine` (from `llm/mod.rs`).
- Produces: `ExtReasoning { pub type_desc: String, pub class: Option<String> }` (in `prompt.rs`, re-exported from `mod.rs`); `ext_reason_prompt(ext: &str, candidates: &[&str]) -> String`; `parse_ext_reasoning(raw: &str, candidates: &[&str]) -> Option<ExtReasoning>`; `reason_extension(engine: &dyn InferenceEngine, ext: &str, candidates: &[&str]) -> Option<ExtReasoning>`.

- [ ] **Step 1: Write the failing test for the prompt** — append to `llm/prompt.rs` tests, and add the type + fn:

```rust
    #[test]
    fn ext_reason_prompt_has_ext_and_schema_no_pii() {
        let p = ext_reason_prompt("fbx", &["Image", "Model3D"]);
        assert!(p.contains("fbx"));
        assert!(p.contains("Model3D"));
        assert!(p.contains(r#""type""#) && p.contains(r#""class""#));
        // 프라이버시: 확장자 토큰만 — 파일명/경로가 프롬프트에 섞이지 않음(이 프롬프트는 ext만 받음)
        assert!(!p.contains("/"));
    }
```
Add to `prompt.rs` (after `FileMeta`):
```rust
/// LLM 확장자 추론 결과. type_desc = "무슨 파일인가" 짧은 설명, class = 후보 중 제안(없으면 None).
#[derive(Debug, Clone, PartialEq)]
pub struct ExtReasoning {
    pub type_desc: String,
    pub class: Option<String>,
}
```
And the prompt fn (after `summary_prompt`):
```rust
/// 확장자 추론 프롬프트 — 확장자 토큰만(파일명/경로/내용 금지). 후보 중 하나 또는 'none'.
pub fn ext_reason_prompt(ext: &str, candidates: &[&str]) -> String {
    format!(
        "A file has extension \".{ext}\". Using ONLY the extension, say what kind of content it is, \
         and pick the best-fitting class id from the candidates, or \"none\".\n\
         Candidates: {list}\n\
         Reply with ONLY this JSON, no prose:\n\
         {{\"type\":\"<short type, e.g. '3D model'>\",\"class\":\"<one candidate id or none>\"}}",
        ext = ext, list = candidates.join(", ")
    )
}
```

- [ ] **Step 2: Run — the prompt test PASSES; add the failing parse test** to `llm/parse.rs` tests, then add `parse_ext_reasoning`:

```rust
    #[test]
    fn ext_reasoning_extracts_type_and_validates_class() {
        // class가 후보에 있으면 Some
        let r = parse_ext_reasoning(r#"{"type":"3D model","class":"Model3D"}"#, &["Model3D"]).unwrap();
        assert_eq!(r.type_desc, "3D model");
        assert_eq!(r.class.as_deref(), Some("Model3D"));
        // class가 후보 밖이면 type은 유지, class는 None(자유 생성 거부)
        let r2 = parse_ext_reasoning(r#"{"type":"3D model","class":"Nope"}"#, &["Model3D"]).unwrap();
        assert_eq!(r2.class, None);
        assert_eq!(r2.type_desc, "3D model");
        // class:"none" → None
        let r3 = parse_ext_reasoning(r#"{"type":"data","class":"none"}"#, &["Model3D"]).unwrap();
        assert_eq!(r3.class, None);
    }
    #[test]
    fn ext_reasoning_failure_paths_are_none() {
        assert!(parse_ext_reasoning("no json", &["X"]).is_none());       // extract None
        assert!(parse_ext_reasoning("{bad}", &["X"]).is_none());         // serde err
        assert!(parse_ext_reasoning(r#"{"class":"X"}"#, &["X"]).is_none()); // type 필드 없음 → None
        assert!(parse_ext_reasoning(r#"{"type":9}"#, &["X"]).is_none());  // type이 문자열 아님
    }
```
Add to `parse.rs` (import the type at top: `use crate::llm::{ExtReasoning, Verdict};` — replace the existing `use crate::llm::Verdict;`):
```rust
/// 확장자 추론 파싱. type 문자열이 있어야 Some; class는 후보에 있을 때만 Some(그 외/none은 None).
pub fn parse_ext_reasoning(raw: &str, candidates: &[&str]) -> Option<ExtReasoning> {
    let js = extract_json(raw)?;
    let v = serde_json::from_str::<serde_json::Value>(js).ok()?;
    let type_desc = v.get("type")?.as_str()?.to_string();
    let class = v.get("class").and_then(|c| c.as_str()).filter(|c| candidates.contains(c)).map(|c| c.to_string());
    Some(ExtReasoning { type_desc, class })
}
```

- [ ] **Step 3: Add the orchestrator + test** in `llm/mod.rs`. Append test:
```rust
    #[test]
    fn reason_extension_maps_llm_json() {
        let e = Fake(Ok(r#"{"type":"3D model","class":"Model3D"}"#.into()));
        let r = reason_extension(&e, "fbx", &["Model3D"]).unwrap();
        assert_eq!(r.type_desc, "3D model");
        assert_eq!(r.class.as_deref(), Some("Model3D"));
    }
    #[test]
    fn reason_extension_error_is_none() {
        let e = Fake(Err("no model".into()));
        assert!(reason_extension(&e, "fbx", &["Model3D"]).is_none());
    }
```
Add the fn (after `summarize_unknown`):
```rust
/// 확장자 하나를 추론(type + 제안 class). infer 실패·파싱 실패는 None.
pub fn reason_extension(engine: &dyn InferenceEngine, ext: &str, candidates: &[&str]) -> Option<ExtReasoning> {
    let out = engine.infer(&ext_reason_prompt(ext, candidates)).ok()?;
    parse_ext_reasoning(&out, candidates)
}
```
Update the re-exports in `mod.rs`:
- `pub use parse::{parse_class_pick, parse_ext_reasoning, parse_summary, parse_verdict, parse_verdict_full};`
- `pub use prompt::{classify_prompt, ext_reason_prompt, summary_prompt, verdict_prompt, ExtReasoning, FileMeta};`

- [ ] **Step 4: Run — PASS.** `cargo test --lib llm` then `cargo test --lib`. `cargo llvm-cov --lib --summary-only` → `prompt.rs`, `parse.rs`, `mod.rs` (llm) **100%**.

- [ ] **Step 5: Commit** `git commit -m "feat(llm): offline extension reasoning (type + suggested class)"`.

---

## Task 3: Web lookup layer (opt-in, anonymous, extension-only)

Behavior: a `WebLookup` trait; pure DDG query-builder + abstract-parser (100% covered); a `#[cfg(not(coverage))]` `ureq` impl that is the ONLY network egress. Sends only the bare extension.

**Files:** Create `src-tauri/src/web/mod.rs`, `src-tauri/src/web/ureq_lookup.rs`; Modify `src-tauri/src/lib.rs`.

**Interfaces:**
- Produces: `trait WebLookup { fn file_type(&self, ext: &str) -> Result<Option<String>, String>; }`; `ddg_query(ext: &str) -> String`; `parse_ddg_abstract(json: &str) -> Option<String>`; `DdgLookup` (gated).

- [ ] **Step 1: Write the failing tests** — create `src-tauri/src/web/mod.rs`:

```rust
//! opt-in 웹 조회 — 확장자 토큰만 전송(프라이버시). 순수 쿼리/파싱은 100% 측정, 실제 ureq는 cfg(not(coverage)).
#[cfg(not(coverage))]
mod ureq_lookup;
#[cfg(not(coverage))]
pub use ureq_lookup::DdgLookup;

/// 확장자 → 파일 타입 설명 조회 seam. 실패는 Err, "정보 없음"은 Ok(None).
pub trait WebLookup {
    fn file_type(&self, ext: &str) -> Result<Option<String>, String>;
}

/// DuckDuckGo Instant Answer 쿼리 URL. 확장자 토큰만 포함 — 파일명/경로 절대 금지.
pub fn ddg_query(ext: &str) -> String {
    format!(
        "https://api.duckduckgo.com/?q={ext}+file+format&format=json&no_html=1&no_redirect=1",
        ext = ext
    )
}

/// DDG 응답 JSON에서 AbstractText 추출. 빈 문자열/필드 없음/파싱 실패는 None.
pub fn parse_ddg_abstract(json: &str) -> Option<String> {
    let v = serde_json::from_str::<serde_json::Value>(json).ok()?;
    let s = v.get("AbstractText")?.as_str()?;
    if s.is_empty() { None } else { Some(s.to_string()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_contains_only_ext_token_no_pii() {
        let q = ddg_query("fbx");
        assert!(q.contains("fbx"));
        // 프라이버시: 확장자만 — 쿼리에 파일명/경로 구분자가 들어갈 여지 없음(이 함수는 ext만 받음)
        assert!(q.contains("api.duckduckgo.com"));
    }
    #[test]
    fn abstract_extracted_or_none() {
        assert_eq!(parse_ddg_abstract(r#"{"AbstractText":"Autodesk FBX is a 3D format."}"#),
                   Some("Autodesk FBX is a 3D format.".to_string()));
        assert_eq!(parse_ddg_abstract(r#"{"AbstractText":""}"#), None); // 빈 abstract
        assert_eq!(parse_ddg_abstract(r#"{"Heading":"x"}"#), None);     // 필드 없음
        assert_eq!(parse_ddg_abstract("not json"), None);              // 파싱 실패
        assert_eq!(parse_ddg_abstract(r#"{"AbstractText":5}"#), None); // 문자열 아님
    }
}
```

- [ ] **Step 2: Register + run FAIL→PASS.** Add `mod web;` to `lib.rs`. Run `cargo test --lib web` from `src-tauri` — compiles + passes (pure impl inline). NOTE: `web/mod.rs` references `mod ureq_lookup;` under `#[cfg(not(coverage))]` — create the file in Step 3 so the non-coverage build compiles.

- [ ] **Step 3: Create the gated `ureq` impl** `src-tauri/src/web/ureq_lookup.rs`:
```rust
//! 실제 DDG 조회(ureq) — 네트워크 egress 유일 지점. coverage 빌드서 제외. 익명 UA, 짧은 타임아웃.
use super::{ddg_query, parse_ddg_abstract, WebLookup};

pub struct DdgLookup;

impl WebLookup for DdgLookup {
    fn file_type(&self, ext: &str) -> Result<Option<String>, String> {
        // 익명: 일반 UA만, 쿠키/키/식별자 없음. 확장자 토큰만 쿼리에 포함.
        let ua = concat!("DiskSage/", env!("CARGO_PKG_VERSION"));
        let body = ureq::get(&ddg_query(ext))
            .header("User-Agent", ua)
            .call()
            .map_err(|e| e.to_string())?
            .body_mut()
            .read_to_string()
            .map_err(|e| e.to_string())?;
        Ok(parse_ddg_abstract(&body))
    }
}
```
(If the installed `ureq` 3.x response API differs, adapt the read to 3.x — `.into_body()`/`.read_to_string()`; the doc-comment + `parse_ddg_abstract` boundary are what matter. This file is typecheck-only in CI via `llm-engine-build`-style compilation and never coverage-measured.)

- [ ] **Step 4: Run — PASS.** `cargo test --lib` (from `src-tauri`); `cargo build --lib` (compiles the gated impl). `cargo llvm-cov --lib --summary-only` → `web/mod.rs` **100%** (`ureq_lookup.rs` is `#[cfg(not(coverage))]`, absent from the gate).

- [ ] **Step 5: Commit** `git commit -m "feat(web): opt-in anonymous DDG file-type lookup (extension-only)"`.

---

## Task 4: Merge orchestration + command + frontend

Behavior: extract distinct unknown extensions, reason offline per-ext, enrich per-ext only when online, merge into advisory `ExtInsight`s; surface under Unknown with a settings toggle.

**Files:** Create `src-tauri/src/reasoning.rs`; Modify `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`, `src/lib/api.ts`; Create `src/lib/Settings.svelte`; Modify `src/lib/Inventory.svelte`.

**Interfaces:**
- Consumes: `ExtReasoning` (Task 2), `WebLookup` (Task 3), `Settings` (Task 1).
- Produces: `ExtInsight { ext: String, type_desc: Option<String>, suggested_class: Option<String>, source: String }` (serde Serialize); `distinct_extensions(samples: &[String]) -> Vec<String>`; `merge_insight(ext: &str, llm: Option<ExtReasoning>, web: Option<String>) -> ExtInsight`; `build_insights(exts: &[String], reason: &dyn Fn(&str) -> Option<ExtReasoning>, web: Option<&dyn Fn(&str) -> Option<String>>) -> Vec<ExtInsight>`; command `reason_unknown_extensions(samples: Vec<String>, app) -> Result<Vec<ExtInsight>, String>`.

- [ ] **Step 1: Write failing tests** — create `src-tauri/src/reasoning.rs`:

```rust
//! 미분류 확장자 추론 병합 — 오프라인 LLM 결과 + (opt-in) 웹 결과를 자문용 ExtInsight로. 순수·100% 측정.
use crate::llm::ExtReasoning;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ExtInsight {
    pub ext: String,
    pub type_desc: Option<String>,
    pub suggested_class: Option<String>,
    pub source: String, // "llm" | "web" | "both" | "none"
}

/// 경로 표본에서 서로 다른 확장자(소문자) 추출 — 정렬·중복 제거. 확장자 없는 경로는 무시.
pub fn distinct_extensions(samples: &[String]) -> Vec<String> {
    let mut exts: Vec<String> = samples
        .iter()
        .filter_map(|p| std::path::Path::new(p).extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()))
        .collect();
    exts.sort();
    exts.dedup();
    exts
}

/// 한 확장자의 LLM/웹 결과를 병합. type_desc는 웹 우선(있으면), 없으면 LLM. class는 LLM만 제안.
pub fn merge_insight(ext: &str, llm: Option<ExtReasoning>, web: Option<String>) -> ExtInsight {
    let (llm_type, suggested_class) = match &llm {
        Some(r) => (
            if r.type_desc.is_empty() { None } else { Some(r.type_desc.clone()) },
            r.class.clone(),
        ),
        None => (None, None),
    };
    let source = match (llm.is_some(), web.is_some()) {
        (true, true) => "both",
        (false, true) => "web",
        (true, false) => "llm",
        (false, false) => "none",
    }.to_string();
    let type_desc = web.or(llm_type); // 웹 우선
    ExtInsight { ext: ext.to_string(), type_desc, suggested_class, source }
}

/// 확장자별 오프라인 추론 + (online일 때만) 웹 조회 병합. web=None이면 웹 분기 절대 미실행(default offline).
pub fn build_insights(
    exts: &[String],
    reason: &dyn Fn(&str) -> Option<ExtReasoning>,
    web: Option<&dyn Fn(&str) -> Option<String>>,
) -> Vec<ExtInsight> {
    exts.iter()
        .map(|ext| {
            let llm = reason(ext);
            let w = web.and_then(|f| f(ext));
            merge_insight(ext, llm, w)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_extensions_lowercased_sorted_deduped() {
        let s = vec!["/a/x.FBX".into(), "/b/y.fbx".into(), "/c/z.parquet".into(), "/d/noext".into()];
        assert_eq!(distinct_extensions(&s), vec!["fbx".to_string(), "parquet".to_string()]);
    }
    #[test]
    fn merge_prefers_web_type_keeps_llm_class() {
        let llm = Some(ExtReasoning { type_desc: "3D model".into(), class: Some("Model3D".into()) });
        let ins = merge_insight("fbx", llm, Some("Autodesk FBX 3D format".into()));
        assert_eq!(ins.type_desc.as_deref(), Some("Autodesk FBX 3D format")); // 웹 우선
        assert_eq!(ins.suggested_class.as_deref(), Some("Model3D"));
        assert_eq!(ins.source, "both");
    }
    #[test]
    fn merge_llm_only_and_web_only_and_none() {
        let llm = Some(ExtReasoning { type_desc: "data".into(), class: None });
        assert_eq!(merge_insight("dat", llm, None).source, "llm");
        assert_eq!(merge_insight("dat", None, Some("desc".into())).source, "web");
        let none = merge_insight("dat", None, None);
        assert_eq!(none.source, "none");
        assert_eq!(none.type_desc, None);
    }
    #[test]
    fn build_insights_offline_never_calls_web() {
        // web=None → 웹 클로저가 없으므로 호출 자체가 불가능(프라이버시: default offline)
        let reason = |e: &str| Some(ExtReasoning { type_desc: format!("t-{e}"), class: None });
        let out = build_insights(&["fbx".into()], &reason, None);
        assert_eq!(out[0].source, "llm");
        assert_eq!(out[0].type_desc.as_deref(), Some("t-fbx"));
    }
    #[test]
    fn build_insights_online_receives_only_ext_token() {
        // 프라이버시: 웹 클로저에 넘어오는 값은 확장자 토큰뿐(경로 구분자 없음)
        let reason = |_: &str| None;
        let web = |e: &str| { assert!(!e.contains('/') && !e.contains('.')); Some(format!("web-{e}")) };
        let out = build_insights(&["parquet".into()], &reason, Some(&web));
        assert_eq!(out[0].source, "web");
        assert_eq!(out[0].type_desc.as_deref(), Some("web-parquet"));
    }
}
```

- [ ] **Step 2: Register + run FAIL→PASS.** Add `mod reasoning;` to `lib.rs`. Run `cargo test --lib reasoning` from `src-tauri` — compiles + passes.

- [ ] **Step 3: Add the command** to `commands.rs` (after `summarize_unknown_bucket`). It reuses the **exact** engine lazy-init pattern from `file_verdicts` (`commands.rs:525-541`) — helpers `model_file_path(&dir)` (`:440`) and `model_status_for(&path)` (`:445`) already exist; lock the engine once and use `guard.as_ref()`. The web branch is **independent of the `llm-engine` feature** (web enrichment works even with no LLM):
```rust
/// 미분류 확장자 자문 추론. samples = InventoryReport.unknown_samples(경로). online_mode일 때만 웹 조회.
/// LLM은 feature+모델 있을 때만; 웹은 online_mode일 때만(feature 무관). 둘 다 없으면 source="none".
#[cfg(not(coverage))]
#[cfg_attr(not(feature = "llm-engine"), allow(unused_variables))]
#[tauri::command(async)]
pub fn reason_unknown_extensions(
    samples: Vec<String>,
    app: AppHandle,
    state: State<AppState>,
) -> Result<Vec<crate::reasoning::ExtInsight>, String> {
    let exts = crate::reasoning::distinct_extensions(&samples);
    let onto = load_ontology_from(&bundled_ontology_ttl(&app)?)?;
    let candidates: Vec<String> = onto.classes.iter()
        .map(|c| c.id.rsplit(['#', '/']).next().unwrap_or(&c.id).to_string()).collect();
    let cand_refs: Vec<&str> = candidates.iter().map(|s| s.as_str()).collect();

    // opt-in 웹: online_mode일 때만 DdgLookup, 아니면 None → build_insights의 웹 분기 절대 미실행(default offline)
    let settings = get_settings(app.clone())?;
    let ddg = crate::web::DdgLookup;
    let web_fn = |ext: &str| -> Option<String> { crate::web::WebLookup::file_type(&ddg, ext).ok().flatten() };
    let web: Option<&dyn Fn(&str) -> Option<String>> = if settings.online_mode { Some(&web_fn) } else { None };

    // 오프라인 LLM(feature+모델+엔진 있으면 실제; 그 블록에서 반환). 없으면 아래 fallback로 낙하.
    #[cfg(feature = "llm-engine")]
    {
        use tauri::Manager;
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        if model_status_for(&model_file_path(&dir)).present {
            let mut guard = state.engine.lock().unwrap();
            if guard.is_none() {
                if let Ok(e) = crate::llm::LlamaEngine::new(&model_file_path(&dir)) {
                    *guard = Some(e);
                }
            }
            if let Some(engine) = guard.as_ref() {
                let reason = |ext: &str| crate::llm::reason_extension(engine, ext, &cand_refs);
                return Ok(crate::reasoning::build_insights(&exts, &reason, web));
            }
        }
    }

    // fallback: LLM 없음(feature off/모델 없음/init 실패) — reason은 항상 None, 웹은 위 settings대로 적용
    let reason = |_: &str| -> Option<crate::llm::ExtReasoning> { None };
    Ok(crate::reasoning::build_insights(&exts, &reason, web))
}
```
Register in `lib.rs` `generate_handler!`: `commands::reason_unknown_extensions,`.

- [ ] **Step 4: api.ts** — add:
```ts
export interface ExtInsight { ext: string; type_desc: string | null; suggested_class: string | null; source: string; }
export const reasonUnknownExtensions = (samples: string[]) =>
  invoke<ExtInsight[]>("reason_unknown_extensions", { samples });
```

- [ ] **Step 5: Settings.svelte** — create a minimal toggle mirroring the model-download capability UI in `Inventory.svelte`:
```svelte
<script lang="ts">
  import { getSettings, setSettings } from "./api";
  let online = $state(false);
  let busy = $state(false);
  $effect(() => { getSettings().then((s) => (online = s.online_mode)).catch(() => {}); });
  async function toggle() {
    busy = true;
    try { const s = await setSettings(!online); online = s.online_mode; } catch {} finally { busy = false; }
  }
</script>
<label class="setting">
  <input type="checkbox" checked={online} disabled={busy} onchange={toggle} />
  온라인 모드(미분류 확장자 웹 조회 — 확장자 토큰만 전송, 기본 꺼짐)
</label>
```

- [ ] **Step 6: Inventory.svelte** — after the report loads, fetch insights for the unknown samples and render them advisory (like the coherence list). Add to the script: `let insights = $state<ExtInsight[]>([]);` and in the load path (after `report` is set), `reasonUnknownExtensions(report.unknown_samples).then((r) => (insights = r)).catch(() => {});`; import `reasonUnknownExtensions`, `ExtInsight`, and mount `<Settings />`. In the Unknown section render each insight: `{i.ext}` → `{i.type_desc ?? "?"}` and, when `i.suggested_class`, a "→ {i.suggested_class}" advisory hint. Non-blocking; gates nothing.

- [ ] **Step 7: Verify** `cargo build --lib`, `cargo test --lib` (all pass), `cargo llvm-cov --lib --summary-only` (`reasoning.rs` **100%**; command is `#[cfg(not(coverage))]`), `npm run check` + `npm run build` clean.

- [ ] **Step 8: Commit** `git commit -m "feat(reasoning): unknown-extension insights (offline LLM + opt-in web) surfaced in Inventory"`.

---

## Post-implementation

- Whole-branch review (most capable model) focused on the **privacy Global Constraints**: (1) confirm no code path sends filename/path/contents to the web — only the extension token (trace `ddg_query` + `build_insights`' web closure); (2) confirm `online_mode=false` makes the web branch unreachable (`build_insights` `web=None`); (3) confirm the LLM still receives only the extension token (`ext_reason_prompt`); (4) determinism (sorted `distinct_extensions`, no HashMap in output); (5) 100% coverage on the Linux gate for all pure logic; (6) no new dependency in `Cargo.toml`.
- Open the PR on `feat/llm-web-reasoning` (spec committed `dfdd2a5`). Merge requires the org bot review / explicit user authorization (see project memory).
- Sub-project **A** (user-defined rule-based classification) remains a separate spec→plan→implement cycle.
