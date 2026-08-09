# DiskSage Requirements and Evidence Traceability

## Purpose

This document prevents product, security, research, and acquisition claims from becoming detached from implementation evidence. It maps canonical requirements and decisions to code areas, tests/workflows, active pull requests, and authoritative references. It intentionally does not embed unstable commit SHAs as timeless design facts.

## Evidence status vocabulary

- `protected_main` — behavior or control is represented by the current protected default branch.
- `active_pr` — implementation/documentation is proposed in an open pull request and is not shipped authority.
- `planned` — accepted direction or gap without integrated implementation evidence.
- `external_control` — evidence is owned by a provider/platform/organization control plane and must be re-fetched live.

## Product and architecture traceability

| Requirement / decision | Canonical source | Implementation / evidence | Status |
| --- | --- | --- | --- |
| Local-first Rust-owned filesystem authority | PRD, TRD, ADR-0001, Architecture | Tauri/Rust command and safety modules; standalone product structure | `protected_main`, docs strengthened in #137 |
| Observation is not authorization | PRD-FR-002, ADR-0002 | cloud/recovery/worktree evidence modules; Rust approval gates | `protected_main` |
| Exact human approval, rationale, phrase and freshness for mutation | PRD-FR-003, Architecture | `src-tauri/src/cloud_transfer.rs`, frontend cloud review contract/tests | `protected_main`, strengthened in #137 |
| Current-state revalidation and stale-plan refusal | PRD-FR-004, TRD | cloud transfer, recovery/materialization, worktree and safety tests | `protected_main` |
| Private versus path-free shareable evidence | PRD-FR-005, Data Model, Threat Model | Naruon export/evidence modules, private dossier/receipt workflows | `protected_main` |
| Cloud capacity/sync/runtime/eviction separation | PRD-FR-006 | provider capacity/runtime/sync modules and cloud transfer tests | `protected_main` |
| Recovery evidence before discard/materialization | PRD-FR-007 | incomplete download audit/recovery/materialization modules | `protected_main` |
| Optional local model remains advisory | PRD-FR-008, ADR-0001 | llama/local reasoning boundary | `protected_main` |
| Model install integrity | ADR-0004, TRD, Threat Model | PR #141 | `active_pr` |
| Model load-time integrity | ADR-0004, TRD, Threat Model | PR #142 stacked on #141 | `active_pr` |
| Modular Naruon / contextual-orchestrator integration | PRD-FR-009, ADR-0005 | path-free contracts; optional orchestration boundary | `protected_main` architecture, docs in #137 |
| Exact-head/live-base repository authorization | ADR-0003, Architecture, TRD | GitHub checks/reviews/rulesets and maintenance automation | `external_control`; policy docs #137 |
| Release artifact provenance | PRD-FR-010, TRD | PR #138 | `active_pr` |
| Fail-closed Tauri CSP | Threat Model | PR #139 | `active_pr` |
| Buyer-visible Cargo metadata / registry publication boundary | TRD | PR #140 | `active_pr` |
| Podman privacy-safe evidence surface | PRD capability matrix | PR #133 | `active_pr` |
| Canonical acquisition documentation graph | Documentation Assessment | PR #137 and `src/lib/architectureDocumentation.test.ts` | `active_pr` |

## Repository evidence traceability

| Evidence class | Source of truth | Never substitute with |
| --- | --- | --- |
| Current PR source identity | GitHub exact head ref | PR body, prior message, merge SHA from an older state |
| Current base identity | independently resolved live base branch tip | PR's historical recorded base snapshot alone |
| Required checks | current GitHub check/workflow evidence and ruleset policy | commit status with similar name, older green run |
| Security findings | current scanner/Advanced Security result and threads | absence of comments, stale scanner run |
| Formal review | eligible GitHub review on current unchanged head when required | comment, reaction, check status, author/self approval, model prose |
| Automated review finding | exact reviewer output bound to current/relevant diff | rate-limit message as a source finding |
| Merge authority | branch/ruleset/repository policy + all required evidence | `mergeable=true` alone |
| Release authority | exact integrated protected head + release acceptance | a successful PR workflow or predecessor package |

## Open pull-request map

The following list is an architectural aid, not a permanent status snapshot. Live GitHub state must be re-fetched before action.

