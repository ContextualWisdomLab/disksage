# DiskSage Requirements, Decisions, and Evidence Traceability

## Purpose

This map connects product requirement -> architecture decision -> implementation surface -> test/evidence. It prevents chat history, PR prose, an active branch, or one green status from becoming an undocumented source of truth.

Capability maturity vocabulary:

- `IMPLEMENTED_ON_PROTECTED_MAIN` — representative implementation is integrated on current protected main.
- `IMPLEMENTED_ON_ACTIVE_PR` — implementation exists only on a currently active branch/PR and is not shipped truth.
- `PARTIAL` — only part of the required capability/evidence is implemented.
- `ACCEPTED_ARCHITECTURE` — accepted design authority exists without complete implementation evidence.
- `PLANNED` — prioritized intent only.
- `RESEARCH_ONLY` — investigation/evidence without product commitment.
- `SUPERSEDED` — replaced implementation/decision retained for history.
- `DOWNSTREAM` — owned by another component/repository/host boundary.
- `REJECTED` — explicitly rejected approach.
- `OUT_OF_SCOPE` — intentionally excluded from current product boundary.

Documentation fitness is independently classified in `docs/DOCUMENTATION_ASSESSMENT.md`. In particular, `OWNED_BY_ACTIVE_PR` documentation is not protected-main authority.

## Product requirement traceability

| Requirement | Decision / architecture | Representative implementation or evidence | Maturity / evidence status |
| --- | --- | --- | --- |
| PRD-FR-001 bounded observation | ARCHITECTURE resource bounds; ADR-0002 | Rust scanners/parsers and feature-specific bounds | `IMPLEMENTED_ON_PROTECTED_MAIN`; feature tests/doctoring |
| PRD-FR-002 evidence classes | ADR-0002 | planner/evidence/receipt types across Rust modules | `IMPLEMENTED_ON_PROTECTED_MAIN`; canonical cross-cutting docs `IMPLEMENTED_ON_ACTIVE_PR` |
| PRD-FR-003 exact human authorization | ADR-0002 | `src-tauri/src/cloud_transfer.rs`, frontend review projection | `IMPLEMENTED_ON_PROTECTED_MAIN`; current approval tests |
| PRD-FR-004 mutation-time revalidation | ADR-0002 | cloud transfer/materialization and identity-aware mutation paths | `IMPLEMENTED_ON_PROTECTED_MAIN` feature families |
| PRD-FR-005 private/shareable evidence | ADR-0001, ADR-0002, DATA_GOVERNANCE | private dossiers/receipts and path-free evidence envelopes | product families `IMPLEMENTED_ON_PROTECTED_MAIN`; governance `IMPLEMENTED_ON_ACTIVE_PR` |
| PRD-FR-006 provider evidence separation | ADR-0001, ADR-0002 | cloud/provider capacity, runtime, queue, sync evidence code | `IMPLEMENTED_ON_PROTECTED_MAIN` feature tests/doctoring |
| PRD-FR-007 recovery before discard | ADR-0001 | incomplete-download audit/recovery/materialization modules | `IMPLEMENTED_ON_PROTECTED_MAIN` |
| PRD-FR-008 supply-chain-bound local model | ADR-0004 | `src-tauri/src/llm/model.rs`, `src-tauri/src/llm/installed_model.rs` | `IMPLEMENTED_ON_PROTECTED_MAIN`; model integrity regressions |
| PRD-FR-009 standalone operation | ADR-0001, ADR-0005 | Tauri/Rust local runtime; optional integrations | `IMPLEMENTED_ON_PROTECTED_MAIN` architecture |
| PRD-FR-010 modular CWL integration | ADR-0005 | Naruon lineage/readiness schemas; optional orchestrator boundary | `PARTIAL` across protected-main feature contracts; consumer compatibility remains per integration |
| PRD-FR-011 audit/recovery evidence | ADR-0002, INCIDENT_RUNBOOK | execution receipts, private dossier/journal evidence | runtime families `IMPLEMENTED_ON_PROTECTED_MAIN`; canonical incident lifecycle `IMPLEMENTED_ON_ACTIVE_PR` |
| PRD-FR-012 reproducible release evidence | ADR-0008 | `.github/workflows/test.yml`, `.github/workflows/release.yml`, organization controls | baseline `IMPLEMENTED_ON_PROTECTED_MAIN`; stronger provenance `IMPLEMENTED_ON_ACTIVE_PR`/`PARTIAL` until integrated/proven |

## Conversation-to-repository decisions

