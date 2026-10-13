# DiskSage Technical Requirements Document

## Document status

**Status:** Proposed canonical technical baseline based on current protected main. This document specifies durable constraints; dated implementation status belongs in traceability/assessment rather than being embedded as permanent PR-number prose.

## Technical objective

DiskSage shall provide a local-first desktop runtime in which untrusted storage, provider, archive, model, and integration inputs can be observed and reasoned about without allowing those observations to acquire mutation authority. Rust is the security-relevant authority layer, Tauri is the typed desktop IPC boundary, and Svelte is the presentation layer.

## Runtime decomposition

| Layer | Responsibility | Authority |
| --- | --- | --- |
| Svelte presentation | Render bounded evidence, collect choices, accessible interaction | No direct filesystem/provider mutation authority |
| Tauri IPC | Expose allow-listed typed commands | Dispatch boundary only |
| Rust observation | Scan, parse, hash, inspect bounded local/provider state | Read-only evidence generation |
| Rust planning | Candidate sets, blockers, proposed actions, fingerprints | Advisory |
| Rust authorization | Exact scope/fingerprint/approval/freshness validation | Decides whether exact mutation may begin |
| Rust execution | Perform one bound operation, revalidate preconditions | Local mutation within authorization |
| Evidence/receipt | Bounded summaries and restricted private records | Records outcomes; grants no new authority |
| Optional model | On-device or explicitly routed explanation | Advisory untrusted output |

## Evidence identity

Every material evidence object requires an explicit schema/version or stable compatibility contract and enough input identity to determine whether it remains current. Depending on the workflow, identity may include source/candidate fingerprint, size/allocation, content digest, destination, provider/account scope, operation class, candidate ordering, observation time, and resource-bound outcome.

A fingerprint is a binding/change-detection primitive. It is not approval and does not prove a fact it does not encode.

## Evidence classes

- **Observation evidence** — bounded read-only facts from one invocation.
- **Decision-support evidence** — recommendations, rankings, explanations, uncertainty.
- **Blocker evidence** — reasons a requested action cannot proceed.
- **Approval evidence** — attributed human intent bound to exact current plan/scope/freshness.
- **Execution evidence** — what the controlled mutation attempted and observed.
- **Receipt evidence** — durable bounded record of the operation outcome.
- **Repository evidence** — source/base revisions, checks, statuses, reviews, scanner findings, runs, artifacts, provenance.
- **Release evidence** — exact integrated source plus accepted package/provenance/governance evidence.

No evidence class implies another.

## Runtime time and freshness

Runtime approval records include issuance and expiry; same-process authorization uses monotonic elapsed-time checks in addition to UTC where implemented. Current cloud-copy authorization uses a maximum 15-minute lifetime. Expired approval, reversed/inconsistent clocks, scope mismatch, or plan drift fails closed.

Repository timestamps and PR descriptions are historical labels, not live authorization.

## Filesystem requirements

### Path and type safety

- Reject unsafe traversal at public mutation boundaries.
- Treat symbolic links and non-regular entries as distinct and untrusted.
- Do not silently follow a link through an authority boundary.
- Do not expose local paths in shareable errors/evidence.

### No-clobber semantics

Preflight existence checks improve diagnostics only. Final publication uses create-new or equivalent no-clobber semantics where an operation creates an artifact.

### Concurrency and identity

TOCTOU is expected. Security-critical operations capture or revalidate operating-system identity so a raced source, staging object, or destination cannot cause DiskSage to replace or delete a foreign object. Cleanup is invocation-owned and identity-aware.

### Rollback/recovery

Source material remains unless a separate exact operation authorizes removal. Partial output is removed or retained only according to explicit recovery rules for the current invocation.

## Resource bounds

Every parser, scanner, provider observation, command/process reader, model input/output, and export path defines applicable bounds for size, count, depth, elapsed time, decoded output, response body, archive expansion, collection cardinality, and memory. Exceeding a bound produces explicit incomplete/blocking evidence rather than silent truncation to success.

## Cloud/provider contract

The implementation keeps at least these states independent:

1. provider/root discovery;
2. account/provider scope;
3. local client/runtime presence;
4. quota/capacity evidence;
5. placeholder/materialization state;
6. provider queue state;
7. item synchronization evidence;
8. remote checksum/durability evidence when supported;
9. destination collision state;
10. copy/adoption receipt;
11. local eviction authorization.

Provider endpoints, redirect targets, response sizes, parsed fields, and diagnostics are bounded and validated. Credentials are purpose-bound and excluded from shareable evidence/logging.

## Model artifact requirements

Current protected main treats the default GGUF as executable supply-chain input and implements installation plus load-time integrity boundaries:

- immutable reviewed upstream revision;
- exact expected byte count;
- SHA-256 digest;
- bounded streamed installation;
- race-resistant/no-clobber publication;
- final installed-byte verification;
- non-following load-time validation;
- exact size and digest verification immediately before llama.cpp;
- verified identity retained through initialization to reduce pathname substitution risk;
- stable path-free refusal codes.

Model integrity proves reviewed-byte identity only. Behavioral safety, model quality, training provenance, licensing, and prompt/output trust remain separate controls.

## LLM and external orchestration

Deterministic safety and mutation authority cannot depend on a model call.

When model-backed CI or product evaluation is justified:

