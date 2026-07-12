# M6 — Packaging / CI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development for the coverage-gated Rust task; the CI/packaging tasks are verified by CI runs (a PR-triggered build-check), not local unit tests.

**Goal:** On a release tag, CI automatically builds installable artifacts for all three OSes — MSI/NSIS (Windows), dmg (macOS), AppImage + deb (Linux) — with the embedded LLM, plus macOS safety hardening and a cross-platform confirm dialog. Meets M6's completion criterion: "3 OS 아티팩트 자동 빌드."

**Architecture:** A tag-triggered `release.yml` matrix workflow runs `tauri build` per OS and publishes artifacts to a GitHub Release. A PR-triggered **build-check** job proves the packaging pipeline (build artifacts, no publish) so the release path is verified before any tag. Release builds enable `--features llm-engine` (real llama.cpp). GPU backends are layered on top per-OS (Metal on macOS-arm; CUDA+Vulkan on Win/Linux) — gated so a CPU-only release still satisfies the completion criterion if GPU toolchain setup proves too costly on CI.

**Tech Stack:** Tauri 2 bundler, `tauri-apps/tauri-action` (SHA-pinned), GitHub Actions matrix, `@tauri-apps/plugin-dialog`, existing `llama-cpp-2` feature.

## Global Constraints

- **Coverage gate** (org CI) stays 100% lines on Linux — only Task 1 (macOS denied prefixes in `safety.rs`) adds measured code; it must be 100% and must NOT break the existing `is_protected` tests. macOS-specific prefixes are `#[cfg(target_os = "macos")]`; keep test assertions platform-appropriate (a macOS-only path assertion under `#[cfg(target_os="macos")]`, cross-platform assertions ungated) so the Linux gate covers what it compiles.
- **No secrets invented**: the release job uses the built-in `GITHUB_TOKEN`; code signing (macOS notarization / Windows Authenticode) is explicitly deferred per spec §10 ("1.0 이전 과제") — artifacts are unsigned. Do not add signing secrets.
- **SHA-pinned actions** (org Scorecard): every `uses:` in new workflows is pinned to a full commit SHA, matching the existing `test.yml` style.
- **No outward-facing release without confirmation**: authoring `release.yml` and the PR build-check is safe; pushing a real version TAG (which publishes a public GitHub Release) is an outward-facing action to be left for the user to trigger.
- **CPU-first**: the release pipeline must produce working artifacts with CPU llama.cpp alone. GPU backends are additive; if a GPU build step fails or is too costly on CI, the CPU artifacts still meet the milestone.

---

## Task 1: macOS safety hardening (denied prefixes)

**Files:** Modify `src-tauri/src/safety.rs`

**Why:** `is_protected`'s `#[cfg(unix)]` denied-prefix list (`/usr /etc /bin /sbin /lib /boot /proc /sys /dev`) omits macOS system locations. On macOS a move/trash targeting `/System`, `/Library`, `/Applications`, `/private`, `/Volumes`, `/cores`, `/Network` must be rejected by the safety layer.

- [ ] **Step 1: Failing test** (in safety.rs tests, `#[cfg(target_os = "macos")]`):

```rust
    #[cfg(target_os = "macos")]
    #[test]
    fn protects_macos_system_paths() {
        for p in ["/System", "/System/Library", "/Library", "/Applications",
                  "/private/etc", "/Volumes/Macintosh HD", "/cores", "/Network"] {
            assert!(is_protected(Path::new(p)), "{p} must be protected");
        }
    }
```

- [ ] **Step 2:** `cargo test --lib safety` on macOS would FAIL — but this repo's gate is Linux, so instead verify the LOGIC by adding the macОS prefixes to the `#[cfg(unix)]` block guarded by `#[cfg(target_os="macos")]`, OR extend the unix list conditionally. Implementation:

```rust
    #[cfg(unix)]
    {
        let mut denied_prefixes: Vec<&str> =
            vec!["/usr", "/etc", "/bin", "/sbin", "/lib", "/boot", "/proc", "/sys", "/dev"];
        #[cfg(target_os = "macos")]
        denied_prefixes.extend_from_slice(&["/System", "/Library", "/Applications", "/private", "/Volumes", "/cores", "/Network"]);
        // ... existing prefix-match loop, unchanged ...
    }
```
Keep the existing prefix-match loop. The `#[cfg(target_os="macos")]` `extend_from_slice` line is compiled out on Linux (so the Linux gate neither runs nor needs to cover it); the macOS-only test covers it on macOS. The existing cross-platform `/usr` etc. tests keep the shared loop at 100% on the Linux gate.

- [ ] **Step 3:** `cargo test --lib` (Linux/local) → existing safety tests pass unchanged; `cargo llvm-cov --lib --summary-only` → `safety.rs` line coverage unchanged (the macOS `extend` line is `cfg`-absent on Linux, exactly like the existing `#[cfg(windows)]` env-fallback lines). Confirm no Linux-gate regression.

- [ ] **Step 4: Commit** — `git commit -m "feat(safety): macOS system-path denylist (/System, /Library, ...)"`.

---

## Task 2: Cross-platform confirm dialog (fix macOS wry `window.confirm`)

**Files:** `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs`, `src-tauri/capabilities/*.json`, `package.json`, the destructive-confirm call sites in `src/lib/*.svelte`.

**Why:** wry on macOS does not implement JS `window.confirm()` — the destructive-op confirmation (§7-6) silently returns/blocks there. Use Tauri's dialog plugin instead.

