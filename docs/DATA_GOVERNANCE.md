# DiskSage Data Governance, Privacy, and Retention

## Document status

**Status:** Proposed canonical governance baseline for the current documentation branch. This document describes DiskSage-owned data authority and explicit host/provider boundaries. It does not invent enterprise tenancy, legal basis, retention periods, certification, or a central application database that the product does not currently implement.

## Purpose

DiskSage is a local-first storage intelligence and conservative reclaim product. Data governance therefore starts from a restrictive question: **what information is necessary for the current local operation or explicit integration purpose, who may use it, for how long, and which component has authority to retain or export it?**

Privacy is not equivalent to indiscriminate masking. Destructive masking can make recovery, synchronization, approval, lineage, or incident evidence unusable. DiskSage instead uses purpose-bound authorization, minimization, explicit private/shareable classes, bounded retention, controlled destinations, and integrity-bound evidence.

## Data classes and authority

| Data class | Examples | Default authority | Default export posture |
| --- | --- | --- | --- |
| Local filesystem coordinates | absolute paths, filenames, directory identities | local Rust runtime/operator workstation | private; no default export |
| Storage observations | sizes, allocation, timestamps, file/provider state | local Rust observation plane | bounded/path-free where a versioned schema permits it |
| Content-derived metadata | archive structure, schema/profile metadata, digests | local Rust runtime | minimized; exact content values are not exported unless an explicit feature contract requires them |
| Provider-local identity | account/root/object identifiers, OAuth-related scope | provider integration + local record | private by default |
| Provider evidence | capacity, sync, remote checksum/durability evidence | local Rust/provider adapter | bounded/versioned evidence only |
| Approval evidence | approver attribution, rationale, exact phrase, timestamps, fingerprints | authorization boundary | restricted to the operation/receipt purpose |
| Execution/receipt evidence | result, recovery state, integrity/fingerprint fields | local operation | restricted local record where required; bounded summary may be shareable |
| Model artifact | reviewed revision, expected size/digest, local verified identity | local model-install/load boundary | model bytes are not evidence-export payloads |
| Model output | explanation, recommendation, classification proposal | advisory only | treated as untrusted; export only by explicit feature contract |
| Repository/release evidence | source/live-base identity, checks, reviews, artifacts, SBOM/provenance | GitHub/software-delivery control plane | software-delivery evidence only; never runtime filesystem authority |
| Secrets/credentials | provider tokens, model API keys, signing or GitHub credentials | owning secret store/provider | never placed in shareable evidence, logs, PR bodies, or artifacts |

## Purpose limitation

Every export or durable record must have an explicit product purpose. A field is included because it is needed to:

- make a local decision understandable;
- revalidate an exact action;
- prove an execution or recovery outcome;
- establish provider evidence required for a bounded workflow;
- support a documented integration contract; or
- prove software delivery, security, or release state.

A convenient future use is not sufficient authority to collect or retain data now.

## Shareable versus private evidence

### Shareable evidence

A shareable envelope may contain version identifiers, bounded path-free counts or aggregates, stable action/blocker/result codes, fingerprints, capability flags, explicit unknown/incomplete states, and other fields explicitly defined by a versioned contract.

Shareable evidence must not silently grow to include raw paths, provider-local account identifiers, unrestricted command output, credentials, private receipt coordinates, or unbounded model/provider payloads.

### Private evidence

Private evidence can include exact paths, provider-local object identifiers, archive offsets/ranges, detailed collision coordinates, digests, approval attribution, and operation receipts. Private evidence requires an explicit restricted local destination or an explicitly authorized host boundary. A private dossier is not uploaded merely because a shareable integration exists.

## Sensitive-data minimization

DiskSage should prefer derived structural evidence over raw content where the product decision does not require content values. Examples include bounded dataset schema profiling, archive indexes without extraction, cryptographic fingerprints, stable issue codes, and provider proof fields rather than complete provider responses.

When content access is required, parsing and retention must be bounded. Temporary buffers and staging outputs are invocation-owned and cleaned or retained only according to explicit recovery semantics.

## Retention and deletion

No universal time-based retention period is claimed without product and legal evidence. Current policy is therefore lifecycle-based and fail-closed:

- transient observation buffers exist only for the invocation unless a feature explicitly persists evidence;
- temporary staging is removed after verified success or according to a documented recovery path;
- private dossiers and receipts are created only when the workflow requires or the operator explicitly requests them;
- provider credentials follow the provider-connection lifecycle and remain outside shareable evidence;
- repository/release evidence follows GitHub/release retention rather than local runtime retention;
- future relational or service persistence must define owner, purpose, retention, deletion, backup, export, encryption, migration, and rollback before integration.

A data-rights or deletion implementation must not delete evidence that is still required for an active recovery transaction without first reaching an explicit safe recovery/closure state.

## Access control and least privilege

- Svelte UI has no ambient filesystem authority.
- Tauri exposes only allow-listed typed commands.
- Rust validates the exact current operation before mutation.
- Provider credentials are purpose-bound to their provider operation.
- Cross-service consumers receive bounded schemas, not ambient local filesystem access.
- Model output cannot grant mutation authority.
- Autonomous repository writers cannot turn GitHub credentials into runtime product authority.

If enterprise host tenancy is introduced, tenant isolation, regional/residency policy, identity mapping, key management, privileged access, retention, export, and audit are host responsibilities until an accepted ADR assigns specific ownership to DiskSage.

## Encryption and secret handling

Secrets belong in the appropriate OS/provider/GitHub secret mechanism and must not be serialized into shareable evidence. When a new durable sensitive store is introduced, its design must document encryption at rest/in transit as applicable, key ownership and rotation, backup/restore, access-purpose logging, and recovery.

Model-backed CI/development uses `NVIDIA_NIM_API_KEY` through GitHub Secrets where required. `COPILOT_GITHUB_TOKEN` is not a DiskSage model-development credential.

## Logging and diagnostics

Diagnostics are bounded and purpose-specific. Shareable logs/errors use stable reason codes rather than raw paths, tokens, provider bodies, process command output, or arbitrary untrusted strings. Exact private coordinates may appear only in a restricted operator-owned record where the feature contract requires them.

## Cross-service and provider transfer

An integration contract must declare:

1. schema/version;
2. sender and receiver purpose;
3. data classification of each field;
4. maximum size/cardinality;
5. authentication/authorization owner;
6. retention owner;
7. error/redaction behavior;
8. whether the receiver may persist or re-export the data;
9. compatibility and rollback rules.

Naruon or another CWL consumer must not infer a reusable mutation credential from a readiness/evidence envelope. contextual-orchestrator may receive only the bounded material required for an advisory model task.

## Incident and breach handling

Potential exposure of secrets, private paths, provider-local identities, receipt data, or model/provider payloads is an incident even if no filesystem mutation occurred. Follow `docs/INCIDENT_RUNBOOK.md`, preserve minimum necessary forensic evidence, rotate/revoke affected credentials through their owning systems, and avoid expanding exposure through issue/PR comments.

## Governance acceptance

A product change that collects, exports, persists, or newly exposes data is incomplete until the reviewed change states:

- data class and purpose;
- authority/owner;
- private/shareable status;
- size/resource bounds;
- retention/deletion lifecycle;
- access and secret boundary;
- failure/recovery behavior;
- tests proving the intended minimization and refusal behavior;
- traceability update.

See `docs/DATA_MODEL.md`, `docs/API_CONTRACT.md`, `docs/THREAT_MODEL.md`, and `docs/TRACEABILITY.md`.