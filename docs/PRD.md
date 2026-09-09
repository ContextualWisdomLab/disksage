# DiskSage Product Requirements Document

## Document status

**Status:** Proposed canonical product baseline for a clean current-main documentation replacement. Capability status is evidence-based: implemented behavior is separated from planned work and no chat or pull-request prose is treated as shipped truth by itself.

## Product vision

DiskSage is a local-first storage intelligence and conservative reclaim product. It helps a person understand storage pressure, recover useful content, identify reclaim opportunities, and execute narrowly authorized changes without turning a scan, heuristic, model answer, provider observation, or stale plan into deletion authority.

DiskSage must remain independently useful as a desktop application while exposing bounded versioned integration seams for ContextualWisdomLab services.

## Users and buyers

### Local operator

Needs clear storage evidence, understandable uncertainty, reversible or bounded actions, and refusal reasons that do not require reading source code.

### Developer and power user

Needs domain-aware handling of build artifacts, worktrees, package caches, virtualized/container storage, incomplete downloads, archives, and cloud-synchronized data.

### Enterprise evaluator or acquirer

Needs a defensible answer to: what data leaves the workstation, which component holds mutation authority, how human approval is bound, how supply-chain inputs are verified, what failure/rollback means, how releases are proven, and how optional CWL composition works without ambient trust.

### CWL integrator

Needs versioned path-free evidence, stable action/reason identifiers, schema/capability negotiation, and explicit authority boundaries. An integration consumer must not receive a reusable filesystem mutation credential.

## Buyer-visible problems

1. A storage scanner can report bytes without proving reclaimability.
2. A large or old file can still be valuable, active, shared, deduplicated, cloud-only, or unrecoverable after deletion.
3. Cloud capacity, local materialization, provider runtime, synchronization, remote durability, and local eviction safety are different facts.
4. Incomplete downloads and archive fragments may contain recoverable content and should not be discarded by filename heuristics.
5. Developer worktrees, caches, containers, and VMs require domain-aware evidence.
6. AI advice is useful but unsafe if it can acquire filesystem authority by implication.
7. Buyers need deterministic security controls, privacy boundaries, operability, provenance, accessible UX, and reproducible release evidence.

## Product principles

### Local-first authority

Security-relevant filesystem authorization and mutation remain in the local Rust boundary.

### Evidence before action

Observation, decision support, blockers, approval, execution, receipts, repository evidence, and release evidence remain separate classes.

### No authority by implication

File existence is not integrity evidence. Provider-client presence is not account ownership. Queue silence is not remote durability. Capacity is not sync. A model answer is not approval. A successful workflow from an older commit is not merge or release authority.

### Bounded and reversible mutation

Use exact fingerprints, current-state revalidation, create-new/no-clobber semantics, OS trash or other reversible mechanisms where applicable, and invocation-owned recovery evidence. The current product contract does not use permanent deletion as a convenience shortcut.

### Privacy-preserving interoperability

Share only the evidence needed for the receiving purpose. Path-bearing/private evidence remains local unless explicitly exported to a restricted destination.

## Product modes

### Inspect

Read-only storage inventory, capacity, archive/file metadata, provider state, worktree/container evidence, and uncertainty.

### Explain

Deterministic and optional model-assisted interpretation of evidence. Explanations are advisory.

### Plan

Produce exact candidate/action plans, blockers, required evidence, destination/provider scope, fingerprints, and backend-authored confirmation phrases.

### Execute

Perform only a currently authorized, revalidated operation through Rust-owned mutation boundaries.

### Prove

Return bounded result evidence, restricted receipts where required, and release/acquisition evidence for software delivery.

## Functional requirements

### PRD-FR-001 — Bounded observation

DiskSage shall bound filesystem scans, parser depth/cardinality, archive inspection, command/process output, network/provider responses, model input/output, and exported evidence.

### PRD-FR-002 — Explicit evidence classes

Every authority-bearing workflow shall distinguish observation, decision support, blocker, approval, execution result, and receipt evidence.

### PRD-FR-003 — Exact human authorization

A mutating operation shall bind approval to the exact current action plan, operation class, current fingerprints, relevant destination/provider/account scope, backend-authored phrase, attributed human approver, rationale, and bounded freshness.

### PRD-FR-004 — Mutation-time revalidation

DiskSage shall revalidate the current source/candidate/destination state immediately before mutation. Plan drift requires a fresh plan and approval.

### PRD-FR-005 — Private/shareable evidence separation

Shareable evidence shall be bounded, versioned, path-free where the contract promises path-free output, and explicit about unknown values. Private evidence requires an explicit restricted local destination.

### PRD-FR-006 — Provider evidence separation

DiskSage shall not collapse provider account scope, runtime presence, capacity, placeholder state, queue state, item synchronization, remote durability, copy receipt, and local eviction safety into a single Boolean claim.

### PRD-FR-007 — Recovery before discard

For supported incomplete or fragmented artifacts, the product shall offer bounded read-only recovery/structure evidence before a discard decision.

### PRD-FR-008 — Supply-chain-bound local model

