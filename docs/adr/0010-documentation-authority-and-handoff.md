# ADR-0010 — Treat canonical documentation as versioned authority with mandatory implementation handoff

**Status:** Proposed governance decision.

## Context

DiskSage accumulated strong feature doctoring, README material, PR descriptions, chat decisions, and source code without one canonical cross-cutting product/technical/architecture graph. That makes diligence and maintenance depend on archaeology. The opposite failure is also dangerous: a comprehensive documentation PR can be mistaken for shipped product completeness or used as a reason to stop implementation work.

The repository needs a durable distinction between documentation authority, implementation maturity, and execution-loop progress.

## Drivers

- make product truth reconstructable from GitHub without chat history;
- prevent PR bodies and transient comments from becoming permanent architecture;
- prevent active PRs/plans from being described as protected-main behavior;
- keep PRD/TRD/Architecture/ADR/UML/ERD/API/security/test/operability/release/traceability semantically synchronized;
- turn documentation-discovered gaps into executable product work;
- avoid duplicate competing documentation branches.

## Alternatives considered

1. Keep architecture mainly in README/PR descriptions — rejected because authority is fragmented and transient.
2. Generate a large one-time documentation pack and consider the repository documented forever — rejected because code and evidence evolve.
3. Treat documentation as advisory only — rejected because requirements, ownership, non-goals, release criteria, and decisions need durable reviewable authority.
4. Maintain one canonical versioned documentation graph with machine checks and mandatory implementation handoff — selected.

## Decision

DiskSage maintains one discoverable canonical documentation graph, indexed from `docs/README.md`, with repository-convention authorities for PRD, TRD, Architecture, ADRs, UML, conceptual/logical data model/ERD, API/evidence contracts, security/threat model, data governance, test strategy, operability/incident recovery, roadmap, release/rollback, licensing/IP, acquisition diligence, traceability, and repository governance.

Documentation status and implementation maturity are separate.

Documentation families use fitness classifications:

- `PRESENT_CURRENT`;
- `PRESENT_STALE`;
- `PARTIAL`;
- `MISSING`;
- `NOT_APPLICABLE`;
- `SUPERSEDED`;
- `OWNED_BY_ACTIVE_PR`.

Capability maturity uses:

- `IMPLEMENTED_ON_PROTECTED_MAIN`;
- `IMPLEMENTED_ON_ACTIVE_PR`;
- `PARTIAL`;
- `ACCEPTED_ARCHITECTURE`;
- `PLANNED`;
- `RESEARCH_ONLY`;
- `SUPERSEDED`;
- `DOWNSTREAM`;
- `REJECTED`;
- `OUT_OF_SCOPE`.

A chat statement, issue, PR body, active PR, design diagram, or target architecture is never promoted to `IMPLEMENTED_ON_PROTECTED_MAIN` without protected-main evidence.

If a documentation family is genuinely not applicable, the canonical graph states why. DiskSage must not invent a database or other component merely to fill an expected diagram family.

Machine-checkable documentation contracts enforce required families, index/link discoverability, ADR lifecycle, Mermaid/code-block structure, current state/entity/API names, conceptual-versus-persisted labels, status vocabulary, ownership boundaries, and selected conversation-derived governance decisions.

## Mandatory handoff

A documentation audit or documentation PR is never a terminal maintenance outcome while safe work remains. After a material documentation mutation, the work-conserving loop rebuilds the full executable queue and executes the highest-priority safe non-documentation action exposed by the audit.

Examples:

- test strategy reveals real coverage deficit -> create/advance realistic production-boundary tests, not a coverage exclusion;
- release diligence reveals missing provenance -> advance the release provenance implementation lane;
- privacy governance reveals an overbroad export -> add a test-first minimization repair;
- ERD/data model reveals ambiguous persistence ownership -> clarify or implement the actual owner, never invent tables;
- incident runbook reveals missing recovery evidence -> implement/rehearse the bounded recovery path.

## Consequences

Documentation becomes a reviewed product interface rather than prose inventory. More changes may update several linked documents in one coherent PR, but implementation claims become more conservative and traceable. A green docs test proves documentation structure/markers only; it does not prove product behavior.

## Failure and recovery

If docs contradict protected main, protected-main implementation is the shipped source evidence and the documentation is marked stale until corrected or an implementation change is accepted. If an active PR becomes stale or is closed, its maturity classification changes accordingly.

If multiple documentation branches compete, select the current canonical owner from fresh evidence, absorb every non-duplicative valuable change, and close/supersede the duplicate only after semantic preservation is proven under ADR-0009.

## Security/governance impact

This decision prevents unreviewed chat/PR prose from silently redefining authority, prevents planned security controls from being represented as shipped, and makes privacy/release/licensing gaps visible. It also prevents documentation completion from masking unresolved source defects.

## Verification/acceptance

`src/lib/architectureDocumentation.test.ts` and related repository tests verify the canonical family and critical markers. Reviews must also compare semantic claims with current code/workflows; substring/file-existence checks alone are not sufficient diligence.

A documentation baseline is considered integrated only after the exact documentation head passes current repository gates and merges into protected main. Acquisition or commercial readiness remains separately governed by exact product/release evidence.

## Migration/rollback

Consolidate scattered durable decisions into the canonical graph while retaining feature-specific doctoring for detailed evidence. Do not delete historical evidence merely because it is no longer canonical. Rollback may revert a defective documentation change but must preserve known gaps and not restore stale claims as current.

## Supersession

Supersede if the repository adopts a stronger machine-readable requirements/architecture system that preserves the same status separation, traceability, semantic review, and implementation handoff guarantees.