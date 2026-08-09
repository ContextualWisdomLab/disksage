# DiskSage Architecture Decision Records

## Status model

- **Proposed** — decision is documented but this documentation branch is not yet protected-main authority, or implementation is still active PR work.
- **Accepted** — decision is integrated into protected main and remains current.
- **Superseded** — a newer ADR replaces the decision; the older record remains for history.

Do not infer shipped functionality from an ADR status alone. Implementation and evidence status are tracked in `docs/TRACEABILITY.md`.

## ADR index

| ADR | Decision | Status in this branch |
| --- | --- | --- |
| [ADR-0001](0001-local-first-runtime-authority.md) | Keep filesystem mutation authority local and Rust-owned | Proposed; reflects current architecture and becomes canonical after protected integration |
| [ADR-0002](0002-evidence-authorization-separation.md) | Separate observation, decision support, approval, execution, and receipts | Proposed; strongly represented by existing product code/architecture |
| [ADR-0003](0003-exact-head-repository-evidence.md) | Bind repository decisions to exact source head and live base, keeping evidence classes separate | Proposed; repository-governance baseline in PR #137 |
| [ADR-0004](0004-model-artifact-integrity.md) | Treat the local GGUF as executable supply-chain input and verify it at install and load | Proposed; implementation spans active PRs #141 and #142 |
| [ADR-0005](0005-central-control-plane-boundary.md) | Keep CWL central automation external to local runtime authority and preserve standalone operation | Proposed; architecture baseline in PR #137 |

## Required ADR content

A material ADR should identify context, decision drivers, considered alternatives, decision, consequences, failure/recovery behavior, security/governance impact, verification/acceptance, migration/rollback, and supersession conditions. Architecture-changing pull requests must update the affected ADR or add a superseding ADR rather than silently changing the contract.