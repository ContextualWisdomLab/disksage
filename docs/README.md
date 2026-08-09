# DiskSage Documentation Index

Use this page as the canonical map of product and acquisition documentation. Feature-specific design specs and doctoring records remain valuable evidence, but they do not replace these cross-cutting documents.

- [Product requirements](PRD.md)
- [Technical requirements](TRD.md)
- [System architecture](../ARCHITECTURE.md)
- [Architecture decisions](adr/README.md)
- [UML and architecture diagrams](UML.md)
- [Data model and ERD](DATA_MODEL.md)
- [API, IPC, and evidence contracts](API_CONTRACT.md)
- [Threat model](THREAT_MODEL.md)
- [Test strategy](TEST_STRATEGY.md)
- [Operability, recovery, and support](OPERABILITY.md)
- [Requirements and evidence traceability](TRACEABILITY.md)
- [Documentation completeness assessment](DOCUMENTATION_ASSESSMENT.md)
- [Security policy](../SECURITY.md)
- [Agent development rules](../AGENTS.md)
- [Repository context](../CLAUDE.md)
- [Changelog](../CHANGELOG.md)

## Evidence status

These canonical documents are proposed in PR #137 until protected integration. Feature-specific active PRs remain labeled as such; do not treat documentation as proof that an unmerged capability is shipped.

## Doctoring and feature designs

`docs/doctoring/`, `docs/architecture/`, and `docs/superpowers/specs/` contain detailed evidence and design records for individual features. Cross-cutting decisions should be promoted into the canonical graph or an ADR when they become material to product identity, trust, persistence, interoperability, release, or acquisition diligence.