The on-device model shall be treated as executable supply-chain input. Model installation and loading must verify the reviewed artifact identity, while model behavior remains advisory.

### PRD-FR-009 — Standalone operation

Core local functions shall remain available without Naruon, contextual-orchestrator, or an organization runtime service.

### PRD-FR-010 — Modular CWL integration

Cross-service integrations shall use explicit versioned bounded contracts and must not bypass DiskSage authorization.

### PRD-FR-011 — Audit and recovery evidence

Applicable mutations shall produce enough bounded evidence to determine what was attempted, what was created or retained, and what recovery/rollback state remains.

### PRD-FR-012 — Reproducible release evidence

A release shall bind exact integrated source, build inputs, checks, review/governance evidence, package artifacts, SBOM/provenance, changelog/version, and release acceptance.

## Non-functional requirements

### Safety and security

- Fail closed on stale, malformed, contradictory, missing, unsupported, or resource-incomplete authority evidence.
- Treat links, non-regular files, provider responses, imported records, model artifacts, and model output as untrusted.
- Preserve least privilege and explicit purpose boundaries.
- Never weaken security or tests to make a PR mergeable.

### Reliability

- Treat concurrent filesystem mutation, provider delay, process termination, power loss, partial output, stale plans, permission changes, and external outages as normal failure modes.
- Preserve source material unless separately authorized.
- Provide deterministic bounded recovery or explicit recovery-required states.

### Privacy

- Minimize exported evidence.
- Do not place secrets, raw paths, provider-local identifiers, unrestricted command output, response bodies, or model bytes in shareable errors/evidence.
- Prefer purpose-bound authorization, encryption, retention, and access control over destructive blanket masking.

### Accessibility

Affected desktop workflows shall support keyboard interaction, programmatic labels/status, non-color-only risk cues, and evidence aligned with current WCAG 2.2 guidance where applicable.

### Performance

Parallelism, caching, and GPU/CPU use may improve throughput, but no optimization may weaken freshness, resource bounds, cancellation/recovery, or race safety. Representative buyer workloads must be benchmarked before numeric performance/SLO claims are published.

### Quality

Owned production code targets exact 100% statement and branch coverage and, where tooling exposes them, exact function and line coverage. Public APIs require beginner-readable rustdoc/JSDoc/docstrings. Coverage exclusions cannot hide production authority behavior.

### Operability

Failures must produce stable bounded reason codes and actionable operator recovery guidance. Measured SLOs may be added only after representative operational evidence exists.

## Standalone and MSA outcomes

### Standalone

The desktop product retains core deterministic safety boundaries without any CWL service.

### Composed

A CWL consumer may request an advisory capability or consume a bounded evidence envelope. It cannot convert that relationship into ambient filesystem, secret, or database authority.

## Degraded and offline behavior

- Provider API unavailable → remote state remains unknown; local authority is not broadened.
- Optional model unavailable or invalid → model-backed explanation is unavailable; deterministic functions remain available where their own prerequisites pass.
- Naruon/contextual-orchestrator unavailable → standalone product remains usable.
- Resource/time bound exceeded → explicit incomplete/blocking state; no guessed success.
- Receipt/private evidence destination cannot be established safely → associated mutation fails closed when that evidence is required.
- Central CI/reviewer outage → affected merge/release evidence remains pending; product policy is not weakened.

## Explicit non-goals

DiskSage does not:

- declare a file safe to delete because it is merely large or old;
- provide permanent deletion as a default convenience path;
- treat AI/model output as human authorization;
- claim provider synchronization from runtime presence, capacity, or queue silence alone;
- require another CWL product for core standalone operation;
- export raw private filesystem evidence by default;
- use repository state as runtime operator authorization;
- claim ISO/NIST/OWASP/SLSA/SOC 2/CSAP/accessibility certification from references alone;
- promote planned or unmerged work to shipped functionality in documentation.

## Acceptance criteria

A bounded feature is product-complete only when its user/API path, refusal/degraded behavior, authority boundary, privacy impact, resource bounds, recovery/rollback semantics, realistic tests, documentation, and exact-head CI/security evidence are complete. Controller stubs, demo-only success paths, TODOs, mock-only integrations, or source-text-only “tests” are not completion.

A release is acceptable only from the exact integrated protected head after current repository policy, required CI/security, exact coverage, packaging, SBOM/provenance, compatibility, affected accessibility evidence, migration/rollback/recovery, zero valid unresolved findings, and required review/approval pass. The released artifact must be independently verifiable.

## Commercial readiness outcomes

The acquisition-quality bar is evidence-driven, not a valuation claim. Buyer confidence should be traceable to:

- safe end-to-end reclaim/recovery workflows;
- reproducible security and release evidence;
- representative performance/capacity benchmarks;
- operator-visible recovery and auditability;
- explicit privacy/data boundaries;
- accessible desktop behavior;
- stable standalone and modular integration contracts;
- documented support/upgrade/rollback policy.

The prioritized path is maintained in `docs/ROADMAP.md` and requirement-to-evidence mapping in `docs/TRACEABILITY.md`.