# Changelog

All notable changes to DiskSage are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and released versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Unreleased entries describe integrated source changes only; they are not release evidence until the repository's review, CI, security, packaging, provenance, and release-acceptance gates pass on the exact tagged commit.

## [Unreleased]

### Added

- Add a read-only Podman evidence panel to Cleanup that separately displays configured VM capacity, raw-image logical size, host allocation, guest filesystem observations, Podman store observations, image/stopped-container/volume logical candidates, evidence completeness, stable issue codes, and a redacted candidate-set fingerprint.
- Add a privacy-safe Tauri contract that removes machine names, local paths, graph-root locations, image identifiers, tags, command output, and dynamic error details before evidence reaches the desktop frontend.

### Changed

- Align the frontend toolchain on Vite 8.2 and `@sveltejs/vite-plugin-svelte` 7.2 so the declared peer dependency graph is installable and reproducible.
- Declare the supported Node.js runtime floor as Node.js 20.19 or Node.js 22.12 and later, matching Vite 8 requirements.
- Pin the primary test workflow to Node.js 20.19.0 so the minimum supported runtime is continuously verified.
- Document the iCloud batch operation's local-only versus path-free shareable evidence boundary and map its fail-closed controls to NIST SP 800-53 Release 5.2.0, ISO/IEC 27040:2024, and primary secure-design literature with APA 7th references and deterministic documentation contract tests.
- Keep Podman image, stopped-container, and volume review boundaries independent and advisory; no candidate class grants authority to another class.

### Fixed

- Hardened iCloud local-copy batch eviction with fresh per-item timestamps, deterministic planner/executor/recorder/clock seams, fail-closed immutable checkpoint handling, bounded manifest admission, symlink-safe control-path validation, and distinct operator diagnostics.

### Security

- Generate the npm lockfile in an exact-head validation job with repository contents read-only and dependency lifecycle scripts disabled, bind the artifact to SHA-256 evidence, and grant `contents: write` only to a separate publication job that verifies the same-run artifact and unchanged branch head before committing the lockfile.
- Removed obsolete one-shot repair workflows and patch scripts so repository automation no longer retains dormant write-capable recovery paths.
- Keep the Podman desktop surface observation-only: it exposes no prune, remove, machine stop/start, VM deletion, TRIM, raw-image mutation, or shell-string construction path, and it never labels Podman logical candidates as verified host physical reclaimability.
