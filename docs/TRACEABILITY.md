# DiskSage Requirements, Decisions, and Evidence Traceability

## Purpose

This map connects product requirement -> architecture decision -> implementation surface -> test/evidence. It prevents chat history, PR prose, or one green status from becoming an undocumented source of truth.

Status vocabulary:

- `protected_main` — representative implementation exists on current protected main.
- `documentation_branch` — canonical documentation/test contract in this clean documentation branch.
- `planned` — intent only; not implementation evidence.

## Product requirement traceability

| Requirement | Decision / architecture | Representative implementation or evidence | Representative test/evidence status |
| --- | --- | --- | --- |
| PRD-FR-001 bounded observation | ARCHITECTURE resource bounds; ADR-0002 | Rust scanners/parsers and feature-specific bounds | `protected_main`; feature tests/doctoring |
| PRD-FR-002 evidence classes | ADR-0002 | planner/evidence/receipt types across Rust modules | `protected_main`; canonical docs `documentation_branch` |
| PRD-FR-003 exact human authorization | ADR-0002 | `src-tauri/src/cloud_transfer.rs`, frontend review projection | `protected_main`; current approval tests |
| PRD-FR-004 mutation-time revalidation | ADR-0002 | cloud transfer/materialization and identity-aware mutation paths | `protected_main` feature families |
| PRD-FR-005 private/shareable evidence | ADR-0001, ADR-0002 | private dossiers/receipts and path-free evidence envelopes | `protected_main` feature tests |
| PRD-FR-006 provider evidence separation | ADR-0001, ADR-0002 | cloud/provider capacity, runtime, queue, sync evidence code | `protected_main` feature tests/doctoring |
| PRD-FR-007 recovery before discard | ADR-0001 | incomplete-download audit/recovery/materialization modules | `protected_main` |
| PRD-FR-008 supply-chain-bound local model | ADR-0004 | `src-tauri/src/llm/model.rs`, `src-tauri/src/llm/installed_model.rs` | `protected_main`; model integrity regressions |
| PRD-FR-009 standalone operation | ADR-0001, ADR-0005 | Tauri/Rust local runtime; optional integrations | `protected_main` architecture |
| PRD-FR-010 modular CWL integration | ADR-0005 | Naruon lineage/readiness schemas; optional orchestrator boundary | mixed `protected_main` feature contracts |
| PRD-FR-011 audit/recovery evidence | ADR-0002 | execution receipts, private dossier/journal evidence | `protected_main` where applicable |
| PRD-FR-012 reproducible release evidence | ADR-0008 | `.github/workflows/test.yml`, `.github/workflows/release.yml`, organization controls | baseline `protected_main`; stronger provenance remains roadmap work until integrated/proven |

## Conversation-to-repository decisions

| Durable decision | Canonical record | Implementation/evidence classification |
| --- | --- | --- |
| Local Rust keeps filesystem mutation authority | PRD, TRD, ARCHITECTURE, ADR-0001 | `protected_main` architecture |
| Observation/recommendation/approval/execution are separate | TRD, DATA_MODEL, ADR-0002, UML | `protected_main` + `documentation_branch` |
| Exact current source head and independently resolved live base are required for repository decisions | TRD, ADR-0003, UML | governance contract; `documentation_branch` canonicalization |
| GitHub review/check/model/status evidence classes remain separate | TRD, ADR-0003 | governance contract |
| One DiskSage writer lease; waiting is local, no report-as-completion | TRD, ADR-0006, UML | scheduler/agent governance + `documentation_branch` |
| No temporary self-modifying repair workflows as steady-state repair mechanism | TRD, AGENTS, ADR-0006 | governance contract |
| Autonomous development uses OpenCode + `NVIDIA_NIM_API_KEY`, never `COPILOT_GITHUB_TOKEN` for model execution | TRD, AGENTS, ADR-0006 | governance contract |
| Database/evidence names use 2+ descriptive `snake_case` words | PRD/TRD/DATA_MODEL/AGENTS | canonical quality rule |
| Documentation completion is intermediate; continue safe work | ADR-0006, DOCUMENTATION_ASSESSMENT | governance contract |
| Release requires exact integrated head, provenance, rollback and artifact verification | PRD, TRD, ADR-0008, RELEASE_AND_ROLLBACK | canonical release contract |

## Model artifact traceability

| Control | Source | Evidence |
| --- | --- | --- |
| Immutable model identity | `src-tauri/src/llm/model.rs` | immutable revision/size/SHA-256 source contract |
| Bounded/race-resistant install | `src-tauri/src/llm/model.rs` | deterministic installer/race regressions and doctoring |
| Load-time verification | `src-tauri/src/llm/installed_model.rs` | missing/link/type/size/digest/identity tests |
| Verified identity retained through llama load | installed-model + engine integration | integrated source and regression/doctoring |
| AI secure-development evidence | ARCHITECTURE / model doctoring | NIST SP 800-218A, OWASP AISVS as design inputs |

## Documentation fitness traceability

`src/lib/architectureDocumentation.test.ts` is the machine contract for the canonical documentation families, ADR count/index, Mermaid diagrams, conceptual ERD, roadmap, release/rollback, and traceability markers. It is intentionally repository-relative so IDE/CI working-directory differences do not redefine the documentation root.

## Standards/research traceability

| Source | Why it matters | Canonical use |
| --- | --- | --- |
| NIST SP 800-218 v1.1 | final secure SDLC baseline | ARCHITECTURE, TEST_STRATEGY, ADRs |
| NIST SP 800-218 Rev. 1 / SSDF 1.2 Initial Public Draft | forward-looking SSDF changes; not final | ARCHITECTURE status note only |
| NIST SP 800-218A | AI/foundation-model producer/acquirer SSDF profile | model artifact/security doctoring |
| ISO/IEC 27001:2022 + Amd 1:2024 | information-security management design input | security/operability context |
| ISO/IEC 27040:2024 | storage security | storage/privacy architecture |
| OWASP ASVS 5.0.0 | application security verification | threat/test strategy |
| OWASP AISVS 1.0 | AI-enabled system verification | model/AI threat and test strategy |
| SLSA 1.2 | source/build/provenance model | release/provenance contract |
| WCAG 2.2 latest Recommendation | accessible digital content/workflows | PRD/testing/accessibility |

## Update rule

Material changes to requirements, authority, persistence, integration schemas, security, deployment, writer/merge governance, or release acceptance update this file in the same reviewed change. Dated PR/run/SHA evidence may be referenced in a dated assessment or PR body but is not embedded as timeless architecture.