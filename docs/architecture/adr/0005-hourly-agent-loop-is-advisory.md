# ADR-0005: Keep the hourly agent loop advisory and secret-gated

**Status:** Superseded by [ADR-0008](0008-hourly-loop-foreign-dependencies-read-only.md)
**Date:** 2026-08-20

> This record preserves the original bootstrap design for historical traceability. The shipped
> workflow follows ADR-0008 and does not perform the credential/KV bootstrap described below.

## Context

DiskSage needs a recurring product-development loop that observes the current
protected PR queue, the product/technical gap baseline, and current main. The
loop must use the ContextualWisdomLab contextual-orchestrator OpenCode Agent
when it is configured, but a personal installation must not require OAuth,
Copilot credentials, or an external model merely to run DiskSage locally.

## Decision

`.github/workflows/hourly-product-loop.yml` runs hourly and on manual dispatch.
Its separate bootstrap job reads `CONTEXTUAL_ORCHESTRATOR_KV_DSN`,
`CONTEXTUAL_ORCHESTRATOR_KV_PASSPHRASE`, and the five provider secrets
(`BYTEZ_API_KEY`, `NVIDIA_NIM_API_KEY`, `NVIDIA_NIM_API_KEY_SUB`,
`OPENROUTER_API_KEY`, and `OPENAI_API_KEY`) only when all are configured. It
pipes each provider key over stdin to contextual-orchestrator's pinned
`register-credential` CLI, so the running Agent resolves keys from the
encrypted KV rather than its environment. The Agent job then reads
`CONTEXTUAL_ORCHESTRATOR_URL` and `CONTEXTUAL_ORCHESTRATOR_TOKEN`, discovers
an available inference model from `/v1/models`, and sends bounded repository
context to `/v1/chat/completions`. The prompt explicitly makes the OpenCode
Agent advisory: it may identify a blocker and the next evidence step, but it
cannot authorize cloud copy, source eviction, cache deletion, GitHub bypass,
or secret disclosure.

When either secret is absent, the job records a skipped configuration message
and uses no fallback model or Copilot token. The workflow has read-only
repository and pull-request permissions, does not commit generated advice,
and leaves runtime Goal/ADR projections and all mutation gates under the
local Rust evidence contracts.

## Consequences

- The recurring loop can be enabled for an organization without changing the
  standalone personal workflow; missing KV bootstrap secrets produce a visible
  skip rather than a partial provider configuration.
- Model discovery and agent output remain external advisory evidence; a model
  response never becomes transfer or deletion authority.
- Missing orchestrator configuration is visible rather than silently falling
  back to an unapproved provider.
- A future visual redesign must add its Figma File ID to a separate ADR; this
  decision introduces no Figma artifact.

## Evidence

- `.github/workflows/hourly-product-loop.yml`
- `src/lib/hourlyProductLoopContract.test.ts`
- `docs/product-technical-gap-baseline.md`
- `docs/architecture/adr/0001-cloud-offload-goal-state.md`
