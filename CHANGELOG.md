# Changelog

All notable changes to DiskSage are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and released versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Unreleased entries describe integrated source changes only; they are not release evidence until the repository's review, CI, security, packaging, provenance, and release-acceptance gates pass on the exact tagged commit.

## [Unreleased]

### Changed

- Added a canonical acquisition documentation graph covering PRD, TRD, architecture decisions, Mermaid UML, conceptual-versus-persisted data model and ERD, API/IPC/evidence contracts, threat model, test strategy, operability/recovery, requirements/evidence traceability, documentation completeness assessment, agent governance, and repository context; a deterministic documentation contract now fails when required families or critical authority markers disappear.
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

- Made architecture evidence tests independent of the process working directory, verified linked evidence files actually exist, enforced heading and exact-head continuity, and retained the two-word `snake_case` database-object naming contract.
- Hardened iCloud local-copy batch eviction with fresh per-item timestamps, deterministic planner/executor/recorder/clock seams, fail-closed immutable checkpoint handling, bounded manifest admission, symlink-safe control-path validation, and distinct operator diagnostics.
- Restored the cloud-copy public documentation regression contract after a temporary repair path removed it, so CI continues to fail when the new Rust or TypeScript approval surfaces lose beginner-readable documentation.

### Security

- Bind the default on-device GGUF model to an immutable upstream revision, exact byte count, and SHA-256 digest; replace whole-model buffering and named sibling staging with bounded streaming into an unnamed same-directory temporary file; ignore and preserve unrelated legacy `.part` paths; refuse destination overwrite with create-new semantics; capture destination ownership from the returned open file handle; re-read and rehash the still-open staging source while copying; flush, sync, re-read, and rehash the destination before final acceptance; reject same-file source or destination mutation; preserve foreign destination replacements through identity-bound cleanup; and keep model installation inside the Rust coverage surface with privacy-safe stable errors and deterministic race regressions.
- Require explicit organization-tenant authority in both the frontend projection and durable Rust transfer gate when either the organization destination scope or the organization-sensitive review reason is present, preventing a missing, contradictory, or malformed candidate field from making cloud approval less restrictive; record the fail-closed decision, rollback boundary, realistic signal-matrix tests, and APA 7th references in `docs/architecture/cloud-review-tenant-authority.md`.
- Separate runtime mutation authorization from repository merge and release authorization: runtime approvals bind exact operation scope, fingerprints, schema, and trusted-clock freshness, while exact repository-head evidence remains a CI and release gate and never becomes an operator credential.
- Persist copy-approval provenance in immutable receipt lineage, reject stale, generic, mismatched, or tampered approvals, and retain explicit backward readability for pre-approval receipt formats.
- Keep npm lockfile regeneration in an exact-head, read-only validation path with dependency lifecycle scripts disabled and SHA-256 artifact binding; publication remains a DiskSage writer-lease operation that verifies the same-run artifact and unchanged source head before using narrowly scoped repository-write authority.
- Removed obsolete one-shot repair workflows and patch scripts so repository automation no longer retains dormant write-capable recovery paths.