# Model Load Integrity Implementation Plan

> **For agentic workers:** Use test-first implementation and exact-head verification. This plan records the load-integrity slice after its parent model-download hardening was squash-merged into protected `main`.

**Goal:** Prevent DiskSage from passing an unverified, pre-positioned, truncated, linked, or tampered GGUF file to llama.cpp.

**Architecture:** A focused Rust verifier beside the model installer re-opens the installed artifact read-only, rejects symbolic links and non-regular files, enforces the immutable pinned byte count, computes SHA-256 through a fixed 64 KiB buffer, and returns privacy-safe stable codes. The feature-gated engine calls the verifier before initializing llama.cpp. Download admission and load-time trust remain separate controls.

**Tech Stack:** Rust, `sha2`, standard-library filesystem and bounded I/O, `llama-cpp-2`, Cargo tests.

## Integration status

The original stacked PR was based on a pre-squash snapshot of the model-download parent. After that parent integrated into protected `main`, the child history diverged because squash merge intentionally did not preserve the parent branch ancestry. This replacement slice is rebuilt from the integrated parent head so it contains only load-time verification work and does not overwrite the parent's newer unnamed-staging and destination-identity hardening.

## Global constraints

- Preserve standalone operation and existing Naruon/contextual-orchestrator/CWL MSA boundaries.
- Add no network, database, tenant, scheduler, GitHub-secret, or filesystem mutation authority.
- Keep production arithmetic and integrity enforcement in Rust.
- Return stable path-free errors; never include local paths, model bytes, or dynamic OS/llama diagnostics.
- Treat exact-head CI, security, coverage, packaging, provenance, review, and repository-policy evidence as non-transferable after any head/base change.
- Never weaken the integrated model-download boundary to make the load-time slice easier to merge.

## Task 1 — Installed-artifact verification contract

- [x] Add `src-tauri/src/llm/installed_model.rs`.
- [x] Reject missing, symbolic-link, non-regular, short, oversized, unreadable, and digest-mismatched artifacts.
- [x] Count bytes and compute SHA-256 using a fixed 64 KiB buffer.
- [x] Add deterministic opener seams so permission-sensitive branches do not depend on CI host privilege behavior.
- [ ] Require fresh exact-head GREEN evidence on the replacement branch.

## Task 2 — Bind llama construction to verified bytes

- [x] Register the installed-model module.
- [x] Call `verify_installed_model(&DEFAULT, model_path)` before `LlamaBackend::init()` and `LlamaModel::load_from_file`.
- [x] Add a source-order regression contract proving verification precedes both initialization boundaries.
- [ ] Require fresh `llm-engine` compilation and exact-head repository Test evidence.

## Task 3 — Durable acquisition and rollback evidence

- [x] Extend the model-integrity doctoring without replacing the parent slice's newer installer security design.
- [x] Record the load-time integrity boundary in `CHANGELOG.md`.
- [x] Preserve stable refusal codes, privacy boundaries, standalone/MSA behavior, and rollback requirements.
- [ ] Require exact-current-head Test, Release, Security Scan, SAST, exact coverage, packaging/provenance, review, branch policy, and release-acceptance evidence before merge.

## Acceptance

A replacement head is acceptable only when the exact unchanged commit proves all of the following:

1. correct installed bytes are accepted;
2. missing, linked, non-regular, short, oversized, unreadable, or changed bytes are rejected with stable path-free codes;
3. rejected pre-open observations never invoke the injected opener;
4. llama backend and model initialization cannot occur before the pinned default artifact passes verification;
5. the integrated parent installer remains unchanged except for documentation that explicitly composes the two boundaries; and
6. no predecessor-head, synthetic-merge, queued, skipped, cancelled, absent, rate-limited, or failed evidence is treated as success.
