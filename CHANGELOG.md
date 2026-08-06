# Changelog

All notable changes to DiskSage are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and released versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Unreleased entries describe integrated source changes only; they are not release evidence until the repository's review, CI, security, packaging, provenance, and release-acceptance gates pass on the exact tagged commit.

## [Unreleased]

### Changed

- Added the authoritative buyer-facing architecture contract for standalone and modular MSA deployment, trust and authorization boundaries, privacy-safe evidence, migration and rollback, exact-head release evidence, database naming, and acquisition diligence, with current APA 7th standards references and a deterministic documentation regression test.
- Defined separate read-only and mutating-operation authority contracts with mandatory scope and fingerprint inputs, trusted UTC and monotonic clock handling, a uniform 15-minute authorization lifetime, and explicit fail-closed rejection states for expired, clock-invalid, scope-mismatched, and stale-plan execution attempts.
- Separated the October 2023 W3C WCAG 2.2 Recommendation from ISO/IEC 40500:2025 in the standards record so publisher, publication date, and canonical URL remain attributable.
- Expanded frontend coverage measurement to all production TypeScript modules under `src/lib` and `src/routes`, excluding only tests and declarations, while retaining 100% statement, branch, function, and line thresholds.
- Bound both exact-head Test and release packaging entry points to `npm run coverage`; Tauri release builds inherit the coverage gate through the package build contract before bundle creation.
- Require a fresh, exact, human-attributed approval and rationale for cloud copy-only and existing-copy adoption actions, with a 15-minute authorization lifetime bound to the candidate, destination, provider, account scope, and review fingerprint.
- Return the candidate-specific cloud copy approval action, exact confirmation phrase, and maximum approval age from the Rust plan contract; the frontend only displays and submits that backend-authored phrase and fails closed when it is missing or does not match the candidate action.
- Align the frontend toolchain on Vite 8.2 and `@sveltejs/vite-plugin-svelte` 7.2 so the declared peer dependency graph is installable and reproducible.
- Declare the supported Node.js runtime floor as Node.js 20.19 or Node.js 22.12 and later, matching Vite 8 requirements.
- Pin the primary test workflow to Node.js 20.19.0 so the minimum supported runtime is continuously verified.
- Document the iCloud batch operation's local-only versus path-free shareable evidence boundary and map its fail-closed controls to NIST SP 800-53 Release 5.2.0, ISO/IEC 27040:2024, and primary secure-design literature with APA 7th references and deterministic documentation contract tests.

### Fixed

- Added retry-safe release concurrency so a new first attempt still supersedes stale work while an explicit GitHub Actions rerun cannot cancel itself inside the same concurrency group.
- Made architecture evidence tests independent of the process working directory, verified linked evidence files actually exist, enforced heading and exact-head continuity, and retained the two-word `snake_case` database-object naming contract.
- Hardened iCloud local-copy batch eviction with fresh per-item timestamps, deterministic planner/executor/recorder/clock seams, fail-closed immutable checkpoint handling, bounded manifest admission, symlink-safe control-path validation, and distinct operator diagnostics.
- Restored the cloud-copy public documentation regression contract after a temporary repair path removed it, so CI continues to fail when the new Rust or TypeScript approval surfaces lose beginner-readable documentation.

### Security

- Added buyer-verifiable release artifact provenance with checksum-first admission, immutable `actions/attest` pinning, tag-only OIDC and attestation authority, and publication that depends on successful exact-artifact provenance generation.
- Fail closed before packaging when `package.json`, Cargo, and Tauri versions disagree, when a version is missing or malformed, or when a release tag is not exactly `v<manifest version>`.
- Enforce Semantic Versioning 2.0.0 numeric prerelease rules and reject leading-zero identifiers such as `1.0.0-01` before packaging.
- Bound every release checksum record to the exact adjacent operational CLI basename and reject malformed, multi-record, redirected, traversing, absolute, or decoy checksum targets before digest verification.
- Reject every unexpected eighteenth release file and every non-regular artifact-tree entry before attestation or publication, preventing unreviewed diagnostics, dumps, logs, secrets, or unrelated executables from becoming durable release assets.
- Persist copy-approval provenance in immutable receipt lineage, reject stale, generic, mismatched, or tampered approvals, and retain explicit backward readability for pre-approval receipt formats.
- Generate the npm lockfile in an exact-head validation job with repository contents read-only and dependency lifecycle scripts disabled, bind the artifact to SHA-256 evidence, and grant `contents: write` only to a separate publication job that verifies the same-run artifact and unchanged branch head before committing the lockfile.
- Removed obsolete one-shot repair workflows and patch scripts so repository automation no longer retains dormant write-capable recovery paths.
