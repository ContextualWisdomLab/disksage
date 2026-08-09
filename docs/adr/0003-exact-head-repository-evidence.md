# ADR-0003: Bind repository decisions to exact source head and live base

## Status

Proposed in PR #137 as the canonical repository-evidence contract.

## Context

DiskSage development is highly automated and uses GitHub Checks, Actions, security scanners, model reviewers, human reviews, stacked pull requests, and organization-level reusable workflows. These evidence sources are asynchronous and can refer to different commits. Treating a previous head, generated merge commit, PR-body snapshot, status context, or queued workflow as current success can authorize a merge that has never actually been reviewed or tested.

## Decision drivers

- PR heads and base branches move independently.
- Stacked PRs depend on exact predecessor ancestry.
- Check runs, statuses, formal reviews, and automated findings have different semantics.
- Required approval cannot be synthesized from comments or statuses.
- Release evidence must refer to the integrated protected source, not a predecessor PR head.

## Alternatives considered

### Reuse the latest green evidence regardless of head

Rejected because it makes stale evidence transferable.

### Trust GitHub's mergeable Boolean alone

Rejected because mergeability does not prove review, security, coverage, provenance, or repository-policy completion.

### Exact source head plus independently resolved live base, with evidence classes kept separate

Selected.

## Decision

Every merge/release decision re-fetches:

- exact current source head SHA;
- current base branch and independently resolved live base tip;
- stack predecessor and ancestry where applicable;
- required checks/workflow runs and their conclusions;
- commit statuses separately from check runs;
- formal reviews and unresolved review threads;
- human/automated/security findings;
- branch protection/ruleset/repository policy;
- package/provenance/release evidence when applicable.

Queued, pending, skipped-required, cancelled, neutral-required, absent, stale-head, predecessor-head, status-only, synthetic-only, action-required, rate-limited, or failed evidence is not passing.

Formal independent approval, when required by live policy or explicit DiskSage/CWL governance, must be an eligible non-author review anchored to the unchanged current head. Comments, reactions, check statuses, model text, author approval, dismissed/stale reviews, and synthetic identities do not qualify.

## Consequences

### Positive

- Review and CI claims become auditable.
- Stacked PRs cannot silently inherit parent/predecessor authorization.
- Scanner/reviewer service outages are distinguishable from source findings.
- Release acceptance can be traced to exact integrated source.

### Negative

- Frequent head changes invalidate otherwise useful prior evidence and may increase CI/review load.
- Automation must maintain more explicit state and avoid tight polling.
- A green synthetic merge result may still need an explicit source-head gate depending on repository policy.

## Failure and recovery

A missing or delayed evidence class blocks only the action that requires it. Automation records/defer-keys the exact PR/head/run/review identity and continues safe work elsewhere. It re-fetches after material state changes rather than assuming old failure or success remains current.

## Security and governance impact

The contract reduces stale-check, spoofed-review, and wrong-head authorization risk. It also prevents broad bot permissions from being provisioned merely to manufacture counted approval. Repository evidence is independent of local runtime operator authorization.

## Verification and acceptance

Automation and documentation tests must preserve:

- exact-head/live-base wording;
- evidence-class separation;
- stale/pending evidence refusal;
- stack-order handling;
- eligibility-aware approval semantics;
- no bypass of branch/ruleset/security requirements.

Operational acceptance should inspect actual current GitHub state rather than relying on PR descriptions or remembered run IDs.

## Migration and rollback

Changing required check names or review/ruleset policy requires updating the evidence inventory, automation, and tests together. Rollback must not fall back to older-head success reuse.

## Supersession conditions

Supersede only if GitHub or another SCM provides a stronger atomic attestation that cryptographically and semantically binds source head, base, checks, reviews, policy, and release artifacts without losing independent evidence classes.