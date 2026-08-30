# ADR 0048: Generated cache recovery is exact-contract and activity bound

## Status

Accepted

## Decision

The process-path probe excludes both its child probe PID and the current DiskSage PID. Otherwise
the CLI's own `--root` argument would classify every inspected cache as active. External processes
whose handles or command paths enter the cache remain blockers.

DiskSage admits only exact cache roots with a named regeneration contract. It never infers safety
from a directory name, age, or size. Recursive open-process evidence, tool locks, and temporary Git
ownership evidence must be complete and inactive. Registered, dirty, or live temporary workspaces
are retained. Cloud-provider storage, Photos libraries, Podman/Colima storage, and Parallels virtual
machines are outside this deletion boundary.

The default result is a dry-run plan, exposed through `disksage-generated-cache-reclaim`.
Execution requires an unchanged content-and-activity fingerprint, the exact approval phrase,
reviewer attribution and rationale, bounded work, and a create-only mode-0600 JSON Lines journal.
The fingerprint binds relative names, entry type, filesystem identity, precise timestamps and
ownership metadata, and bounded file content. Execution accepts only a matching re-observation
collected after approval and no later than the attempt. Immediately before removal it repeats the
audit, atomically renames the exact tree into a private same-filesystem staging directory, then
repeats active-use and complete-manifest checks on that staged object. A failed check restores the
object and never follows a replacement at the approved pathname.
The journal durably records a complete `pending` event before mutation and appends a terminal
receipt afterward. A journal ending in `pending` requires reconciliation, a new observation, and a
new approval; it is never automatic retry authority. Provider data mutation remains false in every
event.

The CLI derives the canonical current-user home instead of accepting an authorization root. It
writes receipts only below DiskSage's fixed private application-data directory. The pending file
and its containing directory are synced before staging starts.

Temporary Git workspaces are audit-only here. Even a clean, inactive workspace is routed to the
existing Git-worktree or shared-temporary-artifact executor, which owns repository identity,
registration, journal, and rollback semantics. This generic cache executor never removes it.

## Consequences

Eligible caches can be reclaimed consistently, while an active tool, incomplete probe, lock,
workspace registration, dirty state, or protected product boundary keeps the bytes. Adding another
cache family requires a new explicit regeneration contract and tests. Customer-facing surfaces
translate evidence codes into a concrete next action (close the named tool, finish synchronization,
or use the specialized workspace review); they do not expose module or storage-engine boundaries.

## Rejected alternatives

Age, size, cache-like names, and LLM judgment alone are rejected because none proves that current
bytes are reproducible or inactive. Recursive deletion of `/private/tmp` and vendor data roots is
also rejected because those roots mix generated state with active work and customer data.

## Operational evidence

The acceptance fixtures reproduce the 2026-08-30 local audit: inactive Torch and Homebrew API or
bootsnap caches are eligible; active uv and Playwright trees are retained; a registered dirty Git
worktree is retained.

## References

Apple Inc. (n.d.). *File System Programming Guide*. Apple Developer Documentation.
https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/FileSystemProgrammingGuide/

Git Project. (n.d.). *git-worktree documentation*. https://git-scm.com/docs/git-worktree

Python Packaging Authority. (n.d.). *Caching*. uv documentation.
https://docs.astral.sh/uv/concepts/cache/
