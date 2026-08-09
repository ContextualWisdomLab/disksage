# DiskSage Product Requirements Document

## Document status

**Status:** Proposed canonical product baseline in PR #137. It becomes the protected-source baseline only after protected integration. Feature statements below distinguish protected-main behavior from active pull-request work and planned work; this document does not convert an unmerged proposal into shipped functionality.

## Product vision

DiskSage is a local-first, cross-platform storage intelligence and conservative reclaim application. It helps a person understand what consumes local storage, distinguish evidence from assumptions, evaluate recovery or cleanup opportunities, and perform narrowly authorized actions without turning a scan, model answer, cloud-provider observation, or stale plan into deletion authority.

The product is designed to work independently as a Tauri desktop application and to compose with ContextualWisdomLab services through bounded, versioned evidence contracts. Standalone operation must not require Naruon, contextual-orchestrator, or the organization control plane.

## Users and buyers

### Local operator

A person who needs to recover space without accidentally deleting irreplaceable work. The operator needs clear evidence, uncertainty, reversible choices, and understandable reasons for refusal.

### Developer and power user

A person with large build caches, repositories, virtual machines, package-manager artifacts, incomplete downloads, or cloud-synchronized data. The user needs more than a directory-size viewer: they need workload-aware evidence and safe cleanup/recovery workflows.

### Enterprise evaluator or acquirer

A security, platform, or procurement reviewer needs to understand exactly which observations are local, which data can leave the workstation, what creates mutation authority, how supply-chain inputs are verified, how releases are proven, and how DiskSage can be embedded without granting another service ambient filesystem authority.

### CWL integrator

A consuming service needs bounded path-free summaries, schema/version negotiation, stable reason codes, fingerprints, and explicit capability boundaries. It must not receive raw paths, reusable mutation credentials, or implicit permission to weaken DiskSage safeguards.

## Buyer-visible problems

1. Storage pressure is easy to observe but difficult to explain safely.
2. A large file is not necessarily reclaimable; local allocation, provider synchronization, process use, provenance, and recovery value are separate facts.
3. Cloud placeholders and provider-client state make local disk usage different from remote durability.
4. Incomplete downloads and archive fragments may contain recoverable data even when the original download failed.
5. Development worktrees, caches, package-manager state, and VM images require domain-aware evidence rather than generic age heuristics.
6. Model-based advice is useful but cannot be allowed to become filesystem authority.
7. Enterprise buyers require auditability, supply-chain evidence, rollback, privacy boundaries, accessible workflows, and defensible release provenance.

## Product principles

### Local-first authority

Filesystem mutation authority stays inside the local Rust boundary. External services may provide advisory evidence or orchestration, but cannot independently authorize a DiskSage mutation.

### Evidence before action

Observation, decision support, authorization, execution, and evidence are separate product planes. Unknown, contradictory, stale, missing, or malformed evidence fails closed.

### No authority by implication

File existence is not integrity evidence. Provider-client presence is not account ownership. A quiet upload queue is not remote durability. A matching capacity estimate is not synchronization proof. A model judgment is not approval. A successful check from an older commit is not merge authority.

### Reversible or bounded mutation

Where DiskSage mutates local state, it favors no-clobber/create-new semantics, OS trash, invocation-owned rollback, exact fingerprints, and receipts. There is no permanent-delete product path in the current product contract.

### Privacy-preserving interoperability

Path-free aggregate evidence may be shared through explicit versioned contracts. Exact paths, local identifiers, sensitive offsets/digests, and operator receipts remain private unless an operator explicitly creates a restricted local artifact.

## Product capability families

| Capability family | Product outcome | Evidence status |
| --- | --- | --- |
| Storage scan and inventory | Explain local usage and surface large/unknown areas | Protected-main product family |
| Known cache/dev artifact cleanup | Identify common reclaim candidates with safeguards | Protected-main product family |
| Duplicate analysis | Find content-equivalent candidates without treating similarity as automatic delete authority | Protected-main product family |
| Ontology organization | Classify and plan organization actions with explicit targets | Protected-main product family |
| Cloud evidence and copy workflows | Separate local bytes, provider state, capacity, copy evidence, sync evidence, and eviction authority | Protected-main and active architecture work |
| Incomplete-download audit/recovery/materialization | Preserve and validate recoverable content before any bounded materialization | Protected-main product family |
| Git worktree evidence | Surface stale secondary worktree candidates without silently pruning | Protected-main product family |
| Podman reclaim evidence panel | Show privacy-safe read-only container/VM evidence | Active PR #133 |
| Acquisition architecture and documentation spine | Make trust, authority, release, and MSA contracts independently auditable | Active PR #137 |
| Release artifact attestation | Produce buyer-verifiable exact release provenance | Active stacked PR #138 |
| Desktop CSP hardening | Fail closed against unintended webview resource/navigation authority | Active PR #139 |
| Cargo acquisition metadata | Publish accurate package identity and fail-closed registry publication policy | Active PR #140 |
| Model download integrity | Bound model installation by immutable revision, exact size, digest, and race-safe publication | Active PR #141 |
| Model load-time integrity | Re-verify installed model immediately before llama initialization | Active stacked PR #142 |

## Functional requirements

### PRD-FR-001 — Bounded local observation

DiskSage shall bound scans, command/process output, archive parsing, metadata extraction, network responses, model inputs, and exported evidence so a hostile local artifact cannot create unbounded resource use merely by being inspected.

### PRD-FR-002 — Evidence classification

Every workflow that can influence a mutation shall distinguish observation, recommendation, blocker, approval, execution result, and durable receipt. A recommendation or model result shall never be interpreted as approval.

