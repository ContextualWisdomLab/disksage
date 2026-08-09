# DiskSage Technical Requirements Document

## Document status

**Status:** Proposed canonical technical baseline in PR #137. `protected_main`, `active_pr`, and `planned` labels are evidence classifications, not marketing maturity claims.

## Technical objective

DiskSage shall provide a local-first desktop runtime in which untrusted storage, provider, archive, model, and integration inputs can be observed and reasoned about without allowing those observations to acquire mutation authority. Rust is the authority layer, Tauri is the narrow desktop IPC boundary, and Svelte is the presentation layer.

## Runtime decomposition

| Layer | Responsibility | Authority |
| --- | --- | --- |
| Svelte presentation | Render bounded evidence, collect explicit operator choices, accessibility | No direct filesystem or provider mutation authority |
| Tauri command boundary | Expose allow-listed commands and typed inputs | Dispatch only; no arbitrary shell bridge |
| Rust observation | Scan, parse, hash, inspect provider/process/archive state | Read-only evidence generation |
| Rust planning | Build candidate sets, risk/blocker codes, destination plans, fingerprints | Advisory only |
| Rust authorization | Verify exact scope, fingerprints, human approval, rationale, phrase, freshness | Decides whether a narrowly scoped mutation may begin |
| Rust execution | Perform only the bound operation using no-clobber/create-new/trash semantics where applicable | Local mutation within exact authorization |
| Evidence/receipt layer | Produce path-free summaries and restricted private records | Records results; does not grant new authority |
| Optional model path | On-device or explicitly routed explanation/classification | Advisory, untrusted output |

## Evidence identity

Every material evidence object must be identifiable by a schema version and the exact inputs needed to determine whether it is still current. Depending on the workflow, this includes source identity, size/allocation observations, content digest, destination identity, provider/account scope, candidate ordering, operation class, timestamps, and bounded environmental evidence.

A fingerprint is a change detector and binding primitive. It is not human approval and does not prove remote durability, physical reclaimability, account ownership, or safety beyond the fields it actually binds.

## Evidence classes

- **Observation evidence:** read-only facts measured in one bounded invocation.
- **Decision-support evidence:** candidate rankings, warnings, explanations, and recommendations.
- **Blocker evidence:** explicit reasons why a requested action cannot proceed.
- **Approval evidence:** attributed human intent bound to the exact current plan and scope.
- **Execution evidence:** what the controlled mutation attempted and observed.
- **Receipt evidence:** bounded durable record of the operation result.
- **Repository evidence:** checks, reviews, workflow runs, source/base revisions, artifacts, and release provenance. Repository evidence never substitutes for runtime approval.

## Time and freshness

Runtime authorization uses an issuance timestamp, expiry timestamp, and trusted current UTC time. For authorizations created and consumed in one process, monotonic elapsed time is also checked. Current architecture fixes the maximum authorization age at 15 minutes. Reversed clocks, expired approval, changed scope, or changed plan fail closed with stable reason codes.

Historical repository timestamps and PR-body text are not live evidence. Repository decisions must bind to the current source head and independently resolved current base-branch tip.

## Filesystem safety requirements

### Path handling

- Reject unsafe parent traversal at public mutation boundaries.
- Treat symbolic links and non-regular filesystem entries as distinct types; do not silently follow them through safety boundaries.
- Canonicalize or otherwise resolve security-relevant ancestors only where semantics are explicit and tested.
- Do not expose local paths in shareable error contracts.

### No-clobber publication

Where a new artifact is created, use create-new or equivalent no-clobber publication. Preflight existence checks improve diagnostics but never replace mutation-time collision proof.

### Concurrent mutation

TOCTOU is expected. Critical paths must re-check or bind operating-system file identity so a raced source, staging path, or destination cannot make DiskSage delete/replace a foreign object. Cleanup may remove only invocation-owned artifacts or exact captured identities.

### Rollback

Failure cleanup must be identity-aware and scoped to output created by the current invocation. Source material remains unless a separately authorized operation explicitly governs its removal.

## Resource bounds

Every parser or external observation path requires explicit bounds appropriate to the input class, including file size, decoded output, archive entry count, response body, command output, recursion/depth, elapsed time, collection cardinality, and model request/response size. Exceeding a bound returns a stable incomplete/blocking state rather than silently truncating a claim into success.

## Cloud-provider technical contract

DiskSage distinguishes at least:

1. local provider-root discovery;
2. account/provider scope;
3. local vendor runtime presence;
4. capacity/quota evidence;
5. item-local placeholder/materialization state;
6. provider/local synchronization evidence;
7. remote checksum or other provider proof where available;
8. destination collision state;
9. copy receipt;
10. local-source eviction authorization.

No earlier state implies a later state. Provider APIs and native tooling must use fixed/validated endpoints and privacy-safe error normalization. Credentials are purpose-bound and must not appear in logs or cross-service evidence envelopes.

## Model artifact integrity

### Protected-main baseline

The existing product may use a local llama.cpp-backed model as advisory computation. The model path is not allowed to weaken deterministic safety or approval boundaries.

### Active PR #141 — installation integrity

The proposed installation boundary pins the default model to an immutable upstream revision, reviewed exact byte count, and SHA-256 digest; streams through bounded memory; rejects short/long/digest-mismatched content; and publishes without clobbering an existing destination. This remains `active_pr` until integrated.

### Active stacked PR #142 — load-time integrity

The proposed load boundary re-verifies the installed artifact immediately before llama backend/model initialization, rejecting missing, symlinked, non-regular, unreadable, wrong-size, or digest-mismatched artifacts. This remains `active_pr` and depends on #141.

