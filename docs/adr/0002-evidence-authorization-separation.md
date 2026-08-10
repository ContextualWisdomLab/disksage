# ADR-0002 — Separate evidence, decision support, approval, execution, and receipts

**Status:** Proposed canonicalization of integrated product behavior.

## Context

Storage systems produce many persuasive but incomplete signals: size, age, provider runtime, capacity, queue state, model advice, hashes, and prior receipts. Treating one signal as permission creates data-loss risk.

## Drivers

- prevent authority-by-implication;
- expose uncertainty honestly;
- support auditable human approval;
- make stale/replayed plans fail closed;
- preserve independent provider evidence classes.

## Alternatives considered

1. single “safe_to_delete” Boolean — rejected;
2. model/heuristic score above a threshold grants action — rejected;
3. explicit staged evidence and authorization state machine — selected.

## Decision

DiskSage distinguishes `evidence_snapshot`, `action_plan`, blocker/decision-support evidence, `approval_record`, execution, and `execution_receipt`. A plan is not approval. Approval binds exact current plan/scope/fingerprints, backend-authored phrase, attributed human, rationale, and bounded freshness. Mutation revalidates current preconditions immediately before execution.

Cloud-copy authorization currently uses a maximum 15-minute lifetime; expiration or inconsistent clocks fail closed. No per-operation convenience path may silently treat an old approval as current.

## Consequences

Workflows are more explicit and may require re-approval after drift. This is intentional: user intent belongs to the action actually executed.

## Failure and recovery

Unknown/malformed/stale/incomplete evidence remains blocking. Partial mutation recovery removes only invocation-owned output or exact captured identities and preserves source unless separately authorized.

## Security and governance impact

Provider capacity, provider-client presence, sync evidence, copy completion, and eviction authority remain distinct. Repository evidence remains separate from runtime operator authority.

## Verification and acceptance

Regression suites cover stale plans, scope mismatch, phrase mismatch, expiry/clock reversal, provider evidence confusion, race-safe revalidation, and receipt non-reusability.

## Migration and rollback

Durable old receipt/approval formats require explicit backward-read semantics. A rollback may not reinterpret newer records under weaker approval rules.

## Supersession

Any proposal that collapses these evidence classes requires a new threat analysis and must demonstrate equal or stronger prevention of stale/implicit authorization.