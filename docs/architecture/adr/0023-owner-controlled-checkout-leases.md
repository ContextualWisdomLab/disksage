# ADR-0023: Checkout leases are owner-controlled and durable

- Status: Accepted
- Date: 2026-08-29

## Context

A clean checkout can have no open file or running process while an automated or human task still
expects to return to it. Process absence therefore cannot prove that a checkout is abandoned.

## Decision

Before launching work in a standalone clone, its owner creates a durable DiskSage checkout lease.
The lease is bound to the filesystem object, issuance commit, owner, owner-supplied expiry, and an
exact fingerprint. Omitting expiry keeps it active until the owner explicitly releases it. DiskSage
never chooses or infers a lease duration. A valid active lease, or incomplete lease evidence,
vetoes planning and mutation even when process and open-file probes are idle. The exact lease is
re-read during the existing pre-mutation replan.

Lease lifecycle operations are serialized by a persistent, per-user private lock outside the
repository, keyed to the canonical Git common directory. DiskSage never replaces or removes that
lock, so Git's atomic replacement of `.git/config` cannot split concurrent processes across
different lock inodes. A release is complete only after removing the lease and durably syncing its
directory. If that sync fails, DiskSage restores and syncs the exact lease before returning the
failure, preserving the cleanup veto.

The private lock root creates a missing platform data-directory hierarchy before validating the
final lock directory as a non-symlink private directory. Platforms must also provide a tested
directory-entry durability primitive. Unix uses a directory `fsync`; Windows lease acquisition and
release fail closed until an equivalent native flush boundary is implemented and exercised on a
Windows runner. DiskSage does not report a durable lease where the operating-system contract has
not been proven.

Registered linked worktrees continue to use Git's native durable `worktree lock` veto. Agent
launchers must acquire the applicable lease or lock before yielding the checkout and release it
only when no later turn will reuse that folder.

The existing exact closed-PR, owner-cutoff stale-open-PR, and fresh default-branch ancestry
authorities remain available only when lease evidence is complete and valid and no active lease or
lock exists.

## Consequences

Dormant work can resume without its checkout disappearing, while explicitly old open-PR clones
remain reclaimable after the owner has released or allowed its exact lease to expire. Customer text
states the next action: finish the work, release preservation, and scan again.

## Rejected alternatives

- Treating no process or open file as inactivity: a paused task commonly has neither.
- A fixed lease duration: no evidence supports a universal timeout.
- A background lease daemon: a durable file and Git's native lock already cover the lifecycle.

## Evidence

The focused regression proves an idle clone satisfying the explicit stale-open authority remains
ineligible while an owner-created lease is active, that replacement of `.git/config` cannot bypass
the lifecycle lock, a failed release leaves the exact lease active, and eligibility follows only
an exact fingerprint-bound durable release. Platform regressions also cover a fresh Linux-style
profile with no existing local-data parents and Windows' explicit fail-closed durability boundary.
