# DiskSage Interoperability and Modular MSA Contract

## Status

This document describes the intended cross-cutting contract on the active canonical documentation branch. Protected `main` remains the shipped source of truth until this branch integrates. Individual integration capabilities must retain their own maturity state in `TRACEABILITY.md`.

## Prime directive

**DiskSage must remain independently useful without ContextualWisdomLab infrastructure, while optional CWL composition must be explicit, versioned, least-privilege, and failure-isolated.** Integration may add evidence, orchestration, or host composition; it must not silently move local mutation authority or create hidden database/runtime coupling.

## Ownership boundaries

| Boundary | DiskSage owns | Optional peer/host owns | Forbidden coupling |
| --- | --- | --- | --- |
| Desktop/runtime | local scan/evidence, recommendation projection, explicit local approval gates, local filesystem/provider mutation implemented by DiskSage | host launch/composition only | peer service becoming implicit authority for local mutation |
| Central `.github` | repository-local caller contracts and exact-head evidence consumption | reusable CI/review/security control-plane implementation under its own writer/governance boundary | copying central internals into product code or leaf workarounds that weaken a central gate |
| Naruon | stable DiskSage-facing evidence/adapter contracts when implemented | host/application composition, user/tenant/session/business workflow authority | direct cross-service application-database access; treating host identity as a local filesystem credential |
| contextual-orchestrator | deterministic validation around any optional model-backed proposal and local acceptance boundary | provider routing/orchestration/model execution | model output becoming mutation, security, merge, or release authority |
| Other CWL services | documented versioned adapter/input/output contract | their own persistence, credentials, tenancy, release lifecycle | undocumented shared tables, ambient credentials, or import-time service dependency |

## Versioned contract rules

1. Every exported cross-repository payload has a version or stable schema identifier before it is treated as durable interoperability authority.
2. Unknown breaking versions fail closed. Optional unknown fields may be ignored only when the contract explicitly declares forward-compatible extension behavior.
3. Evidence payloads preserve provenance/fingerprint semantics needed to detect stale or mismatched observations.
4. A local adapter translates between contracts; it does not reinterpret a remote success into authorization that DiskSage itself would reject.
5. No peer may require DiskSage to expose raw paths, filenames, provider credentials, local account secrets, document contents, or model prompts merely for ordinary composition.
6. Cross-process/repository errors cross the boundary as bounded stable codes plus privacy-safe metadata; raw provider/process exception text is not a public interoperability contract.

## Degraded and disconnected operation

| Dependency state | Required behavior |
| --- | --- |
| No CWL services configured | standalone supported DiskSage workflows remain available |
| Optional service unavailable | only that integration degrades; local evidence and supported local operations remain usable when their own prerequisites are satisfied |
| Schema/version mismatch | integration is refused with a stable compatibility error; no best-effort mutation |
| Model/orchestrator unavailable | deterministic product behavior remains authoritative; model-enhanced proposal is unavailable rather than fabricated |
| Central CI/reviewer unavailable | repository merge/release stays fail-closed; this does not affect installed desktop runtime authority |
| Host tenancy/authorization unavailable | organization-sensitive host integration is unavailable; local personal mode must not infer tenant authority |

## Compatibility evidence

Before claiming an integration supported, require as applicable:

- standalone smoke tests with the integration absent;
- contract/schema tests for accepted and rejected versions;
- representative adapter round trips without hidden persistence;
- privacy tests proving forbidden fields do not cross the boundary;
- dependency-outage and timeout tests;
- stale/fingerprint mismatch refusal;
- no-clobber/idempotency/concurrency behavior when the adapter can trigger work;
- one release compatibility matrix identifying protected-main/released versions actually tested.

An active peer-repository PR is not evidence that a released DiskSage build supports the capability.

## Architecture-description discipline

`ARCHITECTURE.md` identifies system and trust boundaries; this file defines interoperability constraints across them. Per ISO/IEC/IEEE 42010:2022, architecture descriptions and their viewpoints must remain traceable to the concerns they address. For DiskSage that means at minimum the standalone runtime, optional host/service composition, repository control plane, trust boundaries, and degraded/failure modes must not be collapsed into one ambiguous deployment view.

## References (APA 7th)

International Organization for Standardization, International Electrotechnical Commission, & Institute of Electrical and Electronics Engineers. (2022). *ISO/IEC/IEEE 42010:2022 Software, systems and enterprise—Architecture description* (2nd ed.). https://www.iso.org/standard/74393.html