- [ ] **Step 1:** Add the plugin — Rust `tauri-plugin-dialog = "2"` in `src-tauri/Cargo.toml`; JS `@tauri-apps/plugin-dialog` in `package.json`. Register `.plugin(tauri_plugin_dialog::init())` in `lib.rs`'s `run()` (inside the existing `#[cfg(not(coverage))]` builder). Add `dialog:default` to the app capabilities JSON (`src-tauri/capabilities/default.json`).
- [ ] **Step 2:** Replace `window.confirm(...)` / `confirm(...)` in the destructive flows (Organize.svelte's move-confirm, any Cleanup/Duplicates confirm) with `import { confirm } from "@tauri-apps/plugin-dialog"; await confirm(message, { title, kind: "warning" })`. Keep the exact wording (§7-6: mentions undo, not trash, for moves).
- [ ] **Step 3:** `npm run check` (0 errors), `npm run build` (succeeds). The dialog is GUI — verified manually / by the build, not unit tests. `cargo test --lib` unaffected; `cargo build --lib` compiles the plugin.
- [ ] **Step 4: Commit** — `git commit -m "fix(ui): use tauri dialog plugin for destructive confirm (macOS wry lacks window.confirm)"`.

---

## Task 3: Release workflow + PR build-check (3-OS artifacts)

**Files:** Create `.github/workflows/release.yml`. Possibly tweak `src-tauri/tauri.conf.json` bundle targets.

**Why:** the completion criterion. `tauri.conf.json` already has `bundle.targets: "all"` and icons — the workflow just needs to run `tauri build` per OS and collect artifacts.

- [ ] **Step 1:** Author `release.yml` with two entry points:
  - `on: push: tags: ["v*"]` → publish a GitHub Release with artifacts (uses `GITHUB_TOKEN`).
  - `on: pull_request` (paths: the workflow, `src-tauri/**`, `src/**`) OR `workflow_dispatch` → **build-check**: build artifacts on all three OSes but do NOT publish (proves the pipeline without a release).
- [ ] **Step 2:** Matrix over `windows-latest`, `macos-latest` (Apple Silicon), `ubuntu-22.04`. Steps (all `uses:` SHA-pinned like `test.yml`): checkout → Node 20 setup → `npm ci` → Rust toolchain → (ubuntu) install Tauri GTK deps + `cmake clang libclang-dev` (llama.cpp) → `tauri-apps/tauri-action` with `args: "--features llm-engine"` (CPU llama.cpp). On tag: set `tagName`/`releaseName`, `GITHUB_TOKEN` env, and let tauri-action create the release + upload MSI/dmg/AppImage/deb. On PR/dispatch: run the same build with no release inputs (artifacts stay as workflow build output; optionally `actions/upload-artifact` for inspection).
- [ ] **Step 3:** `permissions: contents: write` on the release job (needs to create releases); `contents: read` on the build-check.
- [ ] **Step 4: Verify via CI** — open the M6 PR; the PR build-check job must build artifacts green on all three OSes (this is the real verification — cannot be done locally). Iterate on missing system deps (mirroring `test.yml`/the M5 `llm-engine-build` job) until green.
- [ ] **Step 5: Commit** — `git commit -m "ci: release workflow + PR build-check for 3-OS artifacts (MSI/dmg/AppImage+deb)"`.

---

## Task 4: GPU backends in release builds (additive; CPU-first fallback)

**Files:** `.github/workflows/release.yml` (per-OS feature flags + toolkit setup).

**Why:** spec §10 — macOS Metal, Windows/Linux CUDA+Vulkan, dynamic backend selection at runtime.

- [ ] **Step 1:** Per-OS release feature flags:
  - macOS-arm: `--features llm-engine` (llama-cpp-2 auto-enables `metal` on `aarch64-apple-darwin` — no extra toolkit; verify the build pulls Metal).
  - Windows/Linux: `--features "llm-engine,llama-cpp-2/cuda,llama-cpp-2/vulkan"`. Install CUDA toolkit (`Jimver/cuda-toolkit`, SHA-pinned) + the Vulkan SDK on those runners. llama.cpp compiled with both backends selects at runtime (ggml backend registry) — this realizes the "동적 로딩" + CUDA→Vulkan→CPU auto-detect from §6.
- [ ] **Step 2:** Keep this **additive and non-blocking**: gate the GPU feature args behind a matrix flag so the CPU build path remains available. If the CUDA toolkit step is too slow/costs too much on hosted runners, document it (`log`/comment) and ship CPU artifacts — the milestone criterion is still met. **Escalate to the human** before committing to expensive always-on CUDA CI: note the added build minutes and ask whether to (a) build GPU on every release, (b) build GPU only on demand, or (c) ship CPU-first and add GPU later.
- [ ] **Step 3: Verify via CI** on the M6 PR build-check (GPU matrix entries). Iterate; if a GPU backend can't build on hosted runners, fall back to CPU for that OS and record why.
- [ ] **Step 4: Commit** — `git commit -m "ci: bundle GPU backends in release builds (Metal / CUDA+Vulkan) with CPU fallback"`.

---

## Post-implementation

- Whole-branch review focused on: the macOS denylist correctness + coverage neutrality on the Linux gate, the dialog plugin wiring/capabilities, and the workflow (SHA-pinned actions, `GITHUB_TOKEN`-only, no invented secrets, correct permissions).
- Open the M6 PR; the PR build-check verifies 3-OS packaging. Do NOT push a version tag (public release) — leave the first real release for the user to trigger once the build-check is green.
- Deferred (post-1.0, spec §10): code signing / notarization, Tauri updater.
