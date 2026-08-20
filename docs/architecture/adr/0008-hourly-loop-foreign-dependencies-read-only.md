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

The workflow uses only contextual-orchestrator's published HTTP API. It reads
`CONTEXTUAL_ORCHESTRATOR_URL` and `CONTEXTUAL_ORCHESTRATOR_TOKEN`, discovers a
model through `/v1/models`, and sends bounded repository context to
`/v1/chat/completions`. It checks out the exact scheduled or manually
dispatched `github.sha`, keeps repository and pull-request permissions
read-only, and never checks out or mutates the foreign orchestrator repository.

The five provider credentials (`BYTEZ_API_KEY`, both NVIDIA NIM keys,
`OPENROUTER_API_KEY`, and `OPENAI_API_KEY`) remain deployment-side
configuration of contextual-orchestrator. They are not imported into this
workflow, copied into its KV, passed to the advisory Agent, or printed in
logs. A missing orchestrator URL/token produces a visible skip; there is no
OAuth, Copilot, or local mutation fallback.

## Consequences

- The GitHub workflow cannot change a foreign database or repository and does
  not become a second provider-secret store.
- Model discovery and advisory review continue when the orchestrator endpoint
  is configured, while the standalone personal installation remains OAuth-free.
- Provider credentials must be configured where contextual-orchestrator is
  deployed; this repository cannot prove that external deployment state.
- Exact event-SHA context prevents the loop from reviewing a stale `main` tree.

## Rejected alternatives

- **Checkout contextual-orchestrator in the DiskSage workflow:** rejected
  because it couples the loop to foreign source and dependency installation.
- **Register provider secrets into foreign KV from GitHub Actions:** rejected
  because it expands write authority and secret custody without a product need
  in this repository.
- **Pass provider secrets to the Agent prompt:** rejected because advisory
  review does not need provider credentials and must remain redaction-safe.

## Evidence basis

- Saltzer, J. H., & Schroeder, M. D. (1975). The protection of information in
  computer systems. *Proceedings of the IEEE, 63*(9), 1278–1308.
  https://doi.org/10.1109/PROC.1975.9939
- Joint Task Force. (2020). *Security and privacy controls for information
  systems and organizations* (NIST SP 800-53 Rev. 5, Release 5.2.0, 2025).
  https://doi.org/10.6028/NIST.SP.800-53r5

## Related decisions

- [ADR-0005](0005-hourly-agent-loop-is-advisory.md) — original advisory loop
  contract.
- [ADR-0007](0007-pre-copy-evidence-cohort.md) — fail-closed evidence cohort.