A model digest proves identity of reviewed bytes, not model safety, behavioral quality, training provenance, or license suitability.

## LLM and external orchestration

Deterministic product safety cannot depend on a model call. If a model-backed integration is enabled:

- use `NVIDIA_NIM_API_KEY` only through GitHub Secrets for CI/live model tests;
- never use `COPILOT_GITHUB_TOKEN` as a development-model credential;
- prefer a stable contextual-orchestrator contract when network orchestration is justified;
- keep filesystem authorization, validation, mutation, and receipts inside DiskSage;
- treat model output and retrieved web/service content as untrusted data.

## Frontend requirements

- UI state is advisory and cannot become durable authorization without Rust validation.
- Backend-authored confirmation phrases are displayed and returned exactly; the frontend does not invent the authoritative phrase.
- Error and progress states must be accessible and not depend only on color.
- Stale tabs or repeated submissions must not silently reuse a changed plan.
- Production webview resource/navigation authority is constrained by the reviewed CSP contract when #139 integrates.

## API and schema versioning

Public IPC/evidence/export schemas require explicit versions or stable compatibility rules. A future or malformed version fails closed. Backward-read support for historical receipts/evidence must be explicit; aliases cannot create two authoritative interpretations.

Cross-service contracts exchange bounded schemas, fingerprints, stable action/reason identifiers, and capability/version information. Direct application-database sharing with another CWL product is not part of the architecture.

## Persistence and database requirements

DiskSage currently uses local files/receipts and domain-specific records rather than claiming one central application database. If a relational store is introduced, database objects must contain at least two descriptive words and use `snake_case` by default. Schema migration requires collision checks, forward migration, rollback or an explicit irreversible boundary, backward/forward compatibility evidence, and data-preservation tests.

`docs/DATA_MODEL.md` distinguishes conceptual domain entities from actually persisted forms.

## Repository evidence semantics

A PR merge decision must be based on the exact current source head and independently resolved current base tip. The following evidence classes remain separate:

- check-run/workflow evidence;
- commit status evidence;
- formal human review evidence;
- automated model/reviewer findings;
- security-scanner findings;
- package/provenance evidence;
- repository/ruleset merge authority.

Queued, pending, cancelled, skipped-required, neutral-required, absent, stale-head, predecessor-head, synthetic-only, action-required, rate-limited, or failed evidence is not success. A green status cannot be substituted for a required formal review or check.

## Writer and automation requirements

DiskSage has one authoritative repository-writer loop. Before a write, automation re-fetches the exact target head, current base tip, relevant review state, and target blob/ref. If another source writer moves that branch, only that branch is frozen and the loop rotates to other safe work.

Autonomous development in GitHub Actions uses an immutably pinned OpenCode Agent and the `NVIDIA_NIM_API_KEY` model credential only on the model-backed path. Temporary self-modifying repair workflows and encoded patch/finalizer workflows are not an accepted steady-state repair mechanism.

## Packaging and release

### Protected-main requirements

The Test workflow and build entry points must run the configured production coverage gates. Supported packages/builds require clean-install or equivalent reproducibility checks and security scans defined by repository/organization policy.

### Active PR #138 — provenance

Buyer-verifiable release artifact attestation and stricter artifact-set admission are `active_pr` until #137 integrates and the stacked change is retargeted/revalidated. No predecessor-head provenance is transferable.

### Release acceptance

Release only from the exact integrated protected head after required CI, security, coverage, packaging, SBOM/provenance, reproducibility, compatibility, migration/rollback/recovery, accessibility where affected, and independent review/approval gates pass. A release workflow alone is not sufficient evidence.

## Testing requirements

- Strict red-green-refactor for source defects and new authority-bearing behavior.
- Deterministic unit tests for parsers, validation, fingerprints, time/freshness, and reason codes.
- Filesystem integration tests for links, races, no-clobber, rollback, sparse/hard-linked data, and permission failures.
- Provider parser/contract tests with malformed, missing, duplicated, and inconsistent evidence.
- Concurrency tests for destination/source/staging replacement.
- Coverage must exercise production authority paths instead of excluding them.
- Packaging/release tests validate installed artifacts, metadata, versioning, provenance, and rollback where applicable.
- Documentation tests keep the canonical documentation graph and critical architecture claims discoverable.

Detailed test architecture is in `docs/TEST_STRATEGY.md`.

## Security and standards

Threats and controls are detailed in `docs/THREAT_MODEL.md` and `SECURITY.md`. The architecture and doctoring use current authoritative/primary sources where material and format research/standards references in APA 7th style. Referencing a standard is design evidence, not a certification claim.

## Implemented versus planned evidence

| Item | Classification |
| --- | --- |
| Rust/Tauri/Svelte local-first authority split | `protected_main` product architecture, strengthened in active PR #137 |
| Exact human approval/fingerprint/freshness for cloud copy actions | `protected_main` |
| Canonical PRD/TRD/UML/data-model/ADR graph | `active_pr` #137 |
| Release artifact attestation | `active_pr` #138 |
| Fail-closed Tauri CSP | `active_pr` #139 |
| Cargo package metadata hardening | `active_pr` #140 |
| Bounded model installation integrity | `active_pr` #141 |
| Installed model re-verification at load | `active_pr` #142 |
| Future persistence beyond current local evidence/receipt stores | `planned` unless separately evidenced |

## Technical acceptance

A technical change is not complete when only its happy path or documentation exists. It must have the production path, refusal/degraded path, bounded resource model, relevant security/privacy contract, migration/rollback impact, realistic tests, exact-head validation, and synchronized authoritative documentation.