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
   bounded authority set;
2. the PR is from the same repository, not a fork;
3. the local `refs/heads/<headRefName>` and exact worktree HEAD equal the reported ref and OID;
4. the worktree is not primary, selected, locked, prunable, dirty, active, or a retained tip; and
5. the same evidence is refreshed immediately before deletion and remains bound to the approved
   removal-plan fingerprint.

Detached worktrees and incomplete, malformed, timed-out, unauthenticated, or truncated forge
evidence fail closed. Branches and commits remain; only the registered worktree folder is removed.
Runtime diagnostics are not returned across the customer-visible boundary.

## Consequences

- Merged worktree cleanup remains available through retained-ref ancestry or exact forge evidence.
- Closed-but-unmerged cleanup requires an authenticated GitHub connection and explicit selection.
- Fork PR worktrees require manual review because their local branch identity is not authoritative.
- Another forge can later supply its own authoritative adapter without weakening this contract.

## Rejected alternatives

- Branch age, upstream absence, and remote-branch deletion are not PR-state evidence.
- OID-only matching is insufficient because different branches can share a commit.
- Deleting branches or commits is outside the worktree-folder cleanup authority.

## Reference

GitHub. (2026). *GitHub CLI manual: gh pr list*. https://cli.github.com/manual/gh_pr_list
