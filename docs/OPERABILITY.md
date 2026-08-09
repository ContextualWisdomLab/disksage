# DiskSage Operability, Recovery, and Support Guide

## Scope

This document defines operational expectations for the local DiskSage application, optional provider/model integrations, private evidence/receipts, repository automation, and release acceptance. It intentionally does not invent production SLO numbers that have not been measured.

## Operating modes

### Fully local

Core filesystem observation, deterministic planning, supported recovery/cleanup operations, and local evidence remain available without Naruon or contextual-orchestrator. On-device model advice is optional and can be unavailable independently.

### Provider-assisted

OneDrive, Google Drive, iCloud/File Provider, or local provider-client evidence may enrich or gate a cloud workflow. Provider unavailability produces explicit unknown/incomplete evidence and never broadens local mutation authority.

### CWL-composed

Naruon may consume bounded path-free evidence. contextual-orchestrator may route explicitly enabled model-backed work. A CWL service outage degrades only the integration path.

## Operator-visible health model

DiskSage should distinguish:

- `ready` or complete evidence for the requested read-only operation;
- explicit blocker or prerequisite state;
- incomplete/unknown evidence;
- expired/stale approval;
- execution failure with recovery state;
- successful operation with bounded receipt;
- optional integration unavailable.

A generic green application indicator must not hide which evidence plane is unavailable.

## Stable failure categories

Public/shareable failures use stable non-sensitive codes. Exact code vocabularies are defined by the implementing module. Operators and support tooling should never require raw paths, OAuth values, response bodies, or unrestricted subprocess output in a public report.

Private diagnostics may be stored only in explicitly requested restricted evidence when a supported workflow provides that facility.

## Common operational scenarios

### Scan cannot complete

1. Preserve the source; do not infer zero usage.
2. Report the bounded reason: permission, unreadable entry, entry/time limit, cancellation, or unsupported type.
3. Retry only after the underlying condition changes or with a reviewed broader bound.
4. Do not turn an incomplete scan into cleanup authority.

### Provider API or client unavailable

1. Mark provider-dependent evidence unknown/unavailable.
2. Keep local-only read operations available.
3. Do not claim account, capacity, sync, or eviction safety from provider-client process presence alone.
4. Retry using the same bounded scope after provider health/authentication changes.

### Capacity evidence is stale or inconsistent

Regenerate capacity evidence. The previous approval/plan is not extended. A changed capacity result can require a new plan and human approval.

### Copy or adoption fails

Preserve the source. Remove only invocation-owned partial output whose identity is proven. Preserve pre-existing or concurrently replaced destinations. Record the stable failure and recovery status in the restricted receipt path when applicable.

### Approval expires

Generate current evidence and plan again. The operator must provide a fresh approval; automation/UI cannot refresh the prior approval on the operator's behalf.

### Model missing or invalid

Disable the model-backed advisory path and continue deterministic functionality whose prerequisites pass. Active PRs #141/#142 define stronger install/load integrity and remain unintegrated until their gates pass.

### Release CI or reviewer is delayed

Treat waiting as local to that merge/release action. Do not bypass or count pending evidence as success. Repository automation should rotate to another safe PR, issue, documentation gap, or product slice and revisit after material state change.

## RCA operating procedure

For an unexpected product or repository failure:

1. capture the exact operation/PR/head/base/run/input identity;
2. identify the first failing boundary, not only the final symptom;
3. reproduce or isolate where practical;
4. inspect recent relevant changes and compare a known working path;
5. state one falsifiable root-cause hypothesis;
6. enumerate materially distinct remedies;
7. verify feasibility against actual permissions, credentials, platform semantics, dependency/stack state, writer ownership, reversibility, and blast radius;
8. apply the smallest test-first remedy;
9. rerun the exact failing path and then broader relevant validation;
10. record only evidence that belongs to the new exact state.

After three materially distinct failed hypotheses, reassess the architecture or governing contract before adding a fourth patch.

## Private evidence handling

- Create private dossiers/receipts only at explicit operator-selected destinations supported by the workflow.
- Prefer create-new behavior and restrictive permissions.
- Do not place private evidence inside a source tree or cloud destination when the workflow explicitly forbids overlap.
- Treat exact paths, offsets, account-local identifiers, digests, and detailed collision/source lineage as private.
- Retention is purpose-bound; do not keep private evidence indefinitely merely because it is useful for debugging.

## Backup and recovery

DiskSage is not a backup product. The product should retain source material unless a separately authorized operation governs its removal and should never present an unverified copy as a substitute for a backup.

For DiskSage-owned persistent evidence/configuration, a future persistence change must document backup/restore and corruption handling before release. The current conceptual data model does not imply a central database backup requirement.

## Upgrade and rollback

A release candidate must document migration and rollback whenever persistent state, evidence schemas, package metadata, model artifact identity, provider connection documents, or release formats change.

Rollback evidence binds the exact prior version/artifact/source identity. Rollback cannot be used to reintroduce a known security bypass, mutable model reference, weak CSP, stale authorization rule, or unproven release artifact simply because the older binary is available.

## Observability

Local observability should prioritize bounded structured status/reason codes, durations, aggregate counts/bytes, resource-bound outcomes, and recovery results while excluding sensitive paths/secrets by default.

Repository observability uses exact GitHub workflow/check/review/security state. A PR body or chat report is not the source of truth.

## SLO posture

DiskSage is still early development and does not claim a production GA availability, latency, MTTR, or support-response SLO in this document. Before a numeric SLO is adopted, collect representative workloads and define:

- scan completion and cancellation behavior by workload class;
- UI responsiveness under large trees;
- provider observation latency/failure rates;
- recovery success for interrupted mutations;
- package startup/build/install reliability across supported platforms;
- error-budget ownership and measurement source.

Numeric goals must be tested and monitored before they become contractual buyer claims.

## Release operational acceptance

Before release:

- exact protected source and package version are aligned;
- required tests/coverage/security/review gates pass;
- clean package/install smoke tests pass on supported platforms;
- SBOM/provenance/checksums/attestation requirements pass;
- migration/rollback/recovery instructions are exercised where applicable;
- private/shareable evidence boundaries are unchanged or explicitly reviewed;
- affected accessibility paths are verified;
- `CHANGELOG.md` and authoritative docs describe the integrated state, not an active-PR predecessor.

After release, verify the published artifact identity and basic startup/critical-path behavior rather than assuming publication succeeded because the workflow ended green.

## Acquisition and support handoff

A buyer/support engineer should be able to navigate from `README.md` to PRD, TRD, Architecture, ADRs, UML, data model, threat model, test strategy, traceability, security policy, and changelog without reconstructing conversation history. Operational gaps discovered during diligence are defects or explicitly tracked planned work, not tribal knowledge.