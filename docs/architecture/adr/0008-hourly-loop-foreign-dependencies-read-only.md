# ADR-0008: Keep the hourly loop read-only at foreign dependency boundaries

**Status:** Accepted
**Date:** 2026-08-20

## Context

The hourly product loop needs contextual-orchestrator model discovery and an
advisory OpenCode review. An earlier design checked out the orchestrator
repository and registered five provider credentials into its KV from this
repository's workflow. That created a cross-repository write boundary and made
the DiskSage workflow responsible for provider-secret custody even though the
orchestrator deployment owns its own runtime configuration.

## Decision

The repository-local advisory workflow uses only contextual-orchestrator's
published HTTP API. It reads `CONTEXTUAL_ORCHESTRATOR_URL` and
`CONTEXTUAL_ORCHESTRATOR_TOKEN`, discovers a model through `/v1/models`, and
sends bounded repository context to `/v1/chat/completions`. It checks out the
exact manually-dispatched `github.sha`, keeps repository and pull-request
permissions read-only, and never checks out or mutates the foreign
orchestrator repository.

The repository-local workflow is intentionally **manual-only**. A direct HTTP
model call is not a pinned OpenCode review worker, so a repository-local
schedule would create an unpinned autonomous reviewer. The hourly product and
PR review loop is owned by the trusted central workflow
[`disksage-hourly-review-repair.yml`](https://github.com/ContextualWisdomLab/.github/blob/main/.github/workflows/disksage-hourly-review-repair.yml)
at `37 * * * *`. That caller dispatches the pinned reusable scheduler at
[`a3fdaa1aacaba9443a18573f3c309fe1841fc2f0`](https://github.com/ContextualWisdomLab/.github/blob/a3fdaa1aacaba9443a18573c309fe1841fc2f0/.github/workflows/pr-review-fix-scheduler.yml),
which performs its own OpenCode OIDC exchange and exact-head lease. This keeps
the hourly requirement live without making DiskSage's local advisory workflow
an unpinned mutation authority.

The five provider credentials (`BYTEZ_API_KEY`, both NVIDIA NIM keys,
`OPENROUTER_API_KEY`, and `OPENAI_API_KEY`) remain deployment-side
configuration of contextual-orchestrator. They are not imported into this
workflow, copied into its KV, passed to the advisory Agent, or printed in
logs. A missing orchestrator URL/token produces a visible skip; there is no
OAuth, Copilot, or local mutation fallback.

## Operational evidence

On 2026-08-21, the latest central scheduled runs (including
[`31991358711`](https://github.com/ContextualWisdomLab/.github/actions/runs/31991358711))
ended in `startup_failure` before creating a job. The called scheduler requests
`id-token: write`, while the caller exposed only `contents: read`; the missing
caller permission prevented the OpenCode OIDC exchange from starting. The
minimal repair is tracked in
[`ContextualWisdomLab/.github#1188`](https://github.com/ContextualWisdomLab/.github/pull/1188)
at current head `3ab34b57a7ab04eb14b5fca7994dd047df676748`; it applies the same OIDC
permission fix to DiskSage and its sibling Clearfolio caller and updates the
contract tests. The earlier DiskSage-only repair remains open as #1180. Until
one of these fixes is normally merged and a scheduled run completes, the
hourly cadence is not claimed as operational evidence.

## Consequences

- The GitHub workflow cannot change a foreign database or repository and does
  not become a second provider-secret store.
- Model discovery and advisory review continue when the orchestrator endpoint
  is configured, while the standalone personal installation remains OAuth-free.
- Provider credentials must be configured where contextual-orchestrator is
  deployed; this repository cannot prove that external deployment state.
- Exact event-SHA context prevents either loop from reviewing a stale `main` tree.
- Source revision `9b1c270` additionally uploads a seven-day, path-free advisory receipt when the
  endpoint is configured. The receipt contains only schema version, event SHA, model identifier,
  status, response byte count, and response hash; the model response body is never persisted.

## Rejected alternatives

- **Checkout contextual-orchestrator in the DiskSage workflow:** rejected
  because it couples the loop to foreign source and dependency installation.
- **Register provider secrets into foreign KV from GitHub Actions:** rejected
  because it expands write authority and secret custody without a product need
  in this repository.
- **Pass provider secrets to the Agent prompt:** rejected because advisory
  review does not need provider credentials and must remain redaction-safe.
- **Restore a schedule to the repository-local HTTP advisory:** rejected because
  it would be an unpinned autonomous reviewer; the central pinned OpenCode
  scheduler already provides the required hourly loop.

## Evidence basis

- Saltzer, J. H., & Schroeder, M. D. (1975). The protection of information in
  computer systems. *Proceedings of the IEEE, 63*(9), 1278–1308.
  https://doi.org/10.1109/PROC.1975.9939
- Joint Task Force. (2020). *Security and privacy controls for information
  systems and organizations* (NIST SP 800-53 Rev. 5, Release 5.2.0, 2025).
  https://doi.org/10.6028/NIST.SP.800-53r5

## Related decisions

- [ADR-0005](0005-hourly-agent-loop-is-advisory.md) — original advisory loop
  contract, superseded by this decision.
- [ADR-0007](0007-pre-copy-evidence-cohort.md) — fail-closed evidence cohort.
