# Changelog

All notable changes to DiskSage are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and released versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Unreleased entries describe integrated source changes only; they are not release evidence until the repository's review, CI, security, packaging, provenance, and release-acceptance gates pass on the exact tagged commit.

## [Unreleased]

### Changed

- Add a bounded, read-only exact-photo duplicate audit that records byte identity with BLAKE3 and
  groups local PNGs only when raster dimensions and normalized decoded RGBA16 pixels share the
  version-two domain-separated digest. Provider-managed paths, Photos libraries, dataless
  placeholders, symlinks, oversized encoded inputs, and replacement races fail closed; incomplete
  audits return a non-zero CLI exit after emitting structured rejection evidence. Cleanup and
  permanent deletion remain unavailable, and active-use evidence is reserved for a fresh future
  execution preflight.
- Keep coverage builds compile-safe by applying the same `not(coverage)` boundary to native-copy
  identity cleanup and dependent eviction helpers; the focused authority contract remains green.
- Add durable private failure records in a separate journal directory and a receipt-bound
  cancellation command for bounded native cloud copies; bind cancellation to the active candidate,
  require provider-native local-current materialization evidence before existing-copy adoption can
  hash a destination, cap the private failure journal at 10,000 records, and bind failed-copy
  cleanup to Unix/Windows file identity while keeping shareable lineage exports path-free. Existing
  copy adoption remains explicitly non-cancellable because it performs verification only.
- Persist bounded, path-free local-volume snapshots from cloud plans with create-only files,
  content fingerprints, Unix `0400`/`0700` permissions, and shape-limited retention; surface a
  warning when incident-comparison evidence cannot be written without changing copy authority.
- Persist path-free provider-client process observations with the same bounded, create-only
  evidence contract so a stalled File Provider incident can be compared across planning loops.
- Persist redacted iCloud queue and File Provider activity summaries as bounded, create-only,
  timestamped evidence records, without retaining raw CloudDocs databases or provider dumps;
  surface persistence failure without changing copy or eviction authority.
- Gate iCloud copy plans on a path-free three-stream evidence cohort with deterministic
  fingerprints and a five-minute observation-skew ceiling; incomplete, malformed, or stale
  observations remain blocked and never become cloud-write or eviction authority.
- Carry the integrity-checked iCloud pre-copy cohort and `pre_copy_evidence_met` through the
  Naruon cloud-copy readiness envelope (schema version 8), so aggregate consumers also fail closed
  when the provider queue is quiet but pre-copy evidence is absent.
- Keep the hourly contextual-orchestrator loop on its published read-only API, bind context to
  the exact event commit, and remove foreign-repository checkout, KV mutation, and provider-secret
  ingestion from DiskSage Actions.
- Keep the repository-local contextual-orchestrator advisory workflow manual-only and bind the
  hourly OpenCode review/repair schedule to the trusted central `.github` scheduler, avoiding an
  unpinned autonomous model reviewer while retaining exact-head, read-only evidence boundaries.
- Show the last read-only iCloud File Provider evidence timestamp beside the
  new-copy admission state, so a stalled `no progress`/`hard expired` queue has
  an actionable retry context without exposing provider paths.
