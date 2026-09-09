# DiskSage Quality Attributes and Acceptance Evidence

## Status and purpose

This document is part of the canonical acquisition-documentation graph on the active documentation branch. Until that branch is integrated, it is `IMPLEMENTED_ON_ACTIVE_PR`, not protected-main product truth.

DiskSage uses ISO/IEC 25010:2023 as the cross-cutting product-quality reference model. The standard is useful for requirements, design objectives, testing objectives, quality-control criteria, and acceptance criteria; this document turns that general model into DiskSage-specific evidence expectations. It does **not** claim ISO certification or conformance.

## Quality-attribute scenarios

| Attribute | DiskSage scenario | Required evidence before a release claim |
| --- | --- | --- |
| Functional correctness | A scan, recommendation, approval, copy/adoption, reclaim-evidence, or model-integrity result matches the documented versioned contract and fails closed on malformed or stale evidence. | deterministic unit/integration tests at the public boundary plus representative fixtures |
| Safety and data-loss resistance | Observation or recommendation cannot silently become deletion, eviction, provider mutation, or other destructive authority. | explicit approval/fingerprint/freshness tests, no-clobber tests, rollback/recovery evidence, threat-model review |
| Security | Untrusted paths, archives, provider evidence, model artifacts, webview content, workflow inputs, and repository evidence cannot bypass their trust boundary. | security regressions, SAST/security workflows, dependency/SBOM evidence, current threat model, least-privilege review |
| Reliability | Interrupted, stale, partial, duplicate, or unavailable evidence is surfaced as incomplete/unavailable rather than success. | deterministic failure/retry/idempotency tests, crash/restart or recovery evidence where state can outlive a process |
| Performance efficiency | Large filesystems, archives, provider inventories, and model artifacts remain bounded in memory, time, result count, and diagnostic size. | representative benchmark profile with explicit fixture/hardware/context; no unmeasured latency or throughput guarantee |
| Compatibility/interoperability | Standalone DiskSage works without CWL services; optional CWL integrations exchange only versioned contracts and degrade independently. | standalone tests, adapter/contract tests, unsupported-version refusal, dependency-outage tests |
| Interaction/accessibility | The desktop UI exposes state, errors, approval boundaries, and progress without requiring pointer-only or visually inferred interaction. | applicable WCAG 2.2 acceptance evidence defined in `ACCESSIBILITY_ACCEPTANCE.md` |
| Maintainability/testability | Public behavior, security boundaries, docs, and state names can be understood and changed without hidden coupling. | beginner-readable public docs/rustdoc, canonical docs tests, exact owned-production coverage evidence, bounded modules/contracts |
| Portability/deployability | Supported desktop packages and operational CLIs are produced from one exact source revision with reproducible identity and provenance. | platform build/package tests, exact release admission, SBOM/provenance, install/launch smoke evidence |

## Evidence rules

1. **No metric without context.** A timing, memory, recovery, availability, or error-rate number must record fixture/workload, hardware/OS, build profile, measurement method, sample count, and source revision.
2. **No target promoted by prose.** A planned threshold remains `PLANNED` until a deterministic gate or reviewed release-acceptance procedure enforces it.
3. **No synthetic success.** Mocks may test local contracts but cannot replace representative filesystem/provider/package/recovery evidence for a buyer-facing claim.
4. **No quality collapse.** A single green CI status cannot stand in for correctness, security, accessibility, reliability, provenance, or review authority.
5. **Exact evidence identity.** Release evidence binds to the unchanged source revision and, where relevant, artifact digest and live protected-base/repository state.
6. **Privacy-safe diagnostics.** Quality evidence must not require collecting raw user paths, filenames, provider credentials, account identifiers, document contents, model prompts, or other unnecessary private payloads.

## Release interpretation

A release may cite this document only when each applicable quality attribute has current evidence in `TRACEABILITY.md` or the exact release evidence bundle. Missing evidence is a release gap, not permission to infer success. `QUALITY_ATTRIBUTES.md` complements `PRD.md`, `TRD.md`, `TEST_STRATEGY.md`, `THREAT_MODEL.md`, `OPERABILITY.md`, `ACCESSIBILITY_ACCEPTANCE.md`, and `RELEASE_AND_ROLLBACK.md` rather than overriding them.

## References (APA 7th)

International Organization for Standardization. (2023). *ISO/IEC 25010:2023 Systems and software engineering—Systems and software Quality Requirements and Evaluation (SQuaRE)—Product quality model* (2nd ed.). https://www.iso.org/standard/78176.html

International Organization for Standardization, International Electrotechnical Commission, & Institute of Electrical and Electronics Engineers. (2022). *ISO/IEC/IEEE 42010:2022 Software, systems and enterprise—Architecture description* (2nd ed.). https://www.iso.org/standard/74393.html
