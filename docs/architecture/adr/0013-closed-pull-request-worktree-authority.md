# ADR-0013: Bind closed pull-request worktree cleanup to forge evidence

**Status:** Accepted
**Date:** 2026-08-27

## Context

Git reachability proves that a worktree commit is merged into a retained ref, but squash and rebase
merges need not retain the pull-request head in that ancestry. It also cannot prove that an unmerged
pull request was closed. Branch age, a missing upstream, or a deleted remote branch would be
unsupported heuristics. GitHub CLI exposes structured pull-request state and head identity.

## Decision

When the operator includes closed pull requests, DiskSage obtains bounded structured evidence from
the authenticated GitHub CLI. A clean, inactive secondary worktree is eligible only when:

1. the PR state is exactly `CLOSED` or `MERGED`; merged evidence is queried only for branch names
   registered in the current Git worktree list, so repository-wide merged history cannot crowd the
   bounded authority set; all forge queries share one overall timeout budget;
2. the PR is from the same repository, not a fork;
3. GitHub search discovers candidate PRs for each exact registered worktree HEAD, and DiskSage
   independently verifies that SHA against the candidate PR's paginated commit list;
4. any verified membership in an open PR vetoes removal, even when the same SHA also occurs in a
   closed or merged PR;
5. the worktree is not primary, selected, locked, prunable, dirty, active, or a retained tip; and
6. the same evidence is refreshed immediately before deletion and remains bound to the approved
   removal-plan fingerprint.

Detached worktrees may qualify through exact completed-PR commit membership, including an
intermediate commit, without relying on a branch name or ancestry that squash/rebase can rewrite.
Incomplete, malformed, timed-out, unauthenticated, truncated, or repository-mismatched forge
evidence fails closed. Branches and commits remain; only the registered worktree folder is removed.
Runtime diagnostics are not returned across the customer-visible boundary.

## Consequences

- Merged worktree cleanup remains available through retained-ref ancestry or exact forge evidence.
- Closed-but-unmerged cleanup requires an authenticated GitHub connection and explicit selection.
- Fork PR worktrees require manual review because their local branch identity is not authoritative.
- Another forge can later supply its own authoritative adapter without weakening this contract.

## Rejected alternatives

- Branch age, upstream absence, and remote-branch deletion are not PR-state evidence.
- Unverified OID-only matching is insufficient; a SHA must be rebound to an exact same-repository
  PR commit list and open membership always wins.
- Deleting branches or commits is outside the worktree-folder cleanup authority.

## Reference

GitHub. (2026). *GitHub CLI manual: gh pr list*. https://cli.github.com/manual/gh_pr_list

GitHub. (2026). *REST API endpoints for pull request commits*.
https://docs.github.com/rest/pulls/pulls#list-commits-on-a-pull-request
