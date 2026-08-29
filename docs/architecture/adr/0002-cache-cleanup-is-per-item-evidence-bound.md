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
- normal cache cleanup is journaled and remains reversible through the OS Trash, including npm,
  pip, and Corepack directory children; it does not grant an irreversible-delete authority merely
  because a native manager can regenerate the cache;
- a separate, explicit `--purge-proven-cache-trash` path may permanently remove only direct
  OS-Trash children whose exact known cache name and structural signature are revalidated, whose
  bounded tree contains no symlink, and whose deletion is journaled as pending/ok/error. No
  arbitrary Trash entry, cloud placeholder, or user-file candidate qualifies.
- the headless `--cache-id ... --permanent-cache` path may permanently remove only inactive,
  unchanged direct children of the four catalogued Gradle regeneration roots (`caches`, wrapper
  distributions, toolchain JDKs, and daemon records). Project files, Maven local artifacts, Gradle
  configuration, and every non-Gradle catalog ID remain outside that irreversible exception.

This per-item probe is the authoritative cleanup boundary. A live process elsewhere under the
same cache root must not prevent reclaiming an independently inactive entry, and it must never be
treated as evidence that the inactive entry is safe without its own probe.

## Consequences

- A user can clean inactive uv archive entries while active MCP/uv runtimes continue running.
- Changed, replaced, symlinked, or unreadable entries fail closed before they reach the OS Trash.
- Normal cache cleanup is reversible through the OS Trash. Permanent cache deletion exists only in
  the explicit proven-cache Trash purge, after the object is already in Trash and its known
  structure is revalidated.
- Cache cleanup does not create cloud-copy receipts, provider-sync evidence, or source-eviction
  permits. User files still require the cloud-offload ADR and its provider evidence gates.

## Alternatives rejected

- **Root-wide active-use probe:** safe but unnecessarily blocks unrelated inactive entries.
- **Direct recursive deletion of live cache roots:** not reversible and creates unnecessary
  irreversible authority. Normal active-cache cleanup therefore uses the OS Trash; permanent
  removal is confined to the explicit, structurally proven cache-data purge after Trash staging.
- **Copying caches to iCloud/OneDrive/Google Drive:** wastes cloud capacity for reproducible data and
  conflates cache cleanup with user-file lineage.

## Incident policy: observed macOS regenerable caches

When provider upload is blocked and local pressure is high, DiskSage may run the
`clean_regenerable_caches` command without a second approval prompt for the observed regenerable
macOS roots (npm, pip, Corepack/Node.js, uv, pnpm, Adobe, Microsoft Edge, Trivy, AppMap, Superset,
and Playwright). This is a narrow policy, not a
general path-based delete rule: each direct child is still bound to its reviewed object identity,
byte count, and modification time, and the active-use probe must be complete and idle. DiskSage
staging entries named `.disksage-trash-*` are excluded so a prior cleanup cannot become a recursive
probe target. The cache root is preserved, every successful normal cleanup goes through the
journaled OS-Trash path (including npm, pip, and Corepack directories), and any child in use is
reported and left untouched.

The Cargo registry source tree (`~/.cargo/registry/src`) is catalogued as
`cargo-registry-source` for explicit review because it is regenerable but may require network
downloads to rebuild. It is intentionally excluded from the automatic six-cache action. During
the 2026-08-21 low-disk incident, no Cargo process was running; DiskSage development reclaimed the
observed 1.3 GiB source cache only after recording this boundary, while retaining the Cargo index,
package archives, git checkouts, all user files, and provider-managed data.

The observed `~/.cache/torch`, `~/.cache/prisma`, and `~/.cache/gh` trees are
catalogued as explicit manual-review targets for the same reason. Their paths are now stable
catalog identities, but they are deliberately excluded from `AUTO_REGENERABLE_CACHE_IDS`; the
automatic action remains limited to the incident-approved roots until each tool's rebuild and
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
incident, DiskSage may expose them as read-only candidates and permanently remove them only when
the operator passes `--execute --purge-proven-cache-trash`. The candidate scanner accepts only the
known direct names/signatures for npm, pnpm, Edge, uv, and Trivy caches; it bounds traversal,
rejects symlinks, rechecks the signature immediately before removal, and writes a journal record
for both the pending and terminal outcome. This path never empties the Trash generally and never
applies to user files or cloud-provider placeholders.

## References

- [ADR-0001: Provider evidence drives the cloud-offload Goal](0001-cloud-offload-goal-state.md)
- `src-tauri/src/cache_cleanup.rs`
- `src-tauri/src/rules.rs`
