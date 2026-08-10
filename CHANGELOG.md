# Changelog

All notable changes to DiskSage are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and released versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Unreleased entries describe integrated source changes only; they are not release evidence until the repository's review, CI, security, packaging, provenance, and release-acceptance gates pass on the exact tagged commit.

## [Unreleased]

### Changed

- Bind Tauri packaging to a fail-closed cross-manifest release-version verifier so `package.json`, `Cargo.toml`, `tauri.conf.json`, and any `v*` release tag must agree on one valid Semantic Version before a bundle is built.
- Add retry-safe release concurrency: fresh first attempts may supersede stale runs, while explicit GitHub rerun attempts do not self-cancel inside the same concurrency group.
- Require a fresh, exact, human-attributed approval and rationale for cloud copy-only and existing-copy adoption actions, with a 15-minute authorization lifetime bound to the candidate, destination, provider, account scope, and review fingerprint.
- Return the candidate-specific cloud copy approval action, exact confirmation phrase, and maximum approval age from the Rust plan contract; the frontend only displays and submits that backend-authored phrase and fails closed when it is missing or does not match the candidate action.
- Align the frontend toolchain on Vite 8.2 and `@sveltejs/vite-plugin-svelte` 7.2 so the declared peer dependency graph is installable and reproducible.
- Declare the supported Node.js runtime floor as Node.js 20.19 or Node.js 22.12 and later, matching Vite 8 requirements.
- Pin the primary test workflow to Node.js 20.19.0 so the minimum supported runtime is continuously verified.
- Document the iCloud batch operation's local-only versus path-free shareable evidence boundary and map its fail-closed controls to NIST SP 800-53 Release 5.2.0, ISO/IEC 27040:2024, and primary secure-design literature with APA 7th references and deterministic documentation contract tests.
- Refresh the Tauri CSP standards evidence to the current July 29, 2026 W3C Content Security Policy Level 3 Working Draft and regression-test its exact publication URL so future doctoring cannot silently drift back to an older draft.

### Fixed

- Hardened iCloud local-copy batch eviction with fresh per-item timestamps, deterministic planner/executor/recorder/clock seams, fail-closed immutable checkpoint handling, bounded manifest admission, symlink-safe control-path validation, and distinct operator diagnostics.
- Restored the cloud-copy public documentation regression contract after a temporary repair path removed it, so CI continues to fail when the new Rust or TypeScript approval surfaces lose beginner-readable documentation.

### Security

- Add buyer-verifiable release artifact provenance with read-only platform build jobs, a tag-only least-privilege attestation job, exact 17-file admission, adjacent operational-CLI SHA-256 verification, preserved artifact namespaces, non-regular-entry rejection, and a separate publication job that cannot publish before attestation succeeds.
- Require explicit organization-tenant authority when either the destination account scope is organization-owned or the canonical organization-sensitive review reason is present; fail closed in both frontend projection and durable Rust transfer authorization even when the ordinary review flag is absent, and regression-test contradictory signal combinations.
- Enable an explicit fail-closed Tauri Content Security Policy to keep executable scripts and fonts local, grant production network authority only to the Tauri IPC transport, confine Vite WebSocket HMR to a separate development-only CSP, deny object/frame/base-URI authority, deny form submissions with explicit `form-action 'none'`, deny unused worker, media, and web-app-manifest fetch authority with explicit `'none'` directives, and regression-test against null, wildcard, remote-script/style, eval, and development-authority leakage.
- Re-verify the installed GGUF immediately before llama.cpp initialization and retain the verified model handle through llama.cpp loading: reject missing, linked, non-regular, identity-raced, short, oversized, unreadable, or SHA-256-mismatched artifacts with stable path-free errors; use a stable descriptor path on Unix and a Windows read-sharing guard so the mutable source pathname cannot be substituted between verification and model parsing.
- Bind the default on-device GGUF model to an immutable upstream revision, exact byte count, and SHA-256 digest; replace whole-model buffering and named sibling staging with bounded streaming into an unnamed same-directory temporary file; ignore and preserve unrelated legacy `.part` paths; refuse destination overwrite with create-new semantics; capture destination ownership from the returned open file handle; re-read and rehash the still-open staging source while copying; flush, sync, re-read, and rehash the destination before final acceptance; reject same-file source or destination mutation; preserve foreign destination replacements through identity-bound cleanup; and keep model installation inside the Rust coverage surface with privacy-safe stable errors and deterministic race regressions.
- Persist copy-approval provenance in immutable receipt lineage, reject stale, generic, mismatched, or tampered approvals, and retain explicit backward readability for pre-approval receipt formats.
- Generate the npm lockfile in an exact-head validation job with repository contents read-only and dependency lifecycle scripts disabled, bind the artifact to SHA-256 evidence, and grant `contents: write` only to a separate publication job that verifies the same-run artifact and unchanged branch head before committing the lockfile.
- Removed obsolete one-shot repair workflows and patch scripts so repository automation no longer retains dormant write-capable recovery paths.