- use GitHub Secret `NVIDIA_NIM_API_KEY` for model calls;
- do not use `COPILOT_GITHUB_TOKEN` as a development-model credential;
- prefer contextual-orchestrator only through an explicit stable contract and separate writer lease;
- treat model output/retrieved content as untrusted data;
- keep deterministic product validation and authorization local.

Any GitHub Actions autonomous development agent uses an immutably pinned OpenCode implementation and preserves the independent review-agent credential chain.

## Frontend requirements

- UI state is advisory until Rust validates the complete request.
- Backend-authored confirmation phrases are displayed/submitted exactly; the frontend does not invent authority text.
- Error/progress/refusal states are keyboard and assistive-technology accessible.
- Risk meaning does not depend only on color.
- Stale tabs and duplicate submissions cannot reuse a changed plan as if current.
- Webview CSP and navigation/resource policy fail closed where configured by the integrated product.

## API and schema versioning

Public IPC, evidence, private dossier, receipt, and cross-service formats require explicit versions or reviewed compatibility rules. Future/unknown/malformed versions fail closed. Backward-read compatibility is explicit; aliases cannot create two authoritative interpretations.

See `docs/API_CONTRACT.md`.

## Persistence and database requirements

No central application database is assumed. Conceptual entities may map to Rust structures, JSON/private files, receipts, provider-specific records, or GitHub/release evidence.

If relational persistence is introduced:

- database objects use at least two descriptive words in `snake_case` by default;
- tenant/ownership scope is explicit;
- migration includes collision/data-preservation checks;
- rollback or explicit irreversibility is documented;
- backward/forward compatibility is tested;
- retention, encryption, indexes, and access authority are documented.

See `docs/DATA_MODEL.md`.

## Repository evidence semantics

Merge/release decisions require the **exact current source head** and the **independently resolved live base tip**. Separate evidence classes include check runs, commit statuses, formal reviews, automated/model reviews, security scanners, package/provenance evidence, and repository/ruleset merge authority.

Queued, pending, cancelled, skipped-required, neutral-required, absent, stale-head, predecessor-head, synthetic-only, status-only, action-required, rate-limited, or failed evidence is not success.

No older-head evidence transfers after a source/base change.

## Repository writer lease and work-conserving automation

The dedicated DiskSage maintenance/development loop is the authoritative repository writer. Before every write it re-fetches the exact target head, live base, relevant review/security state, and target blob/ref. Source movement by another writer freezes only the affected branch.

The writer lease is branch-aware: a queued check, reviewer latency, provider cooldown, or one blocked PR does not reserve the entire run. The loop rotates to another safe PR, issue, documentation defect, operational proof, or bounded product slice.

Temporary self-modifying repair workflows, encoded patch workflows, one-shot finalizers, and broad cross-repository bot write authority are not accepted steady-state mechanisms.

A run is not complete because one action succeeded or became blocked. Practical tool/runtime budget or a fresh double exit sweep with no remaining safe work is the termination condition. Detailed governance is recorded in ADR-0006.

## Testing requirements

- Strict red-green-refactor for source defects and authority-bearing behavior.
- Deterministic unit tests for parsers, fingerprints, time/freshness, versioning, and reason codes.
- Realistic filesystem tests for links, races, no-clobber behavior, sparse/hard-linked data, permission failures, and recovery.
- Provider contract tests for malformed, missing, duplicated, delayed, and contradictory evidence.
- Model artifact tests for tamper, size drift, path substitution, and stable redaction.
- Security tests for hostile Unicode/paths/archive metadata/structured inputs.
- Coverage measures production authority paths rather than excluding them.
- Packaging/release tests validate installed artifacts, metadata, compatibility, provenance, and rollback where applicable.
- Documentation tests keep the canonical graph, ADR index, ERD/UML, release/roadmap, and traceability discoverable.

Detailed philosophy is in `docs/TEST_STRATEGY.md`.

## Packaging, provenance, and release

Release only from the exact integrated protected head after required CI, security, exact coverage, packaging, SBOM/provenance, reproducibility, compatibility, migration/rollback/recovery, affected accessibility evidence, zero valid unresolved findings, and qualifying repository review/governance pass.

Build authority, attestation authority, and publication authority remain distinct. A green workflow alone is not a release.

See `docs/RELEASE_AND_ROLLBACK.md`.

## Standards and research requirements

Material security, accessibility, storage, supply-chain, and AI/model decisions use current authoritative primary sources and primary research where relevant. References are recorded in APA 7th form in Architecture, ADRs, or feature doctoring. Draft standards are identified as drafts and never represented as final.

Current cross-cutting references include NIST SP 800-218 v1.1, final SP 800-218A, SP 800-218 Rev. 1 / SSDF 1.2 Initial Public Draft as forward-looking evidence, ISO/IEC 27001:2022 + Amd 1:2024, ISO/IEC 27040:2024, OWASP ASVS 5.0.0, OWASP AISVS 1.0, SLSA 1.2, and W3C WCAG 2.2.

## Technical acceptance

A technical change is incomplete if it has only a happy path, controller stub, TODO, mock-only integration, documentation-only assertion, or predecessor-head evidence. It requires the production path, refusal/degraded behavior, resource model, security/privacy boundary, migration/rollback impact, realistic tests, exact-head evidence, and synchronized canonical documentation.