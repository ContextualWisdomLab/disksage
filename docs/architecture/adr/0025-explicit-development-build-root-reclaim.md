# ADR-0025: Development build-root reclaim requires an explicit workspace selection

- Status: Proposed
- Date: 2026-08-30

## Context

A manual incident recovered about 6.7 GiB from a Cargo `target` tree. Age alone does not prove that
generated output is inactive or reproducible, and a similarly named directory can contain user
data or be managed by a cloud File Provider.

This decision was originally drafted as ADR-0012 on the development-artifact branch. The current
release/Test foundation independently owns ADR-0012 for container-orphan reclaim, so keeping the
same number would make two different decisions appear to share one immutable record identity.
Until this branch converges on the protected lineage and earns current-head acceptance, this record
remains Proposed rather than prematurely Accepted.

## Decision

DiskSage inspects only a user-selected, real development workspace directory. A candidate must be
`target`, `node_modules`, `.venv`, or `venv`; its project directory must contain both the ecosystem
marker and a recognized lockfile. Provider-managed ancestry, dataless placeholders, symlinks,
incomplete bounded manifests, zero physical allocation, changed filesystem identity, and active or
inconclusive open-handle evidence all block cleanup.

Age remains informational and never grants deletion authority. The review screen shows current
local allocation and tells the customer to close development tools, select exact items, and approve
an OS Trash move. Execution rebuilds the manifest, rechecks identity and active use, atomically
stages the same object, writes the existing journal, and uses the existing reversible Trash path.
DiskSage does not offer permanent deletion for this workflow; adding it requires a separate,
explicit irreversible approval contract and receipt.

## Consequences

The workflow can recover locally allocated build output without treating old age as evidence. Very
large or unreadable trees may require a new scan and remain untouched. Empty or sparse-only trees
with no allocated blocks are not presented as reclaim opportunities.

The ADR number no longer collides with the canonical release/Test foundation. Acceptance is deferred
until ordinary/non-force foundation convergence, current-head platform evidence, and the unresolved
Windows active-use deadline boundary are complete.

## Rejected alternatives

- Age thresholds: elapsed time is not reproducibility or inactivity evidence.
- Broad cache deletion: the selected project and exact generated root remain the authority bounds.
- Automatic permanent deletion: this workflow remains reversible by default.
- Reusing ADR-0012: two unrelated immutable decisions cannot share one ADR identity.
- Marking this record Accepted while the owner PR is Draft/diverged: source-level implementation is
  not protected-lineage decision acceptance.

## Evidence

The product incident recovered about 6.7 GiB by removing an inactive Cargo build root. The tiny
regression fixture reproduces its marker, lockfile, generated-root, allocation, and approval shape
without allocating gigabytes or touching a live workspace.

On Windows, the bounded active-use observation registers the complete candidate file inventory in
one Restart Manager session and obtains affected process identities with `RmGetList`; an incomplete
inventory, API failure, timeout, or truncated process list blocks cleanup (Microsoft, 2024a, 2024b).

Microsoft. (2024a, February 22). *RmRegisterResources function (restartmanager.h)*. Microsoft Learn.
https://learn.microsoft.com/windows/win32/api/restartmanager/nf-restartmanager-rmregisterresources

Microsoft. (2024b). *RmGetList function (restartmanager.h)*. Microsoft Learn.
https://learn.microsoft.com/windows/win32/api/restartmanager/nf-restartmanager-rmgetlist