### PRD-FR-003 — Explicit human authorization

Mutating operations shall bind approval to the exact operation class, current fingerprints, scope, backend-authored confirmation phrase, attributed human approver, rationale, and a bounded freshness interval. Stale or mismatched approval shall fail closed.

### PRD-FR-004 — Current-state revalidation

A mutating path shall revalidate the current candidate/source/destination state immediately before the controlled mutation boundary. A changed plan shall require a fresh plan and approval.

### PRD-FR-005 — Private versus shareable evidence

Shareable evidence shall be path-free, bounded, schema-versioned, and explicit about unknown values. Private evidence may include local detail only in an explicitly requested restricted local record.

### PRD-FR-006 — Cloud evidence separation

DiskSage shall not collapse provider capacity, account scope, local placeholder state, provider-client presence, queue state, item synchronization, remote checksum, and local-source eviction safety into one Boolean claim.

### PRD-FR-007 — Recovery before discard

When an incomplete or fragmented artifact contains potentially recoverable content, DiskSage shall support read-only structural/recovery evidence before any discard or materialization decision.

### PRD-FR-008 — Offline model advisory path

The application may use an on-device model for advisory reasoning while remaining useful without networked model services. Model bytes and model output are untrusted until the relevant integrity/schema boundaries pass.

### PRD-FR-009 — Modular CWL integration

DiskSage shall expose stable bounded contracts suitable for optional Naruon or other CWL consumers without giving them cross-database access or hidden filesystem authority.

### PRD-FR-010 — Reproducible release evidence

A releasable product shall bind source revision, build inputs, checks, package artifacts, SBOM/provenance, review evidence, and release acceptance to the exact integrated protected head.

## Non-functional requirements

### Safety

- Fail closed on stale, malformed, contradictory, missing, or unsupported authority evidence.
- Do not follow symbolic links through a security boundary unless a specific reviewed operation explicitly requires and revalidates that behavior.
- Prefer no-clobber and create-new publication semantics.
- Keep local validation distinct from durable authorization.

### Reliability

- Model power loss, process termination, concurrent filesystem changes, provider delays, partial output, stale plans, and network/model unavailability as normal failure modes.
- Preserve source data unless a separately authorized operation governs removal.
- Provide deterministic rollback/recovery guidance for invocation-owned partial output.

### Privacy

- Minimize exported evidence and use purpose-bound access rather than blanket disclosure.
- Stable public failure codes shall not contain raw paths, account identifiers, model bytes, response bodies, or unrestricted subprocess output.

### Accessibility

Affected user workflows shall support keyboard operation, programmatic labels/status, non-color-only risk communication, and WCAG-informed review. Exact release claims require evidence from the affected integrated head.

### Performance

Parallelism and caching may improve scanning and analysis, but performance optimizations shall not weaken evidence freshness, resource bounds, race safety, or cancellation/recovery behavior.

### Quality

Owned production code targets exact 100% statement and branch coverage and, where tooling exposes them, function and line coverage. Public APIs require beginner-readable documentation. Coverage exclusions cannot be used to hide production behavior that carries authority.

## Standalone and MSA outcomes

### Standalone

A user can inspect and operate DiskSage locally with core safety boundaries intact even when every CWL network service is unavailable.

### Composed

A CWL service may consume a versioned, bounded evidence envelope or request an advisory capability. The consuming service cannot silently promote that evidence to mutation authority. Integration failure degrades the optional integration, not the standalone safety contract.

## Degraded and offline behavior

- If a provider API is unavailable, remote state remains unknown; local authority is not broadened.
- If the on-device model is missing or invalid, model-backed advice is unavailable; deterministic product functions remain available where their own prerequisites pass.
- If Naruon or contextual-orchestrator is unavailable, standalone operation remains available.
- If evidence cannot be completed within resource/time bounds, the product reports an explicit incomplete/blocking state rather than a guessed result.
- If a receipt/audit destination cannot be created safely, the associated mutation does not silently proceed without required evidence.

## Explicit non-goals

DiskSage does not:

- claim that large files are safe to delete merely because they are old or large;
- provide a permanent-delete path as a convenience shortcut;
- treat a model recommendation as human approval;
- claim remote cloud synchronization from provider-client presence or local queue silence alone;
- require a central CWL service for core standalone operation;
- expose raw private filesystem evidence as a default cross-service contract;
- treat a repository checkout, Git reference, successful workflow, or model verdict as runtime operator authorization;
- claim ISO, NIST, OWASP, SLSA, SOC 2, CSAP, or accessibility certification merely because those sources inform design;
- promote active-PR designs to shipped protected-main functionality before integration evidence exists.

## Acceptance criteria

A bounded feature is product-complete only when its intended user path, refusal/degraded behavior, authority boundary, privacy impact, rollback/recovery semantics, realistic tests, documentation, and exact-head CI/security evidence are complete. A controller stub, demo-only success path, TODO, mock-only integration, or unverified schema is not product completion.

A release is acceptable only when the exact integrated protected head satisfies repository policy, required CI/security checks, exact coverage, packaging, SBOM/provenance, compatibility, accessibility where affected, migration/rollback/recovery, review/approval requirements, and release-acceptance tests. `CHANGELOG.md` must describe the released changes and the published artifacts must be independently verifiable.

## Requirements traceability

Requirement-to-code/test/ADR/evidence mappings live in `docs/TRACEABILITY.md`. Architecture authority and trust boundaries live in `ARCHITECTURE.md`; implementation constraints live in `docs/TRD.md`; durable decisions live under `docs/adr/`.