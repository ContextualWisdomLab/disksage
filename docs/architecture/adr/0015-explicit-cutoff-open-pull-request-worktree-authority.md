# ADR-0015: Require an explicit cutoff for stale open pull-request worktrees

## Context

An open pull request can leave a local worktree behind even when its branch is no longer being
used. Git reachability and filesystem timestamps do not establish that the worktree is safe to
remove: the pull request may still receive commits, and a local branch can be intentionally
retained. DiskSage must support this cleanup without inventing an age threshold or exposing
implementation details in customer-facing copy.

## Decision

The operator may opt in to open-pull-request cleanup by entering an explicit UTC calendar cutoff.
DiskSage queries the authenticated GitHub CLI at plan time and again immediately before each
removal. Only an OPEN pull request from the same repository whose `createdAt` is strictly before
the supplied cutoff, whose exact head branch and OID match the local worktree, and whose worktree
passes every existing clean, inactive, non-primary, non-retained, non-locked, and complete-evidence
gate may authorize removal of the registered worktree directory. Fork pull requests, missing or
malformed timestamps, stale provider responses, and drift fail closed. Branches and commits are
never deleted.

No default age, filename date, filesystem mtime, upstream absence, or arbitrary score is used.
The cutoff and exact PR head set are included in the removal authority fingerprint, so approval
cannot be replayed after the forge evidence changes.

## Consequences

- Customers choose the policy boundary instead of receiving a hidden heuristic.
- A plan records the cutoff and the exact same-repository PR evidence used to produce it.
- The GitHub CLI is an optional, explicit evidence source; without it, the existing merged-history
  and manual review paths remain available.
- Open pull requests created after the cutoff are preserved until a later, newly approved plan.

## Rejected alternatives

- Automatically deleting worktrees older than a fixed number of days: no user-authorized basis.
- Deleting local branches or commits: exceeds the worktree-folder cleanup responsibility.
- Treating a missing remote branch or filesystem timestamp as pull-request state: not authoritative.

## Evidence

- GitHub REST/CLI pull-request state, head OID, repository identity, and `createdAt` are refreshed
  at each mutation boundary.
- The implementation and tests live in `src-tauri/src/git_worktree.rs` and
  `src/lib/GitWorktreeCleanup.svelte`.
