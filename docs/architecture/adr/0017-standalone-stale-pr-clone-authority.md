# ADR-0017: Standalone stale-PR clones require exact-head authority

- Status: Accepted
- Date: 2026-08-28

## Context

Secondary worktrees can be removed safely under ADR-0013 and ADR-0015, but their audit deliberately
preserves the primary checkout. A separate clone left on a PR head therefore remained invisible.

## Decision

DiskSage may propose a standalone clone only when it has exactly one registered worktree, a clean
working tree, complete recursive active-use and size evidence, a real `.git` directory directly
bounded by the canonical clone root, and a fresh same-repository GitHub branch-and-head match for a
closed PR or an operator-supplied stale-open cutoff. The complete audit must itself report no
evidence gap. Execution requires the exact plan phrase, a create-new approval record outside the
clone, a journal destination outside the clone, re-resolves every authority input, verifies the
filesystem object identity, and moves the clone to OS Trash. If the Trash move cannot complete, the
existing safety layer restores the staged object and retains the journaled failure for recovery.

DiskSage may also authorize a clean published clone when a fresh provider observation names the
default branch and its exact commit OID, the corresponding local remote-tracking reference resolves
to that same OID, and `git merge-base --is-ancestor` proves the clone HEAD reachable from it. This
path rejects stale provider evidence, stale local references, diverged or unpublished heads, and
active or dirty clones. Multi-root discovery is breadth-bounded, does not follow symlinks, stops at
repository roots, and reports an evidence gap when any entry or result limit is exceeded.

DiskSage never invents an age threshold, deletes the branch, runs `git prune`, handles fork or
detached heads, or reports physical capacity as reclaimed before Trash is emptied.

## Consequences

A changed, dirty, active, linked, protected, or unverifiable clone fails closed. The user must empty
Trash before the operating system can expose the capacity.

## Rejected alternatives

Directory age, clone folder names, local branch names alone, stale remote-tracking references, and automatic branch deletion are not
authority because each can destroy current or unpublished work.

## Evidence

The decision reuses the exact Git registration, status, retained-reference, same-repository GitHub
PR state and head OID, bounded size, active-use, and filesystem identity evidence already accepted
by ADR-0013 and ADR-0015.
