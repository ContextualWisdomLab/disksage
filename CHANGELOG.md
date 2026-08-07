# Changelog

All notable changes to DiskSage are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and released versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Unreleased entries describe integrated source changes only; they are not release evidence until the repository's review, CI, security, packaging, provenance, and release-acceptance gates pass on the exact tagged commit.

## [Unreleased]

### Added

- Add a read-only Podman evidence panel to Cleanup that separately displays configured VM capacity, raw-image logical size, host allocation, guest filesystem observations, Podman store observations, image/stopped-container/volume logical candidates, evidence completeness, stable issue codes, and a redacted candidate-set fingerprint.
- Add a privacy-safe Tauri contract that removes machine names, local paths, graph-root locations, image identifiers, tags, command output, and dynamic error details before evidence reaches the desktop frontend.
- Add beginner-readable JSDoc for every Podman frontend contract function and a deterministic source-level regression test that fails when any production function loses its adjacent documentation.
- Add module-level Rust `missing_docs` enforcement and a deterministic source-level contract that requires beginner-readable rustdoc for every Podman desktop function, including private helpers and regression tests.

### Changed

- Require the `Test` workflow to produce exact-head, branch-aware, fail-closed Rust coverage evidence from real `cargo llvm-cov` measurements before organization-level review can treat 100% statement-equivalent region, branch, function, and line coverage as passing.
- Require a fresh, exact, human-attributed approval and rationale for cloud copy-only and existing-copy adoption actions, with a 15-minute authorization lifetime bound to the candidate, destination, provider, account scope, and review fingerprint.
- Return the candidate-specific cloud copy approval action, exact confirmation phrase, and maximum approval age from the Rust plan contract; the frontend only displays and submits that backend-authored phrase and fails closed when it is missing or does not match the candidate action.
- Align the frontend toolchain on Vite 8.2 and `@sveltejs/vite-plugin-svelte` 7.2 so the declared peer dependency graph is installable and reproducible.
- Declare the supported Node.js runtime floor as Node.js 20.19 or Node.js 22.12 and later, matching Vite 8 requirements.
- Pin the primary test workflow to Node.js 20.19.0 so the minimum supported runtime is continuously verified.
- Document the iCloud batch operation's local-only versus path-free shareable evidence boundary and map its fail-closed controls to NIST SP 800-53 Release 5.2.0, ISO/IEC 27040:2024, and primary secure-design literature with APA 7th references and deterministic documentation contract tests.
- Keep Podman image, stopped-container, and volume review boundaries independent and advisory; no candidate class grants authority to another class.

### Fixed

- Hardened iCloud local-copy batch eviction with fresh per-item timestamps, deterministic planner/executor/recorder/clock seams, fail-closed immutable checkpoint handling, bounded manifest admission, symlink-safe control-path validation, and distinct operator diagnostics.
- Restored the cloud-copy public documentation regression contract after a temporary repair path removed it, so CI continues to fail when the new Rust or TypeScript approval surfaces lose beginner-readable documentation.

### Security

- Persist copy-approval provenance in immutable receipt lineage, reject stale, generic, mismatched, or tampered approvals, and retain explicit backward readability for pre-approval receipt formats.
- Generate the npm lockfile in an exact-head validation job with repository contents read-only and dependency lifecycle scripts disabled, bind the artifact to SHA-256 evidence, and grant `contents: write` only to a separate publication job that verifies the same-run artifact and unchanged branch head before committing the lockfile.
- Removed obsolete one-shot repair workflows and patch scripts so repository automation no longer retains dormant write-capable recovery paths.
- Keep the Podman desktop surface observation-only: it exposes no prune, remove, machine stop/start, VM deletion, TRIM, raw-image mutation, or shell-string construction path, and it never labels Podman logical candidates as verified host physical reclaimability.
- Replace untrusted Tauri, operating-system, and JavaScript failure details with one stable Podman UI error code so account-local paths, machine names, socket locations, and command details cannot leak through the visible desktop evidence boundary.
- Accept Podman probe issue prefixes only as bounded lowercase kebab-case codes; delimiter-free paths, sockets, uppercase text, Unicode, whitespace, underscores, and malformed prefixes collapse to `podman-evidence-error` before desktop IPC serialization.
