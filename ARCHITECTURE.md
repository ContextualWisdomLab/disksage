# DiskSage architecture index

The canonical product contract is [docs/PRD.md](docs/PRD.md). This index routes product questions
to technical ownership without turning implementation details into customer-facing language.

- [Product and technical gap baseline](docs/product-technical-gap-baseline.md): dated implementation,
  incident, and pull-request inventory.
- [Architecture decision records](docs/architecture/adr/README.md): accepted safety and integration
  decisions.
- [Base desktop design](docs/superpowers/specs/2026-07-10-disksage-design.md): application layers,
  platforms, packaging, and milestones.
- [Cloud offload Goal](docs/architecture/adr/0001-cloud-offload-goal-state.md): provider-evidence state
  machine and eviction boundary.
- [Cloud transfer failure and materialization](docs/architecture/adr/0011-cloud-transfer-failure-and-materialization.md):
  failure receipts, placeholders, and adoption.

Product outcomes and customer states belong in the PRD. Technical decisions belong in ADRs and
specifications. Current implementation and open work belong in the gap baseline.
