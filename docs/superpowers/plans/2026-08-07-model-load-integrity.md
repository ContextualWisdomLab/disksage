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

- [x] **Step 1: Write the failing tests**

Create a temporary exact fixture and assert that the initial placeholder verifier rejects neither a same-size wrong digest nor a symbolic link. Add cases for missing paths, directories, short files, oversized files, same-size digest drift, uppercase expected digests, and exact valid bytes. All assertions use stable path-free codes.

- [x] **Step 2: Run the focused test and verify RED**

The source-controlled RED commits `ef07e8f80dc1c9812582a6d2684ac7afc6c39edc` and `70bb8912e01e2a1b82bebc5ed058ccce84a59d60` deliberately leave the verifier as `Ok(())`, so missing/non-regular/size/digest/read and engine-binding contracts fail by construction. Repository CI is the durable execution evidence; predecessor or synthetic-merge results are not reused.

- [x] **Step 3: Implement the minimum bounded verifier**

`d9b102f3a7e93adcd72ae6c24e64391abe3e6969` uses `symlink_metadata`, read-only `File::open`, handle metadata, a fixed 64 KiB buffer, exact checked byte count, byte-by-byte lowercase SHA-256 encoding, case-insensitive digest comparison, and stable path-free error codes.

- [ ] **Step 4: Run focused tests and verify GREEN**

Require fresh exact-current-head repository Test evidence. Pending, queued, cancelled, skipped-required, failed, synthetic-merge, or predecessor-head results are not GREEN.

- [x] **Step 5: Commit**

Commit: `d9b102f3a7e93adcd72ae6c24e64391abe3e6969` (`security(model): verify installed artifact bytes`).

### Task 2: Bind engine construction to verified bytes

**Files:**
- Modify: `src-tauri/src/llm/engine.rs`
- Test: `src-tauri/src/llm/installed_model.rs`

**Interfaces:**
- Consumes: `verify_installed_model(&DEFAULT, model_path)`.
- Produces: llama.cpp is never initialized before the exact pinned artifact passes verification.

- [x] **Step 1: Write the failing source-binding test**

`70bb8912e01e2a1b82bebc5ed058ccce84a59d60` requires the pinned-default verifier call to occur textually before both backend initialization and model loading, and rejects multiple/alternate verifier calls.

- [x] **Step 2: Run the focused test and verify RED**

At that RED head, `engine.rs` had no `verify_installed_model` call, so the source-binding contract is intentionally unsatisfied.

- [x] **Step 3: Add the fail-closed engine gate**

`db511300d0d0116dc158255d2658091cccd277cd` places `super::installed_model::verify_installed_model(&super::model::DEFAULT, model_path)?;` before `LlamaBackend::init()` and `LlamaModel::load_from_file` without introducing an alternate constructor.

- [ ] **Step 4: Verify focused and real feature compilation**

Require fresh exact-current-head Test evidence including the `llm-engine-build` job. No live model download is needed.

- [x] **Step 5: Commit**

Commit: `db511300d0d0116dc158255d2658091cccd277cd` (`security(model): verify GGUF before llama load`).

### Task 3: Persist acquisition and rollback evidence

**Files:**
- Modify: `docs/doctoring/model-artifact-integrity.md`
- Modify: `CHANGELOG.md`
- Modify: `docs/superpowers/plans/2026-08-07-model-load-integrity.md`

**Interfaces:**
- Consumes: the exact verifier and engine gate from Tasks 1–2.
- Produces: durable threat-model, migration, rollback, local-versus-shareable evidence, and stack-order documentation.

- [x] **Step 1: Add a failing documentation contract test**

`70bb8912e01e2a1b82bebc5ed058ccce84a59d60` requires the authoritative doctoring to distinguish download admission from load-time verification, state that file existence is not integrity evidence, list all stable codes, preserve the 64 KiB bound and privacy boundary, and bind a changelog entry.

- [x] **Step 2: Run and verify RED**

The parent doctoring lacked the required load-time section and changelog wording at the RED head, so the deterministic documentation contract is intentionally unsatisfied there.

- [x] **Step 3: Update doctoring and changelog**

`0dd4577fed0706e1ff8103411e86c15772ca03f5` records load-time threat model, migration, rollback, privacy, standards mapping, and standalone/MSA compatibility. `66a32525a58e9e12de213aeaf26d2bd3e25683a1` records the release-facing Security entry. APA 7th evidence reuses the parent slice's August 7, 2026 authoritative-source validation without claiming certification or legal clearance.

- [ ] **Step 4: Run complete exact-head validation**

Require repository Test, Release, Security Scan, SAST, Rust coverage, frontend coverage, packaging, provenance, review, approval, branch-protection, and release-acceptance evidence on the exact current head after the parent is integrated and this stack is retargeted. Treat pending, cancelled, skipped-required, absent, failed, stale-head, or synthetic-merge evidence as not passing.

- [x] **Step 5: Commit and retain Draft stacked PR**

PR #142 remains Draft and based on `security/model-download-stream-integrity`; retarget to `main` only after #141 is integrated without discarding or duplicating parent commits.
