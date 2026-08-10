# DiskSage Documentation Index

This directory is the canonical map for product, technical, architecture, security, operability, and acquisition documentation. Feature-specific design specs and doctoring records remain valuable evidence but do not replace this cross-cutting graph.

- [Product requirements](PRD.md)
- [Technical requirements](TRD.md)
- [System architecture](../ARCHITECTURE.md)
- [Architecture decisions](adr/README.md)
- [UML and architecture diagrams](UML.md)
- [Data/evidence model and ERD](DATA_MODEL.md)
- [API, IPC, and evidence contracts](API_CONTRACT.md)
- [Threat model](THREAT_MODEL.md)
- [Test strategy](TEST_STRATEGY.md)
- [Operability and recovery](OPERABILITY.md)
- [Commercial roadmap](ROADMAP.md)
- [Release and rollback](RELEASE_AND_ROLLBACK.md)
- [Requirements/evidence traceability](TRACEABILITY.md)
- [Documentation completeness assessment](DOCUMENTATION_ASSESSMENT.md)
- [Security policy](../SECURITY.md)
- [Agent/development rules](../AGENTS.md)
- [Repository context](../CLAUDE.md)
- [Changelog](../CHANGELOG.md)

## Status language

- `protected_main` means the behavior is evidenced on the protected default branch.
- `proposed` means a reviewed documentation/architecture decision is not yet integrated.
- `planned` means the product intent is not implementation evidence.
- dated PR/run/SHA evidence belongs in review or assessment records, not timeless architecture.

## Feature-specific records

`docs/doctoring/`, `docs/architecture/`, `docs/development/`, and `docs/superpowers/` contain detailed feature or implementation evidence. Promote cross-cutting decisions into this canonical graph or an ADR when they affect product identity, authority, persistence, interoperability, security, release, or acquisition diligence.