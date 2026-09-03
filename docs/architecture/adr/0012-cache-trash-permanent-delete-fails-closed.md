# ADR-0012: Cache Trash permanent deletion fails closed

**Status:** Proposed
**Date:** 2026-09-03
**Supersedes:** ADR-0002 only for permanent deletion of cache entries already in OS Trash

## Context

ADR-0002 allowed a separate `--execute --purge-proven-cache-trash` path to permanently remove
structurally recognized cache directories from operating-system Trash after pathname-based
revalidation. Subsequent implementation review showed that the final irreversible deletion syscall
was not bound to the exact reviewed filesystem object. A pathname can be replaced after review and
before recursive removal, so the earlier policy could not satisfy DiskSage's deletion-safety
boundary even when the candidate name, structure, size, and symlink checks were repeated.

DiskSage already has a reversible, identity-bound cleanup path that moves inactive regenerable cache
children into OS Trash. Permanent removal is different: once Trash is bypassed there is no product
undo boundary, so evidence that is sufficient for staging is not sufficient for irreversible
deletion.

## Decision

DiskSage does not perform in-app permanent deletion of reviewed cache-Trash entries until the final
irreversible filesystem operation can be bound to the exact object that was reviewed and approved.

- `--purge-proven-cache-trash` remains a read-only evidence operation.
- `--execute --purge-proven-cache-trash` returns
  `cache-trash-identity-bound-permanent-delete-unavailable` before journal or filesystem mutation.
- The library boundary also fails closed and does not call pathname-recursive permanent-deletion
  primitives.
- Candidate names, signatures, byte counts, and approval phrases are review evidence only; they do
  not create irreversible mutation authority.
- Operators who intend permanent reclaim must inspect the candidate evidence and empty the native
  Trash manually through the operating system. DiskSage does not claim those bytes as physically
  reclaimed until the operating system reports the resulting availability change.
- User files, cloud-provider placeholders, and arbitrary Trash entries remain outside this cache
  evidence path.

This decision leaves ADR-0002's per-item active-use checks and reversible OS-Trash staging intact.
Only its separate permanent-delete authorization is superseded.

## Consequences

- The CLI and desktop remain conservative under disk pressure: they can identify regenerable cache
  material but cannot silently turn that evidence into an irreversible delete.
- Automation receives a stable refusal code rather than a partial journal or ambiguous success
  receipt.
- Physical space recovery may require an explicit operating-system Trash action after DiskSage has
  completed its reversible cleanup.
- A future permanent-delete capability requires a new or superseding ADR, a real object-bound
  deletion primitive for each supported platform, race/alias/mount/hardlink tests, recovery and
  audit semantics, and current-head release evidence before it can become Accepted.

## Alternatives rejected

- **Keep pathname revalidation plus recursive deletion.** Rejected because repeated pathname checks
  do not bind the final syscall to the reviewed object and leave a check/use race at an irreversible
  boundary.
- **Treat a candidate-set approval phrase as delete authority.** Rejected because a phrase proves
  what the user reviewed, not that the pathname still names the same filesystem object at mutation
  time.
- **Delete first and rely on the journal for recovery.** Rejected because a journal cannot restore an
  object after a genuinely permanent delete and journal failure can itself occur after mutation.
- **Broaden automatic cleanup instead of using Trash.** Rejected because reversible OS-Trash staging
  is the product's established safety and recovery boundary for regenerable cache content.

## Evidence and acceptance

The production CLI regression creates a real cache-shaped directory under a temporary Trash,
invokes `--execute --purge-proven-cache-trash`, and requires the refusal code while proving both the
cache object and journal remain untouched. Documentation contract coverage requires the runbook and
ADR index to describe the same fail-closed behavior.

This ADR remains Proposed while the implementing PR is unmerged. Acceptance requires an unchanged
exact head with the repository's required tests, security gates, coverage, review, and release
verification all passing under live protection rules.

## References

- [ADR-0002: Cache cleanup is per-item active-use evidence bound](0002-cache-cleanup-is-per-item-evidence-bound.md)
- `src-tauri/src/bin/disksage-cache-cleanup.rs`
- `src-tauri/src/cache_cleanup.rs`
- `src-tauri/tests/cache_cleanup_cli_purge_fail_closed.rs`
- [Cache cleanup operator runbook](../../development/cache-cleanup-operator-runbook.md)
