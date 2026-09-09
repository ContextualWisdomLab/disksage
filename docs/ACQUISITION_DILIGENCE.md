# DiskSage Acquisition Diligence

## Document status

**Status:** Proposed diligence baseline. This document is a buyer/evaluator evidence map, not a valuation, certification, warranty, or claim that every gate currently passes. Protected `main` is shipped source truth; an active PR is evidence of work in progress only.

## Purpose

A buyer should be able to determine what DiskSage does, where authority lives, which risks are controlled, which claims are empirically supported, and which gaps remain without reconstructing chat history or reading every pull request. The diligence process therefore separates architectural intent, protected-main implementation, exact-head test/security evidence, released-artifact evidence, and external/legal evidence.

## Evidence authority classes

| Evidence class | What it may prove | What it must not be used to prove |
| --- | --- | --- |
| Protected-main source | integrated implementation and source-controlled policy | released-artifact behavior by itself |
| Active-PR source | proposed/current implementation under review | shipped behavior |
| Deterministic tests | behavior reached by those exact tests | untested real-world effectiveness |
| Coverage | measured execution of owned production graph | correctness, security, usability, or buyer value by itself |
| Security/static analysis | findings within scanner/test scope | absence of all vulnerabilities |
| Formal review | reviewer judgment on reviewed revision | current head after revision changes |
| Package/build evidence | build/installability for proven artifacts | provenance if artifact identity is not bound |
| SBOM/provenance/attestation | artifact/source/build identity within the attested chain | product correctness or legal rights |
| Operational acceptance | observed behavior in the specified environment | universal SLO or platform compatibility |
| External/legal evidence | rights, contracts, certifications, assessments where actually issued | technical behavior outside its scope |
| Documentation | declared requirements/architecture/operating contract | implementation if protected-main evidence is absent |

No evidence is represented as stronger than its source permits.

## Buyer evidence matrix

| Diligence area | Canonical authority | Required evidence before a strong claim | Current documentation posture |
| --- | --- | --- | --- |
| Product purpose and buyer outcomes | `docs/PRD.md` | protected-main feature path + acceptance evidence | canonicalized on this branch |
| Architecture and trust boundaries | `ARCHITECTURE.md`, ADRs, `docs/UML.md` | source/tests matching documented boundaries | canonicalized on this branch |
| Runtime safety and authorization | ADR-0001/0002, TRD | mutation-boundary tests, stale/drift/race refusal evidence | protected-main families exist; exact feature evidence remains per module |
| Data/privacy governance | `docs/DATA_GOVERNANCE.md`, threat model | export/redaction/retention/access tests for affected flows | canonicalized on this branch |
| Cloud/provider evidence | PRD/TRD/API/Data Model | provider-specific malformed/unknown/drift tests and bounded live evidence where applicable | protected-main families exist; completeness is feature-specific |
| Local model supply chain | ADR-0004, model doctoring | immutable revision + size/digest + install/load race tests + package/license evidence | protected-main integrity controls exist; broader model quality/licensing are separate |
| Accessibility | PRD/Test Strategy | keyboard/semantics/non-color/browser/webview evidence on affected workflows | requirement exists; whole-product buyer evidence must be measured |
| Reliability/recovery | Operability/Incident Runbook | deterministic failure/recovery tests + protected-main operational evidence | architecture exists; representative end-to-end recovery remains an independent gate |
| Performance/capacity | PRD/Roadmap | representative benchmark corpus, methodology, variance, hardware/platform context | no unsupported numeric guarantee is claimed |
| Coverage/code quality | Test Strategy | exact-head owned-production statement/branch/function/line evidence where tooling exposes it | target is explicit; exact current production gap must be closed rather than excluded |
| Security | `SECURITY.md`, threat model | current security scans + targeted adversarial tests + review | evidence is scoped; no certification claim |
| Dependency/supply chain | release docs, licensing docs | locked dependency graph, SBOM, action/source immutability, vulnerability/license review | release-evidence gate; current active work may strengthen it |
| Packaging/platform support | release docs | install/run/smoke evidence for each claimed supported platform/artifact | claim only what exact release evidence proves |
| Release provenance | ADR-0008, release docs | exact integrated source -> artifact digests -> SBOM/provenance -> publication proof | design baseline; stronger active work is not protected-main truth until integrated |
| Rights/IP/licensing | `docs/LICENSING_AND_NOTICES.md` | repository rights decision, dependency/model NOTICE/license inventory, contributor/IP provenance | fail closed on missing legal authority; must not invent rights |
| Modular CWL integration | Architecture/API contracts | stable schema/version compatibility and no hidden authority/database coupling | integration contracts exist by feature; each consumer proves compatibility |
| Operations/support | Operability/Incident Runbook | installation/update/recovery/runbook exercise and incident closure evidence | canonical operating baseline, not a support SLA |

