# ADR-0002: Cache cleanup is per-item active-use evidence bound

**Status:** Accepted  
**Date:** 2026-08-19

## Context

Package-manager and tool caches can share one root while individual entries have different
lifecycle states. For example, `~/.cache/uv/archive-v0` can contain live MCP runtimes next to
reproducible, unused environments. A root-wide active-use observation either blocks safe cleanup
of unrelated entries or encourages an unsafe manual bypass. Cache contents are not user-file
lineage and must not be uploaded to a cloud provider merely to reclaim local space.

## Decision

DiskSage exposes known cache roots through the existing cache catalog, including the macOS uv
cache. Cleanup uses the reviewed child manifest (`path`, byte count, modification time, and object
identity) and revalidates that manifest immediately before mutation. Active-use evidence is
collected independently for each reviewed child with bounded, path-local `lsof` evidence
(recursive for directories and direct for regular files):

- incomplete evidence or an active process leaves that child untouched and returns a stable blocker;
- an inactive child may be moved through DiskSage's identity-bound OS-Trash path;
- the cache root and all unrelated children remain untouched;
- the operation is journaled and never permanently deletes cache content.

This per-item probe is the authoritative cleanup boundary. A live process elsewhere under the
same cache root must not prevent reclaiming an independently inactive entry, and it must never be
treated as evidence that the inactive entry is safe without its own probe.

## Consequences

- A user can clean inactive uv archive entries while active MCP/uv runtimes continue running.
- Changed, replaced, symlinked, or unreadable entries fail closed before they reach the OS Trash.
- The operation is reversible through the OS Trash; physical space is not claimed until the user
  empties that Trash, and APFS shared blocks may make physical reclaim smaller than logical size.
- Cache cleanup does not create cloud-copy receipts, provider-sync evidence, or source-eviction
  permits. User files still require the cloud-offload ADR and its provider evidence gates.

## Alternatives rejected

- **Root-wide active-use probe:** safe but unnecessarily blocks unrelated inactive entries.
- **Direct recursive deletion:** not reversible and cannot prove per-entry identity at mutation time.
- **Copying caches to iCloud/OneDrive/Google Drive:** wastes cloud capacity for reproducible data and
  conflates cache cleanup with user-file lineage.

## Incident policy: observed macOS regenerable caches

When provider upload is blocked and local pressure is high, DiskSage may run the
`clean_regenerable_caches` command without a second approval prompt for the three caches observed
in the incident (`pnpm`, Adobe, and Microsoft Edge). This is a narrow policy, not a general
path-based delete rule: each direct child is still bound to its reviewed object identity, byte
count, and modification time, and the active-use probe must be complete and idle. The cache root
is preserved, successful operations go to OS Trash, and a journal entry is written. Edge or any
other child in use is reported and left untouched.

## References

- [ADR-0001: Provider evidence drives the cloud-offload Goal](0001-cloud-offload-goal-state.md)
- `src-tauri/src/cache_cleanup.rs`
- `src-tauri/src/rules.rs`
