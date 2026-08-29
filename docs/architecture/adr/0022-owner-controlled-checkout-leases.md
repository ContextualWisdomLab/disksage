# ADR-0022: Checkout leases are owner-controlled and durable

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
ineligible while an owner-created lease is active, then becomes eligible only after exact
fingerprint-bound release.
