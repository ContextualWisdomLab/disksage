# ADR-0002: Separate observation, decision support, approval, execution, and receipts

## Status

Proposed in PR #137. The separation is already visible in protected-main product behavior and is made explicit here as a canonical architecture decision after integration.

## Context

DiskSage handles evidence whose meaning differs materially: filesystem metadata, provider capacity, synchronization state, model judgments, human approval, mutation results, and durable receipts. Collapsing those states into a single `safe`, `ready`, or `success` Boolean creates unsafe implication chains—for example treating a successful scan as delete permission or a provider-client process as proof of remote durability.

## Decision drivers

- Observations become stale independently.
- Advice can be wrong without implying a security failure.
- Human intent must be attributable and scope-bound.
- Execution must revalidate current state.
- Receipts describe past outcomes and cannot become future authority.
- External consumers need explicit missing/unknown states.

## Alternatives considered

### Single readiness Boolean

Rejected because it erases which evidence is missing and makes accidental authority escalation easy.

### UI-owned workflow state

Rejected because UI state can be stale, duplicated, or manipulated and is not the durable validation boundary.

### Typed evidence/authority stages

Selected.

## Decision

DiskSage models at least five distinct stages:

1. **Observation** — bounded read-only facts and fingerprints.
2. **Decision support** — candidates, rankings, explanations, uncertainty, blockers.
3. **Approval** — attributed human intent bound to exact current plan/scope/freshness.
4. **Execution** — the one narrowly authorized mutation after last-moment revalidation.
5. **Receipt/evidence** — bounded record of what occurred and what remains unproven.

Unknown, missing, malformed, contradictory, stale, or out-of-bound evidence is represented explicitly and fails closed. The default mutation approval lifetime is 15 minutes and cannot be refreshed by a retry, model, UI state, or workflow.

## Consequences

### Positive

- Safety claims become explainable and testable.
- Cross-service evidence can preserve uncertainty rather than inventing success.
- A stale provider signal cannot silently authorize a current mutation.
- Audit and acquisition reviewers can trace which authority made each decision.

### Negative

- More schemas and state transitions must be maintained.
- UI copy must explain distinctions that simpler cleanup products may hide.
- Tests require adversarial combinations of incomplete and conflicting evidence.

## Failure and recovery

A failure in one evidence source blocks only operations requiring that source. It does not erase unrelated observations. A changed plan invalidates the old approval and requires fresh observation/planning/approval rather than attempting to patch the old record in place.

## Security and governance impact

Stable error/blocker codes are preferred for shareable evidence. Private paths/account detail remain local. No model, status, check, scanner result, or provider response can cross directly into runtime mutation authority without the explicit typed authorization boundary.

## Verification and acceptance

Tests must prove:

- missing/unknown evidence never normalizes to zero or approval;
- candidate/plan fingerprints change when authority-relevant state changes;
- stale/mismatched human approval is refused;
- execution revalidates before mutation;
- receipts do not claim facts the operation did not prove;
- public evidence excludes prohibited private coordinates.

## Migration and rollback

New evidence types must map to one stage and state what they do not prove. Historical schemas may remain readable only through explicit compatibility code. Rolling back cannot restore an ambiguous Boolean contract if that would collapse authority distinctions.

## Supersession conditions

Supersede only with a model that preserves or improves explicit provenance, authority separation, freshness, unknown-state semantics, and testability.