| Durable decision | Canonical record | Implementation/evidence classification |
| --- | --- | --- |
| Local Rust keeps filesystem mutation authority | PRD, TRD, ARCHITECTURE, ADR-0001 | `IMPLEMENTED_ON_PROTECTED_MAIN` architecture |
| Observation/recommendation/approval/execution/receipt are separate | TRD, DATA_MODEL, ADR-0002, UML | product `IMPLEMENTED_ON_PROTECTED_MAIN`; cross-cutting docs `IMPLEMENTED_ON_ACTIVE_PR` |
| Exact current source head and independently resolved live base are required for repository decisions | TRD, ADR-0003, UML | governance `IMPLEMENTED_ON_ACTIVE_PR`; current automation operationalizes it |
| GitHub check/status/formal-review/model/scanner evidence classes remain separate | TRD, ADR-0003 | governance contract `IMPLEMENTED_ON_ACTIVE_PR` |
| One DiskSage writer lease; waiting is local; report/prompt/docs/one action are not run completion | TRD, ADR-0006, UML | scheduler governance active; canonical repo docs `IMPLEMENTED_ON_ACTIVE_PR` |
| User redirection about premature stopping is control-loop incident evidence and requires same-invocation work handoff | ADR-0006, ADR-0010, INCIDENT_RUNBOOK | scheduler governance active; canonicalization `IMPLEMENTED_ON_ACTIVE_PR` |
| RCA must identify first failing boundary, generate distinct remedies, verify feasibility, execute root-cause-changing remedy, and prove recovery | INCIDENT_RUNBOOK, UML, ADR-0006 | `IMPLEMENTED_ON_ACTIVE_PR` canonical governance; product-specific tests remain per incident |
| No temporary self-modifying/encoded-patch/one-shot repair workflows as steady-state mechanism | TRD, AGENTS, ADR-0006 | governance `IMPLEMENTED_ON_ACTIVE_PR`; obsolete historical mechanisms are not canonical |
| Stale broad PR closes only after every valuable unique delta is integrated, cleanly preserved, or explicitly rejected | ADR-0009, UML, DOCUMENTATION_ASSESSMENT | `IMPLEMENTED_ON_ACTIVE_PR` governance; active convergence work continues |
| Old checks/reviews/approvals never transfer to replacement heads | ADR-0003, ADR-0009 | governance `IMPLEMENTED_ON_ACTIVE_PR` |
| One canonical implementation owner per overlapping concern and one canonical documentation owner | ADR-0009, ADR-0010 | `IMPLEMENTED_ON_ACTIVE_PR` governance |
| Documentation family existence is not sufficiency; active PR never equals protected-main truth | ADR-0010, DOCUMENTATION_ASSESSMENT | `IMPLEMENTED_ON_ACTIVE_PR` canonical governance |
| Documentation work must hand back to highest-priority safe non-documentation work | ADR-0010, ADR-0006 | scheduler governance active; canonicalization `IMPLEMENTED_ON_ACTIVE_PR` |
| Autonomous development uses OpenCode + `NVIDIA_NIM_API_KEY`, never `COPILOT_GITHUB_TOKEN` for model execution | TRD, AGENTS, ADR-0006 | governance contract `IMPLEMENTED_ON_ACTIVE_PR` |
| Database/evidence names use 2+ descriptive `snake_case` words; no DB is invented for ERD | PRD, TRD, DATA_MODEL, AGENTS | canonical quality rule `IMPLEMENTED_ON_ACTIVE_PR`; physical central DB `OUT_OF_SCOPE` |
| Purpose-bound data governance is preferred over blanket masking; private/shareable/retention/secret authority are explicit | DATA_GOVERNANCE, DATA_MODEL, ARCHITECTURE | product controls mixed `IMPLEMENTED_ON_PROTECTED_MAIN`; cross-cutting governance `IMPLEMENTED_ON_ACTIVE_PR` |
| Licensing/IP/NOTICE/SBOM evidence must match exact release artifacts; missing rights must not be invented | LICENSING_AND_NOTICES, ACQUISITION_DILIGENCE, ADR-0008 | `IMPLEMENTED_ON_ACTIVE_PR` governance; exact release rights evidence remains independent |
| Release requires exact integrated head, provenance, rollback, artifact verification, and applicable rights evidence | PRD, TRD, ADR-0008, RELEASE_AND_ROLLBACK, LICENSING_AND_NOTICES | baseline `IMPLEMENTED_ON_PROTECTED_MAIN`; stronger release evidence `PARTIAL`/active work |
| No evidence means no strong buyer claim; docs/certification/benchmarks do not substitute for exact evidence | ACQUISITION_DILIGENCE | `IMPLEMENTED_ON_ACTIVE_PR` diligence governance |

## Active clean-replacement ownership map

This section names concern ownership categories, not transferable CI authority. Exact branch/PR identities remain dated live evidence and must be re-fetched before action.

| Concern | Current owner class | Maturity rule |
| --- | --- | --- |
| Canonical acquisition documentation | one clean current-main documentation PR | `IMPLEMENTED_ON_ACTIVE_PR` until protected merge |
| Organization-tenant fail-closed authorization repair | clean current-main source PR | `IMPLEMENTED_ON_ACTIVE_PR` until protected merge |
| Exact-head production coverage enforcement | dedicated coverage workflow PR | mechanism `IMPLEMENTED_ON_ACTIVE_PR`; measured product coverage remains `PARTIAL` until target is real |
| Privacy-safe Podman desktop evidence | clean current-main product replacement | `IMPLEMENTED_ON_ACTIVE_PR`/`PARTIAL`; stale predecessor evidence does not transfer |
| Release artifact attestation/provenance | clean current-main release replacement | `IMPLEMENTED_ON_ACTIVE_PR`/`PARTIAL`; stale stacked predecessor evidence does not transfer |

