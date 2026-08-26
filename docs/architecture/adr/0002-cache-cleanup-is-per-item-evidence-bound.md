# ADR-0002: Cache cleanup is per-item active-use evidence bound

**Status:** Accepted  
**Date:** 2026-08-20

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
- the operation is journaled; the normal path never permanently deletes cache content.
- a separate cache-Trash review may surface structurally proven direct children of the native macOS
  Trash, but it does **not** grant permanent-delete authority while the final irreversible deletion
  primitive is pathname-recursive rather than bound to the exact reviewed filesystem object.

This per-item probe is the authoritative cleanup boundary. A live process elsewhere under the
same cache root must not prevent reclaiming an independently inactive entry, and it must never be
treated as evidence that the inactive entry is safe without its own probe.

## Consequences

- A user can clean inactive uv archive entries while active MCP/uv runtimes continue running.
- Changed, replaced, symlinked, or unreadable entries fail closed before they reach the OS Trash.
- The normal operation is reversible through the OS Trash; physical space is not claimed until the
  user empties that Trash, and APFS shared blocks may make physical reclaim smaller than logical
  size. DiskSage may identify proven cache entries already in Trash, but in-app permanent deletion
  remains disabled until the final irreversible deletion primitive itself is object-bound.
- Cache cleanup does not create cloud-copy receipts, provider-sync evidence, or source-eviction
  permits. User files still require the cloud-offload ADR and its provider evidence gates.

## Alternatives rejected

- **Root-wide active-use probe:** safe but unnecessarily blocks unrelated inactive entries.
- **Direct recursive deletion of live cache roots:** not reversible and cannot prove per-entry
  identity at mutation time.
- **Pathname-recursive deletion after an identity check or staging rename:** rejected because a
  same-user replacement can still occur between the last identity check and the final recursive
  pathname deletion. Until an exact-object-bound irreversible primitive exists, the operation must
  fail closed and preserve the reviewed cache.
- **Copying caches to iCloud/OneDrive/Google Drive:** wastes cloud capacity for reproducible data and
  conflates cache cleanup with user-file lineage.

## Incident policy: observed macOS regenerable caches

When provider upload is blocked and local pressure is high, DiskSage may run the
`clean_regenerable_caches` command without a second approval prompt for the observed regenerable
macOS roots (npm, uv, pnpm, Adobe, Microsoft Edge, and Trivy). This is a narrow policy, not a
general path-based delete rule: each direct child is still bound to its reviewed object identity,
byte count, and modification time, and the active-use probe must be complete and idle. DiskSage
staging entries named `.disksage-trash-*` are excluded so a prior cleanup cannot become a recursive
probe target. The cache root is preserved, successful operations go to OS Trash, and a journal
entry is written. Any child in use is reported and left untouched.

The Cargo registry source tree (`~/.cargo/registry/src`) is catalogued as
`cargo-registry-source` for explicit review because it is regenerable but may require network
downloads to rebuild. It is intentionally excluded from the automatic six-cache action. During
the 2026-08-21 low-disk incident, no Cargo process was running; DiskSage development reclaimed the
observed 1.3 GiB source cache only after recording this boundary, while retaining the Cargo index,
package archives, git checkouts, all user files, and provider-managed data.

The observed `~/.cache/node`, `~/.cache/torch`, `~/.cache/prisma`, and `~/.cache/gh` trees are
catalogued as explicit manual-review targets for the same reason. Their paths are now stable
catalog identities, but they are deliberately excluded from `AUTO_REGENERABLE_CACHE_IDS`; the
automatic action remains limited to the six incident-approved roots until each tool's rebuild and
active-use contract is independently established.

The same incident later reached 289 MiB of APFS availability while a Finder/File Provider copy was
still preparing. A bounded read-only provider dump showed progress markers and stale `itemNotFound`
errors, so the operation remained blocked. DiskSage reclaimed only explicitly regenerable package/tool
caches (pnpm, npm's `_npx`/`_cacache`, and node/torch/prisma/gh caches), recovering roughly 1.6 GiB;
active uv/cargo processes and the Cargo registry source tree were not touched in this pass. No cache
was uploaded to iCloud, OneDrive, or Google Drive: reproducible build caches are a cleanup domain,
not user-file lineage, and sending them through a stalled provider would consume additional staging
space. The provider process, Finder copy, CloudDocs database, cloud objects, and user files remained
untouched. This observation is bound to source head `e71ecd13e8c91acf10093271fd58414cae5fe349`.

## Incident policy: proven cache Trash review

When the native macOS Trash contains the exact regenerable cache directories observed during this
incident, DiskSage may expose them as read-only candidates. The scanner accepts only known direct
names/signatures for npm, pnpm, Edge, uv, and Trivy caches, bounds traversal, rejects symlinks, and
binds the reviewed snapshot to each candidate root filesystem identity. Linux and Windows are not
silently treated as `~/.Trash`; the feature remains explicitly scoped to native macOS Trash until
those platforms have native enumeration contracts.

The desktop cleanup screen shows the proven cache entries and their observed bytes, but the backend
mints no destructive approval phrase while the final irreversible deletion primitive is not bound
to the exact reviewed object. The UI therefore does not render an in-app permanent-delete action;
it tells the operator to inspect and empty the macOS Trash manually when physical capacity must be
reclaimed. A direct purge attempt also returns
`cache-trash-identity-bound-permanent-delete-unavailable` and leaves the candidate intact. This
fail-closed state is intentional: checking an inode/device identity and then invoking
`remove_dir_all(path)` is not sufficient because a same-user pathname replacement can occur between
the check and the irreversible recursive deletion.

If a future implementation adds an exact-object-bound permanent-delete primitive, it must retain the
current candidate-set snapshot, root-identity binding, immediate revalidation, per-item journaling,
non-expansion of authority, and explicit operator confirmation before the UI can expose destructive
authority again.

## References

- [ADR-0001: Provider evidence drives the cloud-offload Goal](0001-cloud-offload-goal-state.md)
- `src-tauri/src/cache_cleanup.rs`
- `src-tauri/src/cache_trash_reclaim.rs`
- `src-tauri/src/rules.rs`
