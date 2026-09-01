# ADR-0002: Cache cleanup is per-item active-use evidence bound

**Status:** Accepted
**Date:** 2026-08-20

## Context

Package-manager and tool caches can share one root while individual entries have different
lifecycle states. For example, `~/.cache/uv/archive-v0` can contain live MCP runtimes next to
reproducible, unused environments. A root-wide active-use observation either blocks safe cleanup
of unrelated entries or encourages an unsafe manual bypass. Cache contents are not user-file
lineage and must not be uploaded to a cloud provider merely to reclaim local space.

The irreversible proven-cache Trash purge has an additional authority problem: a preview-time flag
cannot authorize objects that were not shown to the operator. If execution re-discovers Trash
entries after approval, a newly appearing but structurally valid cache could be permanently deleted
without ever being reviewed.

## Decision

DiskSage exposes known cache roots through the existing cache catalog, including the macOS uv
cache. Cleanup uses the reviewed child manifest (`path`, byte count, modification time, and object
identity) and revalidates that manifest immediately before mutation. Active-use evidence is
collected independently for each reviewed child with bounded, path-local `lsof` evidence
(recursive for directories and direct for regular files):

- incomplete evidence or an active process leaves that child untouched and returns a stable blocker;
- an inactive child may be moved through DiskSage's identity-bound OS-Trash path;
- the cache root and all unrelated children remain untouched;
- the operation is journaled; the normal path never permanently deletes cache content; and
- a separate, explicit `--purge-proven-cache-trash` path may permanently remove only exact reviewed
  direct OS-Trash children. Its dry-run candidate record binds known cache name, exact direct-child
  path, byte count, modification time, filesystem object identity, and structural signature. The
  operator must persist and review that exact candidate array and pass it back through
  `--approved-cache-trash-candidates PATH` when executing. Every approved candidate is validated
  before the first irreversible mutation and again immediately before its own deletion. A newly
  appearing candidate is ignored; a moved, replaced, resized, modified, symlinked, structurally
  changed, duplicate, nested, or otherwise stale approved candidate fails closed. Each attempted
  deletion receives pending and terminal journal records. No arbitrary Trash entry, cloud
  placeholder, or user-file candidate qualifies.

This per-item probe is the authoritative cleanup boundary. A live process elsewhere under the
same cache root must not prevent reclaiming an independently inactive entry, and it must never be
treated as evidence that the inactive entry is safe without its own probe.

For the irreversible Trash exception, the reviewed candidate manifest—not the command flags by
themselves—is the mutation authority. Flags choose the operation; the manifest determines the exact
objects that may cross the irreversible boundary.

## Consequences

- A user can clean inactive uv archive entries while active MCP/uv runtimes continue running.
- Changed, replaced, symlinked, or unreadable entries fail closed before they reach the OS Trash.
- The normal operation is reversible through the OS Trash; physical space is not claimed until the
  user empties that Trash, and APFS shared blocks may make physical reclaim smaller than logical
  size. The explicit proven-cache purge is irreversible by design and is limited to exact reviewed
  cache data already placed in Trash.
- A proven cache that appears after review remains in Trash until a later preview and explicit
  approval; execution never widens the reviewed candidate set by rescanning.
- If any reviewed purge candidate becomes stale before the first deletion, the batch fails before
  mutation. Each candidate is revalidated again immediately before its own delete to narrow the
  remaining replacement-race window.
- Cache cleanup does not create cloud-copy receipts, provider-sync evidence, or source-eviction
  permits. User files still require the cloud-offload ADR and its provider evidence gates.

## Alternatives rejected

- **Root-wide active-use probe:** safe but unnecessarily blocks unrelated inactive entries.
- **Direct recursive deletion of live cache roots:** not reversible and cannot prove per-entry
  identity at mutation time. Permanent deletion is allowed only for an exact reviewed,
  structurally proven cache already in OS Trash through the separate explicit path.
- **Execution-time rescan as approval:** rejected because a newly appearing candidate was not shown
  to the operator and therefore has no human-attributed exact approval.
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

## Incident policy: proven cache Trash purge

When the OS Trash itself contains the exact regenerable cache directories observed during this
incident, DiskSage may expose them as read-only candidates. The preview records each accepted
candidate's exact direct-child path, known name, byte count, modification time, filesystem object
identity, and structural signature. The operator reviews and persists that exact candidate array.
Permanent removal then requires all three controls:

1. `--execute --purge-proven-cache-trash` selects the irreversible operation;
2. `--approved-cache-trash-candidates /ABSOLUTE/reviewed.json` supplies the exact reviewed candidate
   set; and
3. execution proves the entire approved set is still current before the first deletion, then
   revalidates each candidate immediately before its own removal.

The candidate scanner accepts only the known direct names/signatures for npm, pnpm, Edge, uv, and
Trivy caches; it bounds traversal and rejects symlinks. Execution never rescans to widen authority:
a matching cache that appears after preview remains untouched until a later review. Stale, nested,
replaced, resized, modified, moved, duplicate, or structurally changed approved candidates fail
closed. Each attempted removal writes both a pending journal record and a terminal `ok`/`error`
record. This path never empties Trash generally and never applies to user files or cloud-provider
placeholders.

## References

- [ADR-0001: Provider evidence drives the cloud-offload Goal](0001-cloud-offload-goal-state.md)
- `src-tauri/src/cache_cleanup.rs`
- `src-tauri/src/bin/disksage-cache-cleanup.rs`
- `src-tauri/src/rules.rs`
