# DiskSage Documentation Index

This directory is the canonical map for product, technical, architecture, quality, accessibility, interoperability, observability, security, data governance, operability, release, licensing, standards, and acquisition documentation. Feature-specific design specs and doctoring records remain valuable evidence but do not replace this cross-cutting graph.

## Canonical graph

- [Product requirements](PRD.md)
- [Technical requirements](TRD.md)
- [System architecture](../ARCHITECTURE.md)
- [Architecture decisions](adr/README.md)
- [UML and architecture diagrams](UML.md)
- [Data/evidence model and conceptual ERD](DATA_MODEL.md)
- [API, IPC, and evidence contracts](API_CONTRACT.md)
- [Product quality attributes and acceptance evidence](QUALITY_ATTRIBUTES.md)
- [Accessibility acceptance](ACCESSIBILITY_ACCEPTANCE.md)
- [Standalone and CWL interoperability](INTEROPERABILITY.md)
- [Privacy-safe observability and evidence boundary](OBSERVABILITY.md)
- [Data governance, privacy, and retention](DATA_GOVERNANCE.md)
- [Threat model](THREAT_MODEL.md)
- [Test strategy](TEST_STRATEGY.md)
- [Operability and recovery](OPERABILITY.md)
- [Incident, RCA, and recovery runbook](INCIDENT_RUNBOOK.md)
- [Commercial roadmap](ROADMAP.md)
- [Release and rollback](RELEASE_AND_ROLLBACK.md)
- [Licensing, IP, and NOTICE evidence](LICENSING_AND_NOTICES.md)
- [Standards and primary-reference registry](STANDARDS_AND_REFERENCES.md)
- [Acquisition diligence](ACQUISITION_DILIGENCE.md)
- [Requirements/decisions/evidence traceability](TRACEABILITY.md)
- [Documentation completeness assessment](DOCUMENTATION_ASSESSMENT.md)
- [Security policy](../SECURITY.md)
- [Agent/development rules](../AGENTS.md)
- [Repository context](../CLAUDE.md)
- [Changelog](../CHANGELOG.md)
- [Outbound repository license](../LICENSE)

## Authority and status language

Documentation fitness uses `PRESENT_CURRENT`, `PRESENT_STALE`, `PARTIAL`, `MISSING`, `NOT_APPLICABLE`, `SUPERSEDED`, and `OWNED_BY_ACTIVE_PR` as defined in `DOCUMENTATION_ASSESSMENT.md`.

Capability maturity uses `IMPLEMENTED_ON_PROTECTED_MAIN`, `IMPLEMENTED_ON_ACTIVE_PR`, `PARTIAL`, `ACCEPTED_ARCHITECTURE`, `PLANNED`, `RESEARCH_ONLY`, `SUPERSEDED`, `DOWNSTREAM`, `REJECTED`, and `OUT_OF_SCOPE` as defined in `TRACEABILITY.md`.

An active PR, chat statement, issue, diagram, or target architecture is not protected-main implementation evidence. Dated PR/run/SHA evidence belongs in review, release, incident, or diligence records rather than timeless architecture.

## Canonical ownership rules

- One active branch owns the canonical cross-cutting documentation graph.
- Feature-specific doctoring remains authoritative for detailed local evidence when consistent with protected main.
- Cross-cutting decisions affecting product identity, authority, persistence, quality, accessibility, privacy, interoperability, observability, security, incident response, repository governance, release, licensing, standards, or acquisition diligence are promoted into this graph or an ADR.
- A stale broad documentation/source branch is not canonical merely because it was created earlier. ADR-0009 governs semantic convergence and clean replacements.
- Documentation completion is never equivalent to product/release readiness. ADR-0010 requires implementation/evidence handoff whenever a safe gap remains.

## Feature-specific records

`docs/doctoring/`, `docs/architecture/`, `docs/development/`, and `docs/superpowers/` contain detailed feature or implementation evidence. They should link to or be indexed by the canonical graph when their decisions become cross-cutting, while preserving historical evidence rather than duplicating it into competing authorities.