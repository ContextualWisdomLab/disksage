# ADR-0003 — Bind repository decisions to exact source head and current live base

**Status:** Proposed canonical repository-governance decision.

## Context

PR descriptions, earlier workflow runs, synthetic merge commits, and API snapshots can become stale while source or the target branch moves. Review/check/status/model evidence also has different authority semantics.

## Drivers

- prevent predecessor-head evidence reuse;
- distinguish PR source from current target base;
- prevent statuses/model prose from substituting for required checks or formal reviews;
- support defensible acquisition/release evidence.

## Alternatives considered

1. trust the latest green result regardless of commit — rejected;
2. use PR `.base.sha` snapshots as current target identity — rejected when the base ref has moved;
3. bind decisions to exact source head plus independently resolved live base and separate evidence classes — selected.

## Decision

Every merge/release decision records the exact current source head and independently resolves the current live base tip. Check runs, commit statuses, formal reviews, automated reviewer findings, security scanner evidence, package/provenance evidence, and branch/ruleset authority remain distinct.

Queued, pending, cancelled, skipped-required, neutral-required, absent, stale-head, predecessor-head, synthetic-only, rate-limited, action-required, or failed evidence is not passing.

## Consequences

A head or base move invalidates dependent evidence and may require reruns/re-review. That cost is preferable to merging unverified source.

## Failure and recovery

If the evidence API or base resolution is unavailable, the dependent integration action remains blocked while unrelated repository work may continue.

## Security and governance impact

Formal independent review cannot be synthesized from comments/statuses/model verdicts. Runtime filesystem authorization is unaffected by repository approval.

## Verification and acceptance

Automation/source tests cover exact checkout, live-base resolution, stale-head refusal, evidence-type separation, and no transfer after replacement PRs are created.

## Migration and rollback

Historical PR bodies and older-run records remain historical evidence only. Rollback of this rule requires explicit governance review and cannot silently reuse predecessor evidence.

## Supersession

Supersede only with an evidence model that preserves commit/base identity and at least the same authority separation.