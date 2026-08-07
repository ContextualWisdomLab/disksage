# Model Load Integrity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent DiskSage from passing an unverified, pre-positioned, truncated, or tampered GGUF file to llama.cpp.

**Architecture:** Add a focused Rust verifier beside the model installer. It opens the installed artifact read-only, rejects symbolic links and non-regular files, enforces the pinned byte count, computes SHA-256 through a fixed 64 KiB buffer, and returns privacy-safe stable codes. The feature-gated engine must call this verifier before initializing llama.cpp; the existing download verifier remains the mutation-time admission control.

**Tech Stack:** Rust 1.97, `sha2`, standard-library filesystem and buffered I/O, `llama-cpp-2`, Cargo tests.

## Global Constraints

- Preserve standalone operation and the existing Naruon/contextual-orchestrator/CWL MSA boundary.
- Add no network, database, tenant, scheduler, GitHub secret, or filesystem mutation authority.
- Keep every production branch covered and every public surface documented for beginners.
- Return stable path-free errors; never include local paths, model bytes, or dynamic OS/llama diagnostics.
- Keep PR #141 as the parent stack and keep this pull request Draft until the parent is integrated and fresh exact-head gates pass.

---

### Task 1: Add the installed-artifact verification contract

**Files:**
- Create: `src-tauri/src/llm/installed_model.rs`
- Modify: `src-tauri/src/llm/mod.rs`

**Interfaces:**
- Consumes: `super::model::ModelSpec`.
- Produces: `pub(crate) fn verify_installed_model(spec: &ModelSpec, path: &Path) -> Result<(), String>`.

- [ ] **Step 1: Write the failing tests**

Create a temporary exact fixture and assert that the initial placeholder verifier rejects neither a same-size wrong digest nor a symbolic link. Add cases for missing paths, directories, short files, oversized files, same-size digest drift, uppercase expected digests, and exact valid bytes. All assertions use stable path-free codes.

- [ ] **Step 2: Run the focused test and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml installed_model`

Expected: FAIL because the placeholder verifier incorrectly accepts invalid artifacts.

- [ ] **Step 3: Implement the minimum bounded verifier**

Use `symlink_metadata` to reject links and non-regular entries, open the file read-only, use handle metadata for the authoritative exact length, stream through `[u8; 64 * 1024]`, compute lowercase SHA-256 byte-by-byte, and compare case-insensitively. Map every failure to one of these stable codes: `model-installed-unavailable`, `model-installed-not-regular`, `model-installed-size-mismatch`, `model-installed-read-failed`, `model-installed-digest-mismatch`.

- [ ] **Step 4: Run focused tests and verify GREEN**

Run: `cargo test --manifest-path src-tauri/Cargo.toml installed_model`

Expected: PASS with every verifier branch covered.

- [ ] **Step 5: Commit**

Commit message: `security(model): verify installed artifact bytes`

### Task 2: Bind engine construction to verified bytes

**Files:**
- Modify: `src-tauri/src/llm/engine.rs`
- Test: `src-tauri/src/llm/installed_model.rs`

**Interfaces:**
- Consumes: `verify_installed_model(&DEFAULT, model_path)`.
- Produces: llama.cpp is never initialized before the exact pinned artifact passes verification.

- [ ] **Step 1: Write the failing source-binding test**

Read `engine.rs` from `CARGO_MANIFEST_DIR` and require the verifier call to occur textually before both `LlamaBackend::init()` and `LlamaModel::load_from_file`. Reject dynamic model-spec substitution and any verification placed after backend/model initialization.

- [ ] **Step 2: Run the focused test and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml engine_requires_verified_model_before_llama_initialization`

Expected: FAIL because `LlamaEngine::new` currently initializes llama.cpp without a pinned-byte verification call.

- [ ] **Step 3: Add the fail-closed engine gate**

At the beginning of `LlamaEngine::new`, call `super::installed_model::verify_installed_model(&super::model::DEFAULT, model_path)?;`. Do not weaken the existing feature gate or introduce an alternate unverified constructor.

- [ ] **Step 4: Verify focused and real feature compilation**

Run:
- `cargo test --manifest-path src-tauri/Cargo.toml installed_model`
- `cargo build --manifest-path src-tauri/Cargo.toml --features llm-engine`

Expected: both pass; no model download is performed.

- [ ] **Step 5: Commit**

Commit message: `security(model): verify GGUF before llama load`

### Task 3: Persist acquisition and rollback evidence

**Files:**
- Modify: `docs/doctoring/model-artifact-integrity.md`
- Modify: `CHANGELOG.md`
- Modify: `docs/superpowers/plans/2026-08-07-model-load-integrity.md`

**Interfaces:**
- Consumes: the exact verifier and engine gate from Tasks 1–2.
- Produces: durable threat-model, migration, rollback, local-versus-shareable evidence, and stack-order documentation.

- [ ] **Step 1: Add a failing documentation contract test**

Require the doctoring record to distinguish download-time admission from load-time verification, explain why file existence is not integrity evidence, list stable errors, document the 64 KiB bounded read, state that no bytes or paths become shareable evidence, and specify rollback without bypassing verification.

- [ ] **Step 2: Run and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml installed_model_documentation_is_durable`

Expected: FAIL until authoritative doctoring and changelog entries exist.

- [ ] **Step 3: Update doctoring and changelog**

Record NIST SP 800-218/800-218A, SLSA 1.2, OWASP Top 10:2025 A03/A08, exact Qwen artifact/license evidence, standalone/MSA compatibility, performance boundary, migration for pre-existing valid files, and reviewed rollback. Use APA 7th references already validated in the parent slice; do not claim certification or legal clearance.

- [ ] **Step 4: Run complete exact-head validation**

Run repository Test, Release, Security Scan, SAST, Rust coverage, frontend coverage, packaging, provenance, and release-acceptance gates. Treat pending, cancelled, skipped-required, absent, failed, stale-head, or synthetic-merge evidence as not passing.

- [ ] **Step 5: Commit and open a Draft stacked PR**

Commit message: `docs(model): record load-time integrity boundary`

Open the pull request against `security/model-download-stream-integrity`, retain Draft status, and retarget to `main` only after PR #141 is integrated.