- Bind Tauri packaging to a fail-closed cross-manifest release-version verifier so `package.json`, `Cargo.toml`, `tauri.conf.json`, and any `v*` release tag must agree on one valid Semantic Version before a bundle is built.
- Add retry-safe release concurrency: fresh first attempts may supersede stale runs, while explicit GitHub rerun attempts do not self-cancel inside the same concurrency group.
- Replace generator-era Cargo package metadata with the DiskSage product description, MIT license expression, canonical source repository URL, and `publish = false` registry-publication boundary; deliberately omit Cargo's deprecated `authors` field, verify publication refusal through Cargo's versioned parsed metadata rather than substring matching, and regression-test commented/out-of-table decoys together with the retained acquisition metadata and doctoring evidence.
- Require a fresh, exact, human-attributed approval and rationale for cloud copy-only and existing-copy adoption actions, with a 15-minute authorization lifetime bound to the candidate, destination, provider, account scope, and review fingerprint.
- Return the candidate-specific cloud copy approval action, exact confirmation phrase, and maximum approval age from the Rust plan contract; the frontend only displays and submits that backend-authored phrase and fails closed when it is missing or does not match the candidate action.
- Align the frontend toolchain on Vite 8.2 and `@sveltejs/vite-plugin-svelte` 7.2 so the declared peer dependency graph is installable and reproducible.
- Declare the supported Node.js runtime floor as Node.js 20.19 or Node.js 22.12 and later, matching Vite 8 requirements.
- Pin the primary test workflow to Node.js 20.19.0 so the minimum supported runtime is continuously verified.
- Document the iCloud batch operation's local-only versus path-free shareable evidence boundary and map its fail-closed controls to NIST SP 800-53 Release 5.2.0, ISO/IEC 27040:2024, and primary secure-design literature with APA 7th references and deterministic documentation contract tests.
- Refresh the Tauri CSP standards evidence to the current July 29, 2026 W3C Content Security Policy Level 3 Working Draft and regression-test its exact publication URL so future doctoring cannot silently drift back to an older draft.

### Fixed

