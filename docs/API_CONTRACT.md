# DiskSage API, IPC, and Evidence Contract

## Purpose

DiskSage exposes local Tauri commands and optional cross-service evidence contracts. This document defines cross-cutting interface rules; exact feature schemas remain source-controlled beside the owning Rust/TypeScript implementation and feature doctoring.

## Authority rule

An API payload is data, not authority by itself. UI state, another service, a model response, a persisted plan, or a prior receipt cannot bypass Rust validation for the current operation.

## Command classes

### Read-only command

A read-only command includes:

- allow-listed operation identifier;
- supported request/schema version;
- bounded scope;
- explicit resource limits where applicable;
- no reusable mutation token.

A successful response may contain evidence and blockers but never silently upgrades to mutation permission.

### Planning command

A planning command may produce:

- exact action identifier/class;
- source/candidate fingerprint;
- destination/provider/account scope when applicable;
- required evidence fingerprints;
- collision/precondition evidence;
- expected unit/byte totals;
- stable blockers/unknowns;
- backend-authored confirmation phrase.

A plan is advisory until separately approved and revalidated.

### Mutating command

A mutating request requires the exact current plan and applicable authorization evidence, including attributed human approval/rationale and exact confirmation phrase. Rust revalidates current preconditions immediately before mutation.

Mutation fails closed on plan drift, stale approval, scope mismatch, collision, unsupported schema, missing required private receipt destination, or incomplete provider/filesystem evidence.

## Common evidence fields

Where applicable, a versioned evidence envelope includes:

```json
{
  "schema_version": 1,
  "observed_at_utc": "2026-08-10T00:00:00Z",
  "evidence_fingerprint": "sha256:...",
  "complete": true,
  "issue_codes": [],
  "capabilities": []
}
```

Exact field names are feature-owned and may differ, but the semantics above remain explicit. Unknown values are represented as unknown/absent with reason, never invented as zero or success.

## Approval semantics

The current cloud-copy authorization family binds a maximum 15-minute lifetime and rejects expiry or inconsistent clocks. Any operation-specific approval surface must document:

- what exact plan/scope it binds;
- whether approval is single-use or single-purpose;
- issuance/expiry semantics;
- current-state revalidation;
- stable refusal codes;
- whether a private receipt destination is required.

No frontend-generated phrase may replace an authoritative backend-defined phrase where the operation contract requires one.

## Stable failure categories

Public failures favor bounded stable categories over raw operating-system, provider, network, model, or path-bearing diagnostics. Examples used by current product families include:

- evidence incomplete/unavailable;
- plan stale;
- approval expired;
- approval clock invalid;
- approval/scope mismatch;
- destination collision/finalization failure;
- model installed artifact unavailable/not-regular/size/read/digest failure.

Exact string constants remain owned by source and tests. This document does not create aliases that are absent from code.

## Model artifact contract

The reviewed default model specification binds immutable upstream revision, exact expected byte count, and SHA-256. Installation and load-time verification remain deterministic and local. Model bytes, provider diagnostics, and local model paths are not shareable evidence by default.

## Cross-service evidence contract

A CWL integration uses:

- explicit schema/version;
- stable action/reason identifiers;
- bounded payload size/cardinality;
- path-free summaries when the contract promises path-free output;
- fingerprints/content identity;
- explicit capability negotiation;
- fail-closed handling of unknown/future versions.

The consumer receives no ambient filesystem, secret, account, or database authority.

## Naruon integration

Naruon may consume readiness/blocker/evidence summaries and action identifiers. Naruon orchestration cannot convert advisory evidence into DiskSage execution authority.

## contextual-orchestrator integration

A network model router may receive only the bounded input explicitly allowed by the product path. DiskSage retains deterministic validation, mutation authority, and receipts. Model/provider responses are untrusted data.

## Repository automation evidence contract

Repository automation maintains separate identities for:

```text
exact_source_head
live_base_tip
check_evidence
commit_status_evidence
formal_review_evidence
automated_review_evidence
scanner_evidence
workflow_run_identity
release_evidence
```

No green status, model verdict, or older-head review is promoted into another evidence class.

## Versioning and compatibility

- Unknown future schema versions fail closed.
- Backward-read compatibility is explicitly tested, not assumed.
- Renames cannot create two competing authoritative interpretations.
- A breaking integration schema change requires versioning, migration guidance, rollback/compatibility analysis, and canonical documentation updates.

## Database boundary

Cross-service APIs do not imply shared application-database access. Any future persistence follows `docs/DATA_MODEL.md` and the two-or-more-word `snake_case` object naming rule.

## Privacy boundary

Never place raw credentials, authorization headers, unrestricted command output, provider response bodies, local private paths, or model bytes in shareable API errors or evidence. Detailed debugging remains a controlled local/operator concern.