## Commercial-readiness gates

DiskSage is commercially defensible only when all applicable areas below have exact evidence on one integrated release candidate:

1. **Product completeness** — core buyer journeys are implemented without demo-only or hard-coded-success paths.
2. **Safety** — mutation authority remains exact, current, human-bound where required, fail-closed, and recoverable.
3. **Quality** — owned production coverage and realistic behavioral tests meet repository policy without meaningless exclusions.
4. **Security/privacy** — current scans and adversarial tests pass; private/shareable/secret boundaries are proven.
5. **Reliability/operability** — failure, cancellation, concurrency, restart, and recovery paths are exercised where applicable.
6. **Accessibility** — affected user journeys have verifiable keyboard/semantic/non-color evidence.
7. **Interoperability** — standalone mode and versioned optional CWL/provider contracts are proven independently.
8. **Packaging/release** — supported artifacts install/run, are integrity-bound, and trace to exact integrated source.
9. **Provenance/SBOM** — artifact set, dependencies, build authority, attestation, and publication authority are independently inspectable.
10. **Rights/IP** — outbound rights, third-party obligations, model rights, contributor/IP ownership, and NOTICE obligations have actual evidence.
11. **Rollback/recovery** — product and software-delivery rollback/recovery procedures are tested or explicitly bounded.
12. **Buyer evidence** — representative workload/use-case evidence exists for the claims sales or acquisition materials intend to make.

A gap in one gate is not converted into a green claim by another gate.

## No-evidence / no-claim rule

Use the phrase **no evidence** or an equivalent explicit gap when a requested claim lacks current proof. Examples:

- no representative benchmark -> no numeric performance guarantee;
- no audited certification -> no certification claim;
- no exact released-artifact provenance -> no provenance-complete release claim;
- no rights decision -> no autonomous outbound-license claim;
- no measured recovery exercise -> no RPO/RTO claim;
- no current-head formal approval -> no claim that predecessor approval covers the current head;
- active PR only -> no claim of protected-main implementation.

Unknown evidence remains unknown; it is never coerced to success.

## Acquisition red flags that block readiness

- source or release artifacts that cannot be traced to exact integrated source;
- production mutations authorized by UI/model/heuristic state without Rust revalidation;
- stale approvals or stale PR checks treated as current;
- hidden cross-service database/credential coupling;
- broad exported path/private/provider data without purpose/retention authority;
- undocumented one-shot/self-modifying repository writers;
- production stubs, fake integrations, or hard-coded success;
- tests that avoid real production authority branches to obtain coverage;
- missing third-party/model/license/NOTICE evidence;
- unsupported claims of certification, SLO, performance, safety, or provider durability.

## Diligence package structure

For an actual buyer/release room, collect immutable evidence for the exact candidate rather than copying mutable URLs into timeless architecture:

- exact protected source revision and release tag;
- required check/security/review/ruleset evidence;
- coverage report identity;
- package inventory and artifact SHA-256 digests;
- SBOM/provenance/attestation records;
- dependency/model license and NOTICE inventory;
- compatibility/install/smoke results;
- migration/rollback/recovery exercise evidence;
- accessibility evidence for affected flows;
- representative benchmark methodology/results where claims depend on them;
- incident history/RCA closure evidence where material;
- open known risks and accepted residual risk owner.

Dated evidence belongs in release/acquisition evidence bundles, not permanent architecture claims.

## Relationship to roadmap

`docs/ROADMAP.md` prioritizes implementation gaps. This document defines the evidence a buyer requires to regard those gaps as closed. Completing documentation alone is not an acquisition-readiness event; it must hand off to implementation, exact-head validation, integration, and released-artifact proof.

See `docs/TRACEABILITY.md`, `docs/RELEASE_AND_ROLLBACK.md`, `docs/LICENSING_AND_NOTICES.md`, and `docs/DOCUMENTATION_ASSESSMENT.md`.