- #133 — privacy-safe Podman reclaim evidence.
- #137 — acquisition architecture and canonical documentation graph.
- #138 — release provenance/attestation, stacked on #137.
- #139 — fail-closed Tauri CSP.
- #140 — Cargo package metadata hardening.
- #141 — model download/installation integrity.
- #142 — model load-time integrity, stacked on #141.

If these PRs close, merge, split, or are superseded, update this mapping in the integration that changes the architectural status.

## Test evidence map

| Concern | Representative evidence path |
| --- | --- |
| Canonical architecture/doc graph | `src/lib/architectureDocumentation.test.ts` |
| Frontend production coverage | `vitest.config.ts`, package `coverage` script, `.github/workflows/test.yml` |
| Cloud approval / tenant authority | cloud review frontend tests; `src-tauri/src/cloud_transfer.rs` tests and integration test |
| Provider capacity/runtime/sync | Rust provider-specific module tests |
| Filesystem mutation / rollback | Rust `safety` and workflow-specific tests |
| Incomplete-download recovery | Rust audit/recovery/materialization tests |
| Model install/load integrity | active PR #141/#142 deterministic Rust tests |
| Release package/provenance | `.github/workflows/release.yml`; active PR #138 for stronger attestation |
| Webview CSP | active PR #139 policy regression tests |
| Cargo metadata | active PR #140 semantic Cargo metadata tests |

The repository must re-fetch and inspect actual current files/checks rather than rely on this table if paths change.

## Standards and research mapping

| Source | Why it is used | Product mapping |
| --- | --- | --- |
| NIST SP 800-218 SSDF 1.1 | secure development evidence and release discipline | exact source/review/test/release evidence |
| NIST SP 800-218A | AI/model producer/acquirer secure-development profile | local model supply-chain and acquisition evidence |
| NIST SP 800-53 Rev. 5 / Release 5.2.0 material referenced in doctoring | security/privacy control vocabulary | tenant authority, audit/evidence controls |
| ISO/IEC 27001:2022 + Amd 1:2024 | information-security risk-management design input | governance and security management vocabulary |
| ISO/IEC 27040:2024 | storage-security design input | local/cloud storage lifecycle and evidence boundaries |
| OWASP ASVS 5.0.0 | application security verification input | input validation, authentication/authorization and webview/service controls |
| OWASP Top 10:2025 A03/A08 | supply-chain/integrity design input | model artifact integrity and package/action supply chain |
| SLSA 1.2 | source/build/provenance vocabulary | release artifact identity and provenance |
| WCAG 2.2 / ISO/IEC 40500:2025 | accessibility verification input | affected UI states and release acceptance |
| Cargo/Rust primary documentation | package metadata semantics | PR #140 |
| Tauri/W3C CSP primary documentation | webview CSP semantics | PR #139 |
| Hugging Face/Qwen/Apache/ureq primary sources | exact model/download/license identity | PR #141/#142 doctoring |

References are maintained in the relevant `ARCHITECTURE.md` and `docs/doctoring/` records in APA 7th style. A citation is not a blanket conformance/certification claim.

## Documentation traceability

| Question | Authoritative document |
| --- | --- |
| What product/problem/outcomes are we building? | `docs/PRD.md` |
| What technical constraints and evidence semantics apply? | `docs/TRD.md` |
| Where are trust/deployment/authority boundaries? | `ARCHITECTURE.md` |
| Why were material architecture decisions made? | `docs/adr/` |
| How do components and state transitions interact? | `docs/UML.md` |
| What are the conceptual/persisted data entities? | `docs/DATA_MODEL.md` |
| What can attack the product and how is it mitigated? | `docs/THREAT_MODEL.md` and `SECURITY.md` |
| How is correctness proven? | `docs/TEST_STRATEGY.md` |
| How is the product operated/recovered/released? | `docs/OPERABILITY.md` |
| Are the documentation families complete and current? | `docs/DOCUMENTATION_ASSESSMENT.md` |
| What changed in an integrated/released version? | `CHANGELOG.md` |

## Change-control rule

A material requirement, authority boundary, persistence contract, model/provider integration, release policy, or standards claim must update the relevant source documents and this traceability map in the same pull request. Active proposals remain marked `active_pr` or `planned`; documentation must not silently promote them to `protected_main`.