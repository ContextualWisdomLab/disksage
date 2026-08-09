# DiskSage API, IPC, and Evidence Contract

## Scope

DiskSage is primarily a Tauri desktop application, so its public product boundary is a set of typed Tauri commands, local CLI entry points, versioned evidence/export structures, restricted private records, and optional CWL/provider adapters rather than one central HTTP API. This document defines cross-cutting contract rules; module-specific schemas remain authoritative for their exact fields.

## Contract classes

### Tauri command contract

Frontend code may call only registered allow-listed commands. Inputs are untrusted and are validated in Rust. A frontend component cannot acquire filesystem or provider authority by invoking arbitrary shell/process code.

Commands are classified as:

- read-only observation;
- plan/decision support;
- approval/authorization preparation;
- controlled mutation;
- evidence/receipt retrieval.

A command that mutates state must not share an indistinguishable API shape with a read-only command.

### CLI contract

Headless audit, recovery, planning, and materialization commands use explicit flags, bounded input roots, and stable exit/error semantics. Mutating CLIs require explicit execution intent and the operation-specific current approval/fingerprint evidence; a read-only command never silently mutates because a flag is omitted or ambiguous.

### Shareable evidence contract

Shareable structures contain only fields approved for cross-process/service use, such as schema version, stable action/reason identifiers, aggregate counts/bytes, completeness, capability flags, and cryptographic fingerprints. Unknown values remain unknown rather than being coerced to zero/false/success.

### Private evidence contract

A private dossier/receipt may include exact local paths, provider-local identifiers, source offsets, digests, collision details, and operator lineage only when the workflow explicitly supports a restricted local output. Private evidence is not a reusable cross-service mutation credential.

## Versioning

Every durable evidence or integration shape with compatibility requirements uses an explicit schema/version identifier or a stable documented compatibility rule. Readers:

- accept exactly supported historical/current versions;
- reject malformed or future unsupported versions;
- do not guess missing authority-bearing fields;
- preserve unknown/incomplete semantics.

A new version that changes authority meaning requires an ADR/Architecture/TRD/Traceability update and migration/compatibility tests.

## Stable identifiers

Where exported, action and reason identifiers must be stable machine-oriented values. Human-facing explanation may evolve independently but cannot change the underlying authority semantics.

Examples of logical contract names follow the repository naming rule: `evidence_snapshot`, `action_plan`, `approval_record`, `execution_receipt`, `capacity_evidence`, and `sync_evidence`.

## Read-only request/response invariants

A read-only operation binds:

- operation identifier;
- supported schema version;
- bounded scope;
- explicit resource limits;
- observation timestamp/fingerprint;
- completeness and issue codes.

It must not return a reusable mutation token or imply account ownership, sync completion, physical reclaimability, or deletion safety unless the operation specifically and authoritatively proves that claim.

## Mutation request invariants

A mutation request includes the operation-specific subset of:

- exact current plan/fingerprint;
- action class;
- exact source/candidate scope;
- destination/provider/account scope where applicable;
- expected units/bytes/collision result;
- attributed human approver;
- human rationale;
- exact backend-authored confirmation phrase;
- issuance and expiry timestamps;
- explicit execution intent;
- restricted receipt location where required.

Every mutation authorization is valid for exactly 15 minutes from issuance. Rust evaluates trusted UTC time and, when issuance and consumption occur in the same process, monotonic elapsed time. At the expiry boundary or later the operation fails with `approval-expired`. A current clock earlier than issuance, reversed monotonic interval, or inconsistent clock pair fails with `approval-clock-invalid`. Relevant current-state drift that changes the approved plan fails with `plan-stale`. Scope, fingerprint, action, provider/account, destination, schema, or confirmation mismatch fails closed rather than being refreshed by the UI, model, retry, workflow, or prior receipt.

Tenant-authority evidence is a separate mutation gate. If `destination_account_scope` is `organization` **or** the canonical review reason `organization-cloud-sensitive-context-needs-explicit-tenant-approval` is present, Rust requires a current, correctly formatted, non-contradictory organization-tenant authority attestation bound to the exact review decision and candidate. Missing, unknown, malformed, contradictory, stale, or mismatched tenant-authority proof fails closed with `organization-tenant-authority-attestation-required` or the more specific validation refusal. A personal-cloud candidate with neither organization signal does not require this organization attestation, though its ordinary review/approval requirements still apply. External observations, UI state, provider responses, Naruon data, and model output can never grant tenant authority.

Rust revalidates current state before the mutation boundary. Mismatch, expiry, clock inconsistency, tenant-authority failure, or plan drift fails closed and requires regeneration rather than server/UI-side approval refresh.

## Error contract

Public errors prefer stable non-sensitive codes. They do not embed:

- raw local paths;
- OAuth/bearer secrets;
- provider account identifiers unless a specific private contract requires them;
- unrestricted subprocess output;
- provider response bodies;
- model bytes or prompt content;
- PR/reviewer private data.

Detailed debugging information remains local/private where a reviewed diagnostic path exists.

## CWL integration contract

### Naruon

May consume bounded versioned path-free readiness/lineage/capacity/review evidence. It cannot convert advisory readiness into DiskSage mutation authority and does not receive a reusable local execution credential.

### contextual-orchestrator

May route explicitly enabled model-backed explanation/evaluation. Model output remains untrusted advisory data and cannot change the Rust mutation contract. DiskSage remains functional without the service.

### Central `.github`

Provides repository policy/workflow evidence only. It is not a product runtime API and cannot become local operator authority.

## Provider integration contract

Provider integrations keep fixed provider semantics separate:

- connection/account scope;
- capacity/quota;
- local provider-client runtime observation;
- file-provider/item state;
- remote object/checksum proof;
- destination collision;
- local-source eviction permission.

Adapters normalize only what the reviewed contract can prove. They do not collapse absent evidence into a generic success.

## Model contract

The local model specification and public refusal codes are part of the supply-chain interface. Active PR #141 proposes immutable revision/exact bytes/SHA-256 bounded installation; active stacked PR #142 proposes load-time re-verification and must ultimately preserve the verified artifact identity through llama initialization. These remain `active_pr`, not protected-main contract claims, until integrated.

## Repository automation contract

Automation APIs distinguish source head, live base tip, formal reviews, review threads, check runs/workflow runs, commit statuses, security findings, ruleset/branch policy, and release artifacts. A value from one class never substitutes for another. Writes use current target blob/ref identity and fail/re-evaluate on concurrent movement.

## Idempotency and concurrency

- Read-only observations are safely repeatable but are regenerated rather than treated as durable authorization.
- Mutation plans/approvals are exact-state bound and must fail after relevant drift.
- Create-new/no-clobber operations preserve pre-existing and concurrent foreign outputs.
- Retry must not duplicate receipts/artifacts unless the contract explicitly provides unique attempt identity and safe reconciliation.

## Contract testing

Each public/IPC/evidence contract needs positive, missing-field, extra/unsupported-version, malformed, oversized, stale, privacy, and authority-confusion tests as applicable. Schema fixtures must reflect real production producers/consumers. Cross-service tests prove both valid composition and fail-closed standalone degradation.

## Future HTTP/service API

A future network service must be a separate adapter over these same authority/evidence concepts. It requires explicit authentication/tenant authorization, idempotency, rate/resource limits, audit, privacy classification, OpenAPI/schema versioning, threat-model update, and an ADR. This document does not claim such a service exists today.