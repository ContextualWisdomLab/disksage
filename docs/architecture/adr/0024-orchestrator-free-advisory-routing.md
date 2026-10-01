# ADR-0024: Delegate local advisory model selection to contextual-orchestrator

**Status:** Proposed
**Date:** 2026-09-12

## Context

ADR-0008 established the durable foreign-dependency boundary for DiskSage's repository-local advisory workflow: the workflow may consume contextual-orchestrator through its published HTTP API, but it must not check out or mutate the foreign repository, import provider credentials, or become a second provider-secret store. At the time, the accepted implementation discovered a model through `/v1/models` and then selected a returned model locally.

The current contextual-orchestrator contract assigns provider, model, group, and fallback selection to the gateway. A DiskSage caller that discovers model inventory and chooses an entry locally would duplicate routing authority and could diverge from gateway policy. The repository-local advisory also has no product need to impose a total elapsed-time cutoff on model reasoning, streaming, or tool execution; it only needs a bounded connection establishment and bounded response payload. The local workflow remains manual-only. Scheduled review cadence remains owned by the central pinned workflow described by ADR-0008.

This proposal changes routing mechanics only. It does not revise ADR-0008's accepted read-only foreign-dependency boundary or grant DiskSage model-provider, repository-write, cloud-write, deletion, or eviction authority.

## Decision

If this proposal is accepted, `.github/workflows/hourly-product-loop.yml` will remain a thin read-only client of contextual-orchestrator and will request the literal virtual route `orchestrator/free` through the released chat-completions API.

The repository-local caller:

- must not call `/v1/models` or choose a provider, concrete model, provider group, or paid fallback;
- sends only `CONTEXTUAL_ORCHESTRATOR_URL` and `CONTEXTUAL_ORCHESTRATOR_TOKEN` to the gateway boundary and never imports provider credentials;
- keeps GitHub `contents` and `pull-requests` permissions read-only and checks out the exact event SHA with persisted checkout credentials disabled;
- retains a 30-second connection-establishment timeout and a 65,536-byte response bound, but does not impose a caller-owned total elapsed-time cutoff on a successful model exchange;
- validates the bounded response before producing a path-free receipt and never persists the model response body;
- fails closed when gateway configuration is missing, the request fails, the response exceeds the bound, or the response contract is malformed;
- does not introduce a direct-provider, locally selected model, Copilot, OAuth, or paid fallback when `orchestrator/free` is unavailable;
- remains manual-only. The central pinned scheduler remains the owner of recurring review cadence and exact-head writer leases.

If contextual-orchestrator cannot satisfy a required capability through its released route, the repair belongs to the contextual-orchestrator owner. DiskSage does not work around that deficiency by copying routing policy or provider configuration into this repository.

## Consequences

- Provider/model routing policy has one authority: contextual-orchestrator.
- DiskSage no longer needs model-inventory discovery merely to run its local advisory.
- The local workflow can wait for gateway/model completion without confusing a caller-selected elapsed-time limit with provider completion, while connection establishment and response size remain bounded.
- A gateway outage or unavailable `orchestrator/free` capability blocks the advisory rather than silently switching to a direct or paid provider.
- Existing foreign-repository, filesystem, cloud, deletion, and recovery authorities do not change.
- Because this record is Proposed, ADR-0008 remains the accepted historical decision until this proposal is normally accepted or superseded through the ADR lifecycle.

## Rejected alternatives

- **Continue `/v1/models` discovery and choose the first or preferred model locally:** rejected because it duplicates gateway routing authority and makes repository behavior depend on model-list ordering.
- **Hard-code a concrete provider/model or provider group:** rejected because the provider/model policy belongs to contextual-orchestrator and would create mutable cross-owner coupling.
- **Allow a direct or paid fallback from DiskSage:** rejected because it bypasses gateway governance and provider-cost policy.
- **Keep a fixed total elapsed-time cutoff such as `--max-time 120`:** rejected because elapsed wall time alone is not a valid completion signal for reasoning, streaming, or tool execution. The caller still bounds connection establishment and response bytes.
- **Restore a repository-local schedule:** rejected because ADR-0008 assigns recurring cadence to the central pinned scheduler; this proposal changes route selection, not scheduler authority.

## Evidence and traceability

- `.github/workflows/hourly-product-loop.yml` — current thin caller and bounded receipt behavior.
- `src/lib/hourlyProductLoopContract.test.ts` — executable caller/ADR contract.
- [ADR-0008](0008-hourly-loop-foreign-dependencies-read-only.md) — accepted read-only foreign-dependency and central-cadence boundary retained by this proposal.
- Saltzer, J. H., & Schroeder, M. D. (1975). The protection of information in computer systems. *Proceedings of the IEEE, 63*(9), 1278–1308. https://doi.org/10.1109/PROC.1975.9939
- Joint Task Force. (2020). *Security and privacy controls for information systems and organizations* (NIST SP 800-53 Rev. 5). National Institute of Standards and Technology. https://doi.org/10.6028/NIST.SP.800-53r5

## Follow-up

Before this decision can become Accepted, the unchanged exact PR head must prove the workflow contract, preserve ADR-0008 history, pass applicable deterministic/security/governance gates, and retain fail-closed behavior for missing gateway capability. Any later change to provider/model ownership, local scheduling, or timeout semantics requires a new ADR rather than rewriting this record after acceptance.
