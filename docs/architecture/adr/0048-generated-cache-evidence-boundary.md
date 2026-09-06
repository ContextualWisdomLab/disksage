# ADR 0048: Generated cache recovery is exact-contract and activity bound

## Status

Proposed. This ADR is not present on protected `main`; owner integration, current-head checks,
and protected prerequisite adoption remain outstanding.

## Context and decision drivers

Generated output can contain resumable work, and a successful rename does not revoke an existing
directory descriptor. A synthetic native cleanup reproduced late nested-session deletion through
a descriptor to the cache's parent after target-only activity checks passed. The decision must
preserve session state, keep exact regeneration boundaries, reject incomplete evidence, and avoid
claiming that a shared cache parent represents exclusive application ownership.

## Decision

The process-path probe excludes both its child probe PID and the current DiskSage PID. Otherwise
the CLI's own `--root` argument would classify every inspected cache as active. External processes
whose handles or command paths enter the cache remain blockers.

DiskSage admits only exact cache roots with a named regeneration contract. It never infers safety
from a directory name, age, or size. Recursive open-process evidence, tool locks, and temporary Git
ownership evidence must be complete and inactive. Registered, dirty, or live temporary workspaces
are retained. Cloud-provider storage, Photos libraries, Podman/Colima storage, and Parallels virtual
machines are outside this deletion boundary.

In the context of exact generated-cache cleanup, facing writers retaining parent-directory
handles across staging, we decided for an identity-bound immediate-parent handle veto and against
target-only checks or labeling a recursive shared-parent scan as exclusive ownership evidence,
to achieve rejection of the reproduced parent-descriptor case, accepting another observation
cost and explicit limits for sibling, ancestor, and later-arriving writers.

The proposal binds the canonical immediate-parent path and filesystem identity into the plan and
approval fingerprint. Parent activity is checked on that exact directory object, without recursive
sibling traversal, before and after the audit and at both staged checkpoints. Parent replacement,
active handles, and incomplete evidence retain the candidate. Target-tree checks and original-path
command association remain required. This additional veto does not establish a complete owning
activity scope or replace provider-native locking.

The default result is a dry-run plan, exposed through `disksage-generated-cache-reclaim`.
Execution requires an unchanged content-and-activity fingerprint, the exact approval phrase,
reviewer attribution and rationale, bounded work, and a create-only mode-0600 JSON Lines journal.
The fingerprint binds relative names, entry type, filesystem identity, precise timestamps and
ownership metadata, and bounded file content. Execution accepts only a matching re-observation
collected after approval and no later than the attempt. Immediately before removal it repeats the
audit, atomically renames the exact tree into a private same-filesystem staging directory, then
repeats active-use and complete-manifest checks on that staged object. Failure restoration must
preserve any newly occupied original pathname. Shared atomic no-replace restoration is a pending
protected prerequisite; the current check-then-rename implementation is not evidence that this
requirement has been satisfied.
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

The parent veto can retain an otherwise reproducible cache when its parent is busy or cannot be
observed completely. It does not exclude a writer acquiring access after the final observation.
An owning-provider concurrency contract and the pending atomic restoration remain necessary
acceptance work; passing the focused parent test does not authorize a universal zero-error claim.

## Rejected alternatives

Age, size, cache-like names, and LLM judgment alone are rejected because none proves that current
bytes are reproducible or inactive. Recursive deletion of `/private/tmp` and vendor data roots is
also rejected because those roots mix generated state with active work and customer data.

A recursive scan of a shared `.cache` or `Library/Caches` parent is not adopted as an ownership
proof: it captures unrelated applications and still cannot exclude access from ancestors or new
writers. The exact parent-object check is an additional veto with a narrower stated purpose.

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
