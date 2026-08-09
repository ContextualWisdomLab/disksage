# ADR-0005: Keep CWL central automation external to DiskSage runtime authority

## Status

Proposed in PR #137.

## Context

DiskSage participates in the ContextualWisdomLab ecosystem. The organization `.github` repository can supply reusable review, coverage, security, provenance, and release workflows; Naruon can consume bounded evidence; contextual-orchestrator can coordinate optional model-backed work. Tight coupling would make DiskSage impossible to operate independently and could blur repository governance, product runtime authority, and cross-service trust.

## Decision drivers

- DiskSage must be independently installable and useful.
- Central policy should be reusable without duplicating it locally.
- A central CI/review success cannot become local filesystem permission.
- Another product should not need direct access to DiskSage private evidence or persistence.
- Dedicated repository writer loops must not race each other.

## Alternatives considered

### Copy organization workflows and policy into DiskSage

Rejected as a long-term pattern because policy drifts and duplicate schedulers waste Actions/review capacity.

### Make central services mandatory runtime dependencies

Rejected because outages would disable core product use and externalize local trust.

### Optional versioned integration plus external repository control plane

Selected.

## Decision

`ContextualWisdomLab/.github` is an external repository-governance control plane. DiskSage consumes its required shared workflows/policy when repository rules require them while retaining local repository-specific diagnostics and tests.

Naruon and other CWL services may consume bounded, versioned, path-free evidence contracts. contextual-orchestrator may handle explicitly enabled model routing. None receives ambient local mutation authority.

Repository writer ownership is explicit: the dedicated DiskSage maintenance loop writes DiskSage; repositories with their own enabled writer loops are read-only dependencies unless a separate non-conflicting lease is established. A waiting dependency blocks only the dependent action.

## Consequences

### Positive

- Organization governance can evolve centrally.
- DiskSage retains standalone availability and a clear local trust boundary.
- Cross-service contracts remain independently versionable and testable.
- Duplicate automation and writer races are easier to detect and remove.

### Negative

- Central workflow changes can block DiskSage even when local code is healthy.
- Integration adapters need compatibility and failure-mode tests.
- Operational RCA must distinguish local, central, provider, and governance failures.

## Failure and recovery

If a central workflow or external service is unavailable, automation identifies the first failing boundary, verifies whether any local remedy is realistic, defers the dependent action, and continues non-conflicting DiskSage work. Runtime product behavior degrades only the optional external capability; local authority is not broadened.

## Security and governance impact

Shared workflow references used for privileged automation should be immutably source-pinned where repository policy requires. Secrets remain purpose-bound. Review-agent credentials are not repurposed as development credentials. Autonomous model-backed development uses `NVIDIA_NIM_API_KEY`, not `COPILOT_GITHUB_TOKEN`.

Temporary self-modifying repair workflows, encoded patch finalizers, or broad cross-repository writer permissions are not accepted as a steady-state integration mechanism.

## Verification and acceptance

- Standalone tests run without CWL runtime services.
- Cross-service schemas fail closed on unknown versions.
- Shared workflow failures remain distinguishable from local source defects.
- Writer loops re-fetch target head/base/blob state before writes and avoid branch races.
- No central service can bypass Rust runtime approval checks.

## Migration and rollback

Changing a central contract requires versioned compatibility or coordinated migration. A central dependency can be disabled or replaced without corrupting local DiskSage evidence. Rollback must restore a known compatible contract, not weaken required security/review gates.

## Supersession conditions

Supersede only if a future platform provides stronger modularity, offline/degraded behavior, writer isolation, immutable provenance, and local authority preservation.