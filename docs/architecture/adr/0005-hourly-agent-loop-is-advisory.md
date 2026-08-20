# ADR-0005: Keep the hourly agent loop advisory and secret-gated

**Status:** Accepted  
**Date:** 2026-08-20

## Context

DiskSage needs a recurring product-development loop that observes the current
protected PR queue, the product/technical gap baseline, and current main. The
loop must use the ContextualWisdomLab contextual-orchestrator OpenCode Agent
when it is configured, but a personal installation must not require OAuth,
Copilot credentials, or an external model merely to run DiskSage locally.

## Decision

`.github/workflows/hourly-product-loop.yml` runs hourly and on manual dispatch.
It reads only two repository secrets, `CONTEXTUAL_ORCHESTRATOR_URL` and
`CONTEXTUAL_ORCHESTRATOR_TOKEN`, discovers an available inference model from
`/v1/models`, and sends bounded repository context to
`/v1/chat/completions`. The prompt explicitly makes the OpenCode Agent
advisory: it may identify a blocker and the next evidence step, but it cannot
authorize cloud copy, source eviction, cache deletion, GitHub bypass, or
secret disclosure.

When either secret is absent, the job records a skipped configuration message
and uses no fallback model or Copilot token. The workflow has read-only
repository and pull-request permissions, does not commit generated advice,
and leaves runtime Goal/ADR projections and all mutation gates under the
local Rust evidence contracts.

## Consequences

- The recurring loop can be enabled for an organization without changing the
  standalone personal workflow.
- Model discovery and agent output remain external advisory evidence; a model
  response never becomes transfer or deletion authority.
- Missing orchestrator configuration is visible rather than silently falling
  back to an unapproved provider.
- A future visual redesign must add its Figma File ID to a separate ADR; this
  decision introduces no Figma artifact.

## Evidence

- `.github/workflows/hourly-product-loop.yml`
- `docs/product-technical-gap-baseline.md`
- `docs/architecture/adr/0001-cloud-offload-goal-state.md`
