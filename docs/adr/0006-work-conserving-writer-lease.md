# ADR-0006 — Work-conserving autonomous maintenance with a branch-local writer lease

**Status:** Proposed canonical automation/governance decision.

## Context

Earlier autonomous runs repeatedly stopped after one useful action or spent the remainder of a run polling a queued check/reviewer. Temporary self-modifying repair workflows also created overlapping writer authority and stale-branch risk.

## Drivers

- maximize safe repository progress per finite invocation;
- never race another writer;
- prevent review/check latency from becoming global idle time;
- make RCA lead to realistic action rather than blocker narration;
- preserve exact source/base evidence.

## Alternatives considered

1. one PR/one fix per hourly run — rejected because it strands safe work;
2. poll one target until completion — rejected because model/CI/review can take hours;
3. multiple concurrent branch writers — rejected because source races invalidate evidence;
4. one authoritative DiskSage writer with branch-local deferral and a live work queue — selected.

## Decision

The dedicated DiskSage loop owns repository writes. Immediately before each write it re-fetches the exact target head, independently resolved live base, relevant review/security state, and target blob/ref. Source movement or another writer on the same branch freezes only that branch for the rest of the invocation.

Waiting is local: queued CI, OpenCode/CodeRabbit latency, provider cooldown, central dependency, or missing approval defers the exact lane and the loop immediately executes another safe PR, issue, operational proof, documentation defect, or bounded product slice.

A completed RCA, commit, merge, documentation update, review request, or blocked lane is never a run-completion reason while safe work remains. The run ends only at practical tool/runtime budget exhaustion or after two fresh whole-repository sweeps prove every remaining path non-actionable.

Temporary self-modifying/encoded-patch/one-shot branch repair workflows are not an accepted maintenance mechanism.

## Consequences

Runs may perform multiple sequential non-conflicting actions. The scheduler prompt must remain concise enough to execute yet explicit about queue rotation and exit criteria. Branch-local freezes may defer a valid fix to the next hourly invocation.

## Failure and recovery

A failed remedy becomes new RCA evidence and triggers a materially distinct safe option. After three cross-layer failed hypotheses, reassess architecture/governance rather than stacking patches.

## Security/governance impact

The model avoids competing writers and stale evidence. It never invents credentials, reviewers, permissions, or bypasses. Other dedicated CWL repositories remain read-only dependencies.

## Verification/acceptance

Automation reviews must demonstrate fresh-state inventory, exact pre-write identity, local deferral of waits, queue rotation, stale-branch convergence, and double exit sweep. Routine status output is not accepted as completion evidence.

## Migration/rollback

Existing overlapping or self-modifying loops are disabled/removed when their scope duplicates this writer. Rollback may reduce automation but must not reintroduce competing branch writers.

## Supersession

Supersede if a transactional repository-writer coordination mechanism provides stronger mutual exclusion, progress, and auditable evidence.