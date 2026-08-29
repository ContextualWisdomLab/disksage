# DiskSage product requirements

## Outcome

Help a person recover a measured disk-space target without losing irreplaceable data. The target is
an observation goal, never deletion authority.

## Required product loop

1. Measure allocated bytes by independent reclaim domain.
2. Show the evidence gap and the next action for every blocked candidate.
3. Require provider, relationship, runtime, or repository authority appropriate to that domain.
4. Revalidate identity and authority immediately before mutation.
5. Prefer reversible OS Trash or provider-native local eviction; retain an auditable receipt.
6. Re-measure physical capacity and continue until the target is met or all remaining bytes are
   explicitly unresolved.

## Acceptance

- Cloud-local eviction requires current provider upload, identity, conflict, and materialization
  evidence; `local-current` with `is_uploaded=false` is blocked.
- Exact duplicates use content identity. Non-identical photos require measured media evidence and
  a human-selected survivor.
- Container and VM maintenance never removes active resources or rewrites raw VM images.
- Worktrees and standalone clones require fresh exact Git/GitHub authority, clean and inactive
  state, complete and valid lease evidence, no active owner-created lease or Git worktree lock,
  explicit approval, and no branch deletion or Git pruning. Lease expiry is supplied by its owner;
  DiskSage never invents one.
- All customer-visible text states what happened, what remains blocked, and the next safe action;
  it does not expose implementation boundaries.
