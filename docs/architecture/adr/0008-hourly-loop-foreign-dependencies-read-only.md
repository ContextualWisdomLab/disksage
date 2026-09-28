# ADR-0008: Delegate product-loop review and model routing to the central owner

**Status:** Accepted, amended 2026-09-21
**Date:** 2026-08-20

## Context

DiskSage needs recurring review and repair of its protected PR queue, but it must not become a
second owner of contextual-orchestrator provider discovery, model selection, GitHub App/OIDC
review authority, or provider-secret custody. The earlier repository-local workflow called
`/v1/models` and `/v1/chat/completions` directly. Even though that caller was read-only, it still
implemented model-routing policy inside the product repository and duplicated a control-plane
responsibility that now belongs to `ContextualWisdomLab/.github` and
`contextual-orchestrator`.

The organization control plane has since consolidated the former per-product hourly callers into
`.github/workflows/hourly-review-repair.yml`. Native PR/review events own normal progress; the
central schedule is a distributed missed-event recovery. At protected `.github` revision
`e6334e229581a918e2f22de18733b76fa65d7e71`, DiskSage is the `37 10 * * *` recovery target and
the reusable engine is `.github/workflows/pr-review-fix-scheduler.yml`.

## Decision

The repository-local `.github/workflows/hourly-product-loop.yml` remains manual-only, but it is
now only a thin admission point. It calls the central reusable scheduler at the exact protected
owner revision:

`ContextualWisdomLab/.github/.github/workflows/pr-review-fix-scheduler.yml@e6334e229581a918e2f22de18733b76fa65d7e71`

The caller supplies only DiskSage queue parameters (`target_repository`, `base_branch`, bounded
scan/dispatch limits, retry window, and conflict-repair policy). It grants `contents: read` and
`id-token: write`, because the central scheduler exchanges the established OpenCode application
identity through OIDC. The product repository does not hold a local write token for this loop.

DiskSage does not discover providers or models, call inference endpoints, choose a paid fallback,
or copy contextual-orchestrator provider credentials. Model-backed review remains behind the
central owner contract, whose protected workflow fixes OpenCode review to
`contextual-orchestrator/orchestrator/free`; contextual-orchestrator owns provider discovery and
runtime failover behind that virtual model.

This preserves the product/domain boundary: DiskSage owns disk-space, filesystem, deletion,
recovery, and platform-adapter truth; `.github` owns review/recovery workflow orchestration; and
contextual-orchestrator owns model/provider routing.

## Consequences

- The manual product entry point and the central missed-event recovery use one review/repair
  implementation instead of two model-routing implementations.
- The workflow is exact-SHA pinned, so a protected owner change requires an explicit consumer
  bump rather than silently changing DiskSage behavior.
- DiskSage no longer needs `CONTEXTUAL_ORCHESTRATOR_URL`,
  `CONTEXTUAL_ORCHESTRATOR_TOKEN`, `/v1/models`, or `/v1/chat/completions` in its own workflow.
- Provider credentials stay inside contextual-orchestrator's owner boundary. No Bytez, NVIDIA
  NIM, OpenRouter, OpenAI, or Copilot provider secret is accepted by the DiskSage caller.
- The repository-local workflow is not a second scheduler. It has no `schedule` trigger; normal
  PR/review events plus the central owner recovery cadence remain authoritative.

## Rejected alternatives

- **Keep dynamic `/v1/models` discovery in DiskSage:** rejected because product-local model
  selection duplicates contextual-orchestrator policy and can drift from the organization free
  pool.
- **Pin a provider/model group in DiskSage:** rejected because provider routing belongs to
  contextual-orchestrator; the workflow-level contract is only `orchestrator/free` through the
  central owner.
- **Copy the central scheduler into DiskSage:** rejected because source copies create mutable
  dual ownership and bypass the reusable-workflow contract.
- **Call the central workflow by branch name:** rejected because a mutable branch cannot provide
  exact workflow provenance.
- **Restore a repository-local schedule:** rejected because the organization control plane owns
  recurring recovery and already carries DiskSage in its target registry.

## Evidence

- `.github/workflows/hourly-product-loop.yml`
- `src/lib/hourlyProductLoopContract.test.ts`
- `src/lib/hourlyProductLoopWorkflow.test.ts`
- `ContextualWisdomLab/.github@e6334e229581a918e2f22de18733b76fa65d7e71`
  - `.github/workflows/hourly-review-repair.yml`
  - `.github/workflows/pr-review-fix-scheduler.yml`
  - central OpenCode contract tests pinning `contextual-orchestrator/orchestrator/free`

## Evidence basis

- Saltzer, J. H., & Schroeder, M. D. (1975). The protection of information in computer systems.
  *Proceedings of the IEEE, 63*(9), 1278–1308. https://doi.org/10.1109/PROC.1975.9939
- Joint Task Force. (2020). *Security and privacy controls for information systems and
  organizations* (NIST SP 800-53 Rev. 5). National Institute of Standards and Technology.
  https://doi.org/10.6028/NIST.SP.800-53r5

## Related decisions

- [ADR-0005](0005-hourly-agent-loop-is-advisory.md) — historical bootstrap design, superseded.
- [ADR-0007](0007-pre-copy-evidence-cohort.md) — fail-closed evidence cohort.
