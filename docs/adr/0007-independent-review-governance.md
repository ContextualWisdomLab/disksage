# ADR-0007 — Separate realistic independent-review governance from CODEOWNERS enforcement

**Status:** Proposed canonical governance decision.

## Context

The organization has historically placed CODEOWNERS-required review on hold because a solo-maintainer configuration can make that gate unsatisfiable. That operational hold can be misread as either “all approvals are unnecessary” or an invitation to manufacture bot approval.

## Drivers

- preserve realistic separation of duties where policy actually requires it;
- avoid impossible governance configurations;
- distinguish formal GitHub review from comments/status/model text;
- prevent self-approval or broad bot permissions created solely to satisfy a count.

## Alternatives considered

1. require CODEOWNERS regardless of eligible reviewer pool — rejected as unsatisfiable;
2. disable every review expectation globally — rejected because live rules or explicit governance may still require independent review;
3. inspect live policy and eligible reviewer routes per exact head, while keeping CODEOWNERS hold narrowly scoped — selected.

## Decision

CODEOWNERS required-review enforcement remains on hold while no realistic independent code-owner pool exists. This does not automatically waive all other review/governance requirements.

Before calling approval a blocker, automation determines whether current GitHub rules, explicit DiskSage/CWL governance, or both require an independent non-author review; inspects formal reviews, requested users/teams, unresolved threads, CODEOWNERS/team routes, and collaborator/App eligibility; and exhausts legitimate autonomous review-delivery routes without manufacturing identity or authority.

COMMENTED reviews, comments, reactions, statuses, check runs, model verdicts, author reviews, dismissed/stale reviews, and predecessor-head reviews never qualify as a current formal `APPROVED` review.

## Consequences

Some merges may remain externally governed. That gate blocks only the merge, not other safe repository work. Review policy must be documented independently of tool availability.

## Failure and recovery

A 422/ineligible reviewer route is recorded as disproven until eligibility changes and is not spammed. If governance becomes unsatisfiable, the minimum policy/people action is surfaced only after all other safe work is exhausted.

## Security/governance impact

Never self-approve, impersonate another person, use an alternate author credential, or grant a bot broad write solely to manufacture approval. Never reduce security checks to unblock review.

## Verification/acceptance

Merge automation proves reviewer eligibility where possible and binds qualifying review to the unchanged exact source head. Head changes invalidate dependent review evidence according to repository policy.

## Migration/rollback

When the organization gains a real independent maintainer/reviewer pool, CODEOWNERS enforcement may be reconsidered through an explicit governance change with a dry-run/eligibility audit.

## Supersession

Supersede when organization-wide review governance is formally changed and the new route is both satisfiable and at least as resistant to self-approval/spoofing.