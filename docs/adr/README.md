# DiskSage Architecture Decision Records

## Status model

- **Proposed** — canonical record is under review or the decision includes planned governance not yet integrated.
- **Accepted** — canonical record and applicable implementation are integrated on protected main.
- **Superseded** — a newer ADR replaces the decision while history remains discoverable.

An ADR status never turns unimplemented functionality into shipped behavior. Implementation/evidence status belongs in `docs/TRACEABILITY.md`.

## ADR index

| ADR | Decision | Current documentation status |
| --- | --- | --- |
| [ADR-0001](0001-local-first-runtime-authority.md) | Local-first Rust runtime authority | Proposed canonicalization |
| [ADR-0002](0002-evidence-authorization-separation.md) | Separate evidence, approval, execution, receipts | Proposed canonicalization |
| [ADR-0003](0003-exact-head-live-base-repository-evidence.md) | Exact source-head + live-base repository evidence | Proposed governance baseline |
| [ADR-0004](0004-model-artifact-integrity.md) | Model artifact install/load integrity | Integrated behavior; proposed canonical record |
| [ADR-0005](0005-central-control-plane-boundary.md) | Central control plane vs local runtime ownership | Proposed canonicalization |
| [ADR-0006](0006-work-conserving-writer-lease.md) | Work-conserving maintenance + branch-local writer lease | Proposed governance baseline |
| [ADR-0007](0007-independent-review-governance.md) | Independent review realism and CODEOWNERS hold | Proposed governance baseline |
| [ADR-0008](0008-release-provenance-and-rollback.md) | Build/provenance/publication/rollback authority separation | Proposed release baseline |
| [ADR-0009](0009-stale-branch-clean-replacement-convergence.md) | Stale broad branch decomposition and clean-replacement convergence | Proposed governance baseline |
| [ADR-0010](0010-documentation-authority-and-handoff.md) | Canonical documentation authority, maturity status, and implementation handoff | Proposed documentation/governance baseline |
| [ADR-0011](0011-filesystem-object-bound-destructive-authority.md) | Bind destructive filesystem authority to the exact validated object or fail closed | Active implementation evidence; proposed canonical record |

## Required ADR content

Material ADRs include context, drivers, alternatives, decision, consequences, failure/recovery, security/governance impact, verification/acceptance, migration/rollback, and supersession conditions.

Architecture-changing PRs update an affected ADR or add a superseding ADR rather than silently changing authority, persistence, interoperability, or release contracts.

## Lifecycle rules

- ADR links remain stable after acceptance; superseding decisions add a new ADR and point back to the prior record.
- `Proposed` is never interpreted as protected-main implementation evidence.
- `Accepted` requires the canonical record and applicable behavior/governance to be integrated on protected main.
- A stale branch cannot become the canonical ADR owner by ancestry alone; convergence follows ADR-0009.
- Documentation completion is an intermediate maintenance event and hands back to implementation/verification under ADR-0010 and ADR-0006.
