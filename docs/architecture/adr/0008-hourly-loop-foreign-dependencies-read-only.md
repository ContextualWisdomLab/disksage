# ADR-0008: Keep the hourly loop read-only at foreign dependency boundaries

**Status:** Accepted; amended 2026-09-12  
**Date:** 2026-08-20

## Context

DiskSage needs bounded model-backed advisory review without turning a product repository into a
second provider-routing control plane. An earlier design checked out contextual-orchestrator and
registered provider credentials from this repository; the subsequent shipped design removed that
foreign write boundary but still called `/v1/models`, selected the first returned model, and imposed
a caller-owned total inference timeout. Those behaviors are now stale: contextual-orchestrator owns
provider/account/model discovery and failover, while GitHub Actions callers use the admitted
fail-closed zero-cost alias `orchestrator/free`.

The repository-local advisory is not DiskSage's hourly scheduler. It is a manual, read-only diagnostic
caller. Organization-level cadence and repair orchestration are owned by
`ContextualWisdomLab/.github`.

## Decision

The repository-local `.github/workflows/hourly-product-loop.yml` remains `workflow_dispatch`-only
and uses only contextual-orchestrator's published HTTP API. It reads
`CONTEXTUAL_ORCHESTRATOR_URL` and `CONTEXTUAL_ORCHESTRATOR_TOKEN`, sends literal model alias
`orchestrator/free` to `/v1/chat/completions`, and never calls `/v1/models` or supplies provider,
provider-group, candidate-model, or paid-fallback configuration.

Provider/account/model discovery, route admission, ZDR/free-pool policy, and failover remain
contextual-orchestrator responsibilities. DiskSage therefore does not infer a concrete provider model
from the gateway catalog and does not copy control-plane discovery logic into this repository.

The inference request has no caller-owned elapsed-time model cutoff. The workflow retains a
30-second connection-establishment timeout and a 65,536-byte response-size bound; the GitHub job
lifecycle remains a separate administrative execution boundary. Missing gateway URL/token fails
closed and there is no local/provider/Copilot fallback.

The workflow checks out exact `${{ github.sha }}` with persisted Git credentials disabled, limits
repository permissions to read-only contents and pull-request access, collects bounded PR/baseline
context, and persists only a seven-day path-free receipt. The receipt records the requested route
alias (`orchestrator/free`), event SHA, status, response byte count, and response SHA-256; it does not
persist the model response body, provider credentials, or a discovered concrete provider model.

Organization-level hourly product cadence is owned by the consolidated
[`ContextualWisdomLab/.github/.github/workflows/hourly-review-repair.yml`](https://github.com/ContextualWisdomLab/.github/blob/cb0872c9a20d5584703dffacca65c096fc034c6c/.github/workflows/hourly-review-repair.yml),
which replaced the former per-repository `*-hourly-review-repair.yml` callers. DiskSage does not copy
that scheduler or its provider-routing implementation into its own workflow.

## Current authority and evidence

At `ContextualWisdomLab/.github@cb0872c9a20d5584703dffacca65c096fc034c6c`, the central
architecture documents `hourly-review-repair.yml` as the single thin caller for the product
repositories, and the central agent policy routes OpenCode, Noema, and Strix through the fail-closed
`orchestrator/free` pool. Those sources establish ownership and routing policy; they are not, by
themselves, evidence that any particular future scheduled run completed.

DiskSage's local contract test therefore checks the repository-owned facts directly: no
`/v1/models`, no provider/model candidate configuration, literal `orchestrator/free`, no caller total
`--max-time 120`, distinct connection timeout, exact-SHA checkout, read-only permissions, bounded
receipt, and no foreign repository or secret bootstrap.

## Consequences

- DiskSage cannot mutate a foreign repository/database or become a second provider-secret store.
- A configured gateway chooses the admitted provider/model behind `orchestrator/free`; DiskSage sees
  only the stable route contract.
- Provider credentials remain deployment-side contextual-orchestrator configuration and never enter
  the advisory request or logs.
- Missing gateway configuration is a visible failure rather than a silent provider/local fallback.
- Exact event-SHA context prevents manual advisory review of a stale tree.
- Connection establishment, response-size control, provider completion, user cancellation, and
  administrative job termination remain distinct lifecycle concerns instead of one elapsed-time
  inference timeout.
- The central `.github` owner may change cadence or scheduler internals without requiring DiskSage to
  copy those implementation details.

## Rejected alternatives

- **Discover a model through `/v1/models` in DiskSage:** rejected because provider/model discovery
  belongs to contextual-orchestrator and first-entry selection bypasses route admission policy.
- **Specify provider, provider group, candidate model list, or paid fallback:** rejected because it
  duplicates control-plane policy and can bypass the fail-closed free pool.
- **Apply a fixed total inference timeout in the caller:** rejected because elapsed time alone cannot
  distinguish provider completion, reasoning/stream/tool-call progress, user cancellation, and an
  administrative execution limit.
- **Checkout contextual-orchestrator in the DiskSage workflow:** rejected because it couples this
  repository to foreign source and dependency installation.
- **Register provider secrets into foreign KV from GitHub Actions:** rejected because it expands
  write authority and secret custody without a DiskSage product need.
- **Restore a schedule to the repository-local advisory:** rejected because organization-level
  heartbeat/repair cadence is already owned centrally; a second scheduler would create competing
  automation authority.

## Evidence basis

- Saltzer, J. H., & Schroeder, M. D. (1975). The protection of information in computer systems.
  *Proceedings of the IEEE, 63*(9), 1278–1308. https://doi.org/10.1109/PROC.1975.9939
- Joint Task Force. (2020). *Security and privacy controls for information systems and organizations*
  (NIST SP 800-53 Rev. 5). National Institute of Standards and Technology.
  https://doi.org/10.6028/NIST.SP.800-53r5

## Related decisions

- [ADR-0005](0005-hourly-agent-loop-is-advisory.md) — original advisory-loop contract, superseded by
  this decision.
- [ADR-0007](0007-pre-copy-evidence-cohort.md) — fail-closed evidence cohort.
