# ADR-0012: Development build-root reclaim requires an explicit workspace selection

- Status: Accepted
- Date: 2026-08-30

## Context

A manual incident recovered about 6.7 GiB from a Cargo `target` tree. Age alone does not prove that
generated output is inactive or reproducible, and a similarly named directory can contain user
data or be managed by a cloud File Provider.

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

## Rejected alternatives

- Age thresholds: elapsed time is not reproducibility or inactivity evidence.
- Broad cache deletion: the selected project and exact generated root remain the authority bounds.
- Automatic permanent deletion: this workflow remains reversible by default.

## Evidence

The product incident recovered about 6.7 GiB by removing an inactive Cargo build root. The tiny
regression fixture reproduces its marker, lockfile, generated-root, allocation, and approval shape
without allocating gigabytes or touching a live workspace.
