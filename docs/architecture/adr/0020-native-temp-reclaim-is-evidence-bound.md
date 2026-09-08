# ADR-0020: Native temp reclaim is evidence-bound

**Status:** Proposed

## Context

`/tmp`, macOS `/private/tmp`, Linux temporary roots, and the Windows temporary directory mix
generated data with active processes and private application state. Age, modification time, or a
filename cannot prove that an entry is disposable. DiskSage already owns bounded generated-root
manifests, filesystem identity checks, active-handle evidence, OS Trash, and an undo journal.

The active container/runtime reclamation foundation reserves ADR-0012 through ADR-0019 on its
current owner lineage. This record therefore uses ADR-0020 rather than colliding with an earlier
active decision. It remains Proposed until its implementation and prerequisite lineage reach
protected authority.

## Decision

DiskSage resolves the operating system's native temporary root and canonicalizes macOS `/tmp` to
`/private/tmp`. Discovery is bounded by time, entry count, depth, and candidate count. It admits
only an existing development-artifact kind whose independent adjacent project marker, complete
metadata manifest, object identity, and inactive-handle observation all agree. Symlinks,
provider-managed roots, Photos libraries, unknown entries, partial scans, and timeouts remain
unavailable.

Execution requires a fresh human-attributed phrase bound to one candidate fingerprint. The
candidate and active-use evidence are rechecked before the existing identity-bound Trash and
journal helper runs. Permanent deletion is unavailable. No age or mtime value grants authority.

## Consequences

The initial adapter intentionally covers only directly nested generated build roots. Clean
closed/merged clones, worktrees, and DiskSage-owned nonce manifests require their own existing
lineage verifier before they may become additional adapters. Unknown temporary data stays put.
The record must not become Accepted merely because this branch is open or locally green; protected
integration and exact-current evidence are prerequisites.

## Rejected alternatives

- Reusing ADR-0012: rejected because an earlier active owner already uses that immutable identity.
- General age-based temp cleanup: rejected because time is not ownership or inactivity evidence.
- Recursive deletion or raw permanent deletion: rejected because it is not reversible.
- A second deletion engine: rejected in favor of the shared manifest, identity, Trash, and journal
  boundary.

## Evidence

The realistic fixture contains one Cargo-marked `target` and one unknown private directory. Only
the generated root is proposed. A macOS regression verifies that `/tmp` canonicalizes exactly to
`/private/tmp`; no live temporary root is mutated by the test.