- Reject ontology organize destinations that are relative to the process working directory,
  named-user tilde paths, or parent-traversal paths; only an absolute destination or a home token
  (`~`/`~/`, plus native Windows `~\`) can produce a move plan, and literal tildes in absolute
  paths are preserved.
- Surface the bounded iCloud File Provider upload/download fractions and label repeated
  `no progress` observations as a Finder “copy preparing” stall, so the operator can cancel the
  pending Finder request before retrying; this remains diagnostic and never grants copy or eviction
  authority.

- Keep the shipped Naruon readiness verifier source includable by its integration boundary test;
  the terminal parser contract now compiles in both the binary and test-module contexts.

- Cover the `sensitive-config` archive-kind wire label in the generated cloud-plan implementation,
  so the macOS/Linux/Windows cloud-plan binaries compile after the sensitive-config safety
  boundary is enabled.
- Keep the local staging-headroom gate on new native copies only; existing-copy adoption now
  remains available on low-disk volumes because it verifies an already-present destination without
  creating local staging data.
- Surface insufficient local staging headroom as a dry-run notice and native-copy blocker before
  review, while keeping the non-staging provider-API fallback and existing-copy adoption available.
- Make `disksage-duplicate-audit --help` exit successfully so release staging can
  verify its usage contract without treating a help request as a failed audit.
- Publish the source-bound SPDX SBOM as a separately named artifact only after
  provenance succeeds, then download it in the release publication job so the
  attested 18-file release set cannot silently omit its component inventory.
- Isolate the macOS global File Provider dump helper in a private process group and terminate the
  whole group on timeout, preventing descendant helpers from retaining a pipe after a stalled
  Finder/provider copy.
- Classify repeated File Provider `-1005 itemNotFound` markers as a path-free global sync blocker,
  retain the same-blocker duration when reconciliation counts change, and direct operators to
  cancel a stalled Finder copy before retrying.
- Reject legacy provider evidence that reports `sync_complete=true` without an explicit complete
  `sync_state` at the current authorization and eviction boundary, while retaining compatibility
  reads and a public-boundary regression test.
- Replace the unmaintained direct `jwalk` production dependency with the maintained `walkdir`
  backend across scanner, duplicate, artifact, cloud, and reclaim traversals; preserve symlink/
  reparse filtering and fail-closed traversal-error accounting with a locked dependency contract.
- Inventory direct credential-bearing configuration names as blocked `sensitive-config` entries;
  never open them for metadata probing or include them in cloud-copy or source-eviction authority.
- Bound one-minute background reconciliation to 128 immutable provider evidence records per
  receipt, and validate active iCloud File Provider transfers as blocked readiness evidence.
- Scope persisted API object-id recovery by receipt filename prefix before scanning, so unrelated
  receipts in a shared evidence directory cannot hide a valid Google Drive or OneDrive locator.
- Add an explicit macOS Finder-copy cancellation command that sends only a bounded Escape request;
  it never accepts scripts or paths and never terminates iCloud/File Provider services.
- Fail closed when OneDrive or Google Drive runtime evidence is unavailable during client recovery;
  an unknown observation is no longer treated as proof that the provider process is absent.
- Hardened iCloud local-copy batch eviction with fresh per-item timestamps, deterministic planner/executor/recorder/clock seams, fail-closed immutable checkpoint handling, bounded manifest admission, symlink-safe control-path validation, and distinct operator diagnostics.
- Restored the cloud-copy public documentation regression contract after a temporary repair path removed it, so CI continues to fail when the new Rust or TypeScript approval surfaces lose beginner-readable documentation.

### Security

- Default personal cloud-provider OAuth consent to read-only; upload scope and API write
  authority now require an explicit user opt-in.
- Catalog the Cargo registry source tree as an explicit, identity-bound regenerable-cache target;
  keep it out of automatic cleanup because rebuilding may require network downloads.
- Catalog the observed Node.js, PyTorch, Prisma, and GitHub CLI cache trees as identity-bound
  manual-review targets; keep them out of automatic cleanup until their active-use and rebuild
  contracts are independently established.
- Add buyer-verifiable release artifact provenance with read-only platform build jobs, a tag-only least-privilege attestation job, exact 18-file admission including a source-bound SPDX SBOM, adjacent operational-CLI SHA-256 verification, preserved artifact namespaces, non-regular-entry rejection, and a separate publication job that cannot publish before attestation succeeds.
- Require explicit organization-tenant authority when either the destination account scope is organization-owned or the canonical organization-sensitive review reason is present; fail closed in both frontend projection and durable Rust transfer authorization even when the ordinary review flag is absent, and regression-test contradictory signal combinations.
- Enable an explicit fail-closed Tauri Content Security Policy to keep executable scripts and fonts local, grant production network authority only to the Tauri IPC transport, confine Vite WebSocket HMR to a separate development-only CSP, deny object/frame/base-URI authority, deny form submissions with explicit `form-action 'none'`, deny unused worker, media, and web-app-manifest fetch authority with explicit `'none'` directives, and regression-test against null, wildcard, remote-script/style, eval, and development-authority leakage.
- Re-verify the installed GGUF immediately before llama.cpp initialization and retain the verified model handle through llama.cpp loading: reject missing, linked, non-regular, identity-raced, short, oversized, unreadable, or SHA-256-mismatched artifacts with stable path-free errors; use a stable descriptor path on Unix and a Windows read-sharing guard so the mutable source pathname cannot be substituted between verification and model parsing.
- Bind the default on-device GGUF model to an immutable upstream revision, exact byte count, and SHA-256 digest; replace whole-model buffering and named sibling staging with bounded streaming into an unnamed same-directory temporary file; ignore and preserve unrelated legacy `.part` paths; refuse destination overwrite with create-new semantics; capture destination ownership from the returned open file handle; re-read and rehash the still-open staging source while copying; flush, sync, re-read, and rehash the destination before final acceptance; reject same-file source or destination mutation; preserve foreign destination replacements through identity-bound cleanup; and keep model installation inside the Rust coverage surface with privacy-safe stable errors and deterministic race regressions.
- Persist copy-approval provenance in immutable receipt lineage, reject stale, generic, mismatched, or tampered approvals, and retain explicit backward readability for pre-approval receipt formats.
- Generate the npm lockfile in an exact-head validation job with repository contents read-only and dependency lifecycle scripts disabled, bind the artifact to SHA-256 evidence, and grant `contents: write` only to a separate publication job that verifies the same-run artifact and unchanged branch head before committing the lockfile.
- Removed obsolete one-shot repair workflows and patch scripts so repository automation no longer retains dormant write-capable recovery paths.
