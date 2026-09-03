# ADR-0002: Cache cleanup is per-item active-use evidence bound

**Status:** Superseded by ADR-0012  
**Date:** 2026-08-20

## Context

Package-manager and tool caches can share one root while individual entries have different
lifecycle states. For example, `~/.cache/uv/archive-v0` can contain live MCP runtimes next to
reproducible, unused environments. A root-wide active-use observation either blocks safe cleanup
of unrelated entries or encourages an unsafe manual bypass. Cache contents are not user-file
lineage and must not be uploaded to a cloud provider merely to reclaim local space.

A second boundary applies after cache data reaches OS Trash. An irreversible operation cannot be
authorized merely because a pathname, cache name, aggregate byte count, or structural signature
matched during an earlier preview. A same-user process can replace a pathname between validation
and deletion, and equal-sized descendant changes can preserve shallow metadata. A crash after an
irreversible mutation can also leave an ambiguous journal unless recovery semantics are explicit.

## Decision

### Reversible cache cleanup

DiskSage exposes known cache roots through the cache catalog. Cleanup uses the reviewed child
manifest and revalidates current identity before mutation. Active-use evidence is collected
independently for each reviewed child with bounded path-local evidence:

- incomplete evidence or an active process leaves that child untouched and returns a stable blocker;
- an inactive, unchanged child may move through DiskSage's identity-bound OS-Trash path;
- the cache root and unrelated children remain untouched;
- the operation is journaled; and
- the ordinary cleanup path never permanently deletes cache content.

A live process elsewhere under the same cache root is not evidence that another entry is active or
safe. Each reviewed child owns its own evidence and decision.

### Irreversible cache-Trash deletion

Permanent cache-Trash deletion is **not an available DiskSage capability until the final destructive
primitive is proven race-safe and recoverable**. A legacy pathname-based implementation existing in
source history or an unintegrated branch is not product authority and must not be presented as an
operator-supported execution path.

Any future implementation may be enabled only when all of the following are true on one protected,
reviewed revision:

1. preview produces an exact bounded candidate identity for a direct OS-Trash child;
2. the reviewed identity covers the relevant descendant tree, so equal-sized nested replacement is
   detected rather than accepted;
3. approval has an explicit freshness/expiry boundary and cannot be reused indefinitely;
4. validation and irreversible deletion are bound to the same filesystem object using a
   descriptor-relative/no-follow or equivalently strong platform primitive rather than a later
   pathname lookup;
5. concurrent rename/replacement cannot redirect deletion to an unreviewed object;
6. pending and terminal evidence are durable, and restart/retry reconciliation can represent a
   completed deletion whose terminal write initially failed without deleting again;
7. newly appearing candidates never inherit authority from a previous preview;
8. symlinks/reparse points, nested candidates, unknown names, user files, provider placeholders and
   arbitrary Trash entries fail closed; and
9. platform-specific tests prove the boundary on every platform where the capability is enabled.

Until that acceptance evidence exists, the supported interface is read-only candidate inspection.
An implementation branch may fail closed earlier than protected `main`; neither state authorizes
permanent deletion until the safe implementation itself reaches protected authority.

## Consequences

- A user can clean independently inactive regenerable cache entries while active runtimes continue
  running elsewhere under the same cache root.
- Ordinary cleanup remains reversible through OS Trash. Logical bytes are not promoted to physical
  recovery until filesystem evidence supports that claim.
- A structurally recognizable cache already in Trash may be shown as review evidence, but preview
  evidence alone is never irreversible mutation authority.
- Product and operator documentation must not advertise an irreversible command while the race,
  descendant-identity, freshness, or recovery requirements above remain unsatisfied.
- A future safe implementation must land through its canonical implementation owner and then be
  reflected here; documentation branches must not duplicate that runtime repair.
- Cache cleanup creates no cloud-copy receipt, provider-sync evidence, or source-eviction permit.
  User files remain governed by the cloud-offload/reversible-removal boundaries.

## Alternatives rejected

- **Root-wide active-use probe:** safe but unnecessarily blocks unrelated inactive entries.
- **Direct recursive deletion by previously checked pathname:** rejected because validation does not
  stay bound to the object consumed by the destructive call.
- **Structural signature plus aggregate bytes as deletion identity:** rejected because equal-sized
  descendant replacement can preserve that shallow evidence.
- **Execution-time rescan as approval:** rejected because a newly appearing candidate was not shown
  to the operator and has no human-attributed exact approval.
- **Permanent deletion with best-effort terminal journaling:** rejected because post-delete journal
  failure creates ambiguous recovery state.
- **Copying caches to iCloud/OneDrive/Google Drive:** rejected because reproducible caches are local
  cleanup data, not user-file lineage.

## Incident policy: observed regenerable caches

The automatic regenerable-cache action remains deliberately narrow. It may act only on catalogued
roots whose rebuild and active-use contracts are understood, and every selected child still needs
complete current evidence. DiskSage staging entries such as `.disksage-trash-*` are not recursive
cleanup targets. Cataloguing an additional cache root does not automatically grant mutation
authority for it.

During low-disk development incidents, DiskSage used this distinction to preserve active uv/Cargo
work, provider-managed data and user files while reclaiming only explicitly regenerable local
artifacts. Those observations are development evidence, not a standing permission to delete future
paths with similar names.

## Operator status

The current operator contract is documented in
[`docs/development/cache-cleanup-operator-runbook.md`](../../development/cache-cleanup-operator-runbook.md).
It exposes reversible cleanup and read-only proven-cache inspection. If a checked-out source revision
still contains a legacy irreversible execution path, operators must treat it as a known defect, not
as an approved DiskSage capability, until the canonical fail-closed/safe implementation is
integrated and verified.

## References

- [ADR-0001: Provider evidence drives the cloud-offload Goal](0001-cloud-offload-goal-state.md)
- [ADR-0012: Cache Trash permanent deletion fails closed](0012-cache-trash-permanent-delete-fails-closed.md)
- `src-tauri/src/cache_cleanup.rs`
- `src-tauri/src/bin/disksage-cache-cleanup.rs`
- `src-tauri/src/rules.rs`