Stale broad predecessors remain historical/noncanonical until ADR-0009 convergence proves closure safe.

## Model artifact traceability

| Control | Source | Evidence |
| --- | --- | --- |
| Immutable model identity | `src-tauri/src/llm/model.rs` | immutable revision/size/SHA-256 source contract |
| Bounded/race-resistant install | `src-tauri/src/llm/model.rs` | deterministic installer/race regressions and doctoring |
| Load-time verification | `src-tauri/src/llm/installed_model.rs` | missing/link/type/size/digest/identity tests |
| Verified identity retained through llama load | installed-model + engine integration | integrated source and regression/doctoring |
| Model rights/integrity separation | `docs/LICENSING_AND_NOTICES.md`, ADR-0004 | integrity is `IMPLEMENTED_ON_PROTECTED_MAIN`; rights diligence remains separate evidence |

## Data governance traceability

| Governance question | Canonical authority | Representative product evidence |
| --- | --- | --- |
| What may be shared? | DATA_GOVERNANCE, API_CONTRACT | path-free/bounded evidence schemas where implemented |
| What remains private? | DATA_GOVERNANCE, DATA_MODEL | restricted dossiers/receipts/local identifiers where implemented |
| Who owns mutation authority? | ADR-0001/0002, Architecture | Rust authorization/execution boundaries |
| Who owns provider secrets? | DATA_GOVERNANCE, provider contracts | provider/OAuth local records; secrets excluded from shareable evidence |
| What is retention? | DATA_GOVERNANCE | lifecycle-based until a feature defines measured/legal retention |
| Is there a central DB? | DATA_MODEL | `OUT_OF_SCOPE` in current architecture; ERD is conceptual/logical |

## Incident/recovery traceability

| Incident phase | Canonical authority | Acceptance evidence |
| --- | --- | --- |
| Containment | INCIDENT_RUNBOOK, THREAT_MODEL | fail-closed authority and source preservation |
| RCA | INCIDENT_RUNBOOK, UML | exact identity + first failing boundary + falsifiable hypothesis |
| Remedy selection | INCIDENT_RUNBOOK, ADR-0006 | distinct remedies + authority/feasibility/blast-radius/rollback proof |
| Remediation | TEST_STRATEGY, feature tests | realistic RED -> narrow fix -> GREEN or deterministic operational failing probe |
| Closure | INCIDENT_RUNBOOK, OPERABILITY | exact repaired evidence + recurrence search + protected/release operational proof where relevant |

## Documentation fitness traceability

`src/lib/architectureDocumentation.test.ts` is the machine contract for the canonical documentation families, ADR count/index, Mermaid diagrams, conceptual ERD, data-governance/incident/acquisition/licensing authorities, roadmap, release/rollback, maturity vocabulary, and traceability markers. It is repository-relative so IDE/CI working-directory differences do not redefine the documentation root.

The contract protects discoverability and selected invariants only. Semantic review must still compare documentation with current protected source/workflows. ADR-0010 requires a non-documentation work handoff when the audit exposes a safe implementation gap.

## Acquisition/release traceability

| Claim | Minimum evidence | No-substitution rule |
| --- | --- | --- |
| Commercially usable feature | protected-main path + refusal/recovery + realistic tests | PR/docs alone do not prove it |
| 100% owned production coverage | exact-head statement/branch/function/line measurement where exposed | exclusions/stale report do not prove it |
| Secure release | exact integrated source + current security gates + artifact identity | one green scanner does not prove universal security |
| Provenanced release | exact artifact set + SBOM + provenance/attestation + publication proof | source tag alone does not prove artifact identity |
| Redistribution rights | root license + dependency/model/asset rights + NOTICE inventory | root MIT license cannot grant third-party rights by itself |
| Performance/SLO | representative benchmark/operational evidence | architecture target is not measurement |
| Certification | external authoritative assessment/certificate | standards references are not certification |

See `docs/ACQUISITION_DILIGENCE.md`.

## Standards/research traceability

Standards and research references are design/evaluation inputs and never certification claims. Current authoritative details are retained in Architecture, security/test documents, ADRs, and feature doctoring. Draft/final status is recorded at the source that relies on it rather than treated as permanent implementation maturity.

## Update rule

Material changes to requirements, authority, persistence, privacy/data handling, integration schemas, security, deployment, writer/merge governance, stale-branch convergence, incident recovery, licensing/IP, buyer evidence, or release acceptance update this file in the same reviewed change. Dated PR/run/SHA evidence may be referenced in dated assessments/PRs/evidence bundles but is not embedded as timeless architecture.