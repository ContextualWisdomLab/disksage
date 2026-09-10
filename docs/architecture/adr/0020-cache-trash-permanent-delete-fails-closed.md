# ADR-0020: Cache Trash permanent deletion fails closed

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

## Platform capability assessment

The refusal is a product invariant, not a claim that every supported operating system has identical
filesystem primitives. The platform adapter must expose capability separately from policy and must
not silently fall back from an object-bound implementation to pathname recursion.

### Windows

Windows has a plausible object-bound implementation path that is materially stronger than the
withdrawn pathname deletion design. `FILE_ID_INFO` combines the volume serial number and 128-bit file
identifier and Microsoft documents that the pair can be used to determine whether two open handles
represent the same file. `SetFileInformationByHandle` applies rename and disposition operations to
an already-open handle, and `FILE_DISPOSITION_INFO` requires a handle opened with `DELETE` access.

A future Windows adapter may therefore be evaluated around this sequence:

1. open the reviewed cache object without traversing a reparse point and retain the handle;
2. compare its `FILE_ID_INFO` to the reviewed snapshot before creating mutation authority;
3. if staging is required, rename through the same handle into a DiskSage-private recovery location;
4. enumerate directories without following reparse points and bind each descendant to its own open
   handle and identity evidence before irreversible removal;
5. mark only those same verified handles for deletion, with durable pending/terminal receipts and
   crash recovery that cannot repeat a completed delete.

This is an implementation candidate, not authorization. It requires real NTFS/ReFS filesystem tests
covering replacement races, reparse points, hard links, open/mapped files, ACL failures, partial
recursive progress, reboot/close semantics, and recovery before this ADR can be superseded.

### Linux

Linux `openat2(2)` can constrain untrusted path resolution with `RESOLVE_BENEATH`,
`RESOLVE_NO_SYMLINKS`, `RESOLVE_NO_MAGICLINKS`, and `RESOLVE_NO_XDEV`; the kernel can fail resolution
when it cannot safely prove the requested boundary. Those controls are useful for inspection and
bounded traversal.

They do not by themselves solve this ADR's final-object requirement. POSIX `unlinkat()` removes a
directory entry selected by a name relative to a directory file descriptor. A directory FD binds the
parent directory, but the final entry is still selected by pathname at the unlink operation. DiskSage
therefore does not treat `openat2` plus `unlinkat` as proof that the final irreversible mutation is
bound to the exact previously reviewed cache object.

### macOS and portable POSIX boundary

Portable `openat()`, `renameat()`, and `unlinkat()` reduce races caused by replacing ancestor path
components because lookup is relative to already-open directory descriptors. They still select the
final directory entry by name rather than expressing an irreversible delete of a previously opened
object handle. DiskSage therefore does not claim portable POSIX APIs alone satisfy the reviewed-object
invariant for recursive cache-directory deletion on macOS.

An OS-specific stronger primitive or a separately reviewed reversible quarantine protocol may change
that conclusion. A quarantine-first design is not accepted merely because `renameat()` is atomic:
if an attacker replaces the candidate immediately before the rename, the wrong object can still be
moved. Any such design must prove mismatch recovery, private-quarantine integrity, mount and alias
behavior, and that no mismatched object is irreversibly deleted.

## Consequences

- The CLI and desktop remain conservative under disk pressure: they can identify regenerable cache
  material but cannot silently turn that evidence into an irreversible delete.
- Automation receives a stable refusal code rather than a partial journal or ambiguous success
  receipt.
- Physical space recovery may require an explicit operating-system Trash action after DiskSage has
  completed its reversible cleanup.
- Platform capability is explicit. A future Windows implementation can advance independently behind
  a capability boundary without authorizing a weaker Linux/macOS fallback.
- A future permanent-delete capability requires a new or superseding ADR, a real object-bound
  deletion primitive for each enabled platform, race/alias/mount/hardlink tests, recovery and audit
  semantics, and current-head release evidence before it can become Accepted.

## Alternatives rejected or deferred

- **Keep pathname revalidation plus recursive deletion.** Rejected because repeated pathname checks
  do not bind the final syscall to the reviewed object and leave a check/use race at an irreversible
  boundary.
- **Treat directory-relative `unlinkat()` as target identity binding.** Rejected because the parent
  directory is descriptor-bound while the final entry remains name-selected.
- **Treat a candidate-set approval phrase as delete authority.** Rejected because a phrase proves
  what the user reviewed, not that the pathname still names the same filesystem object at mutation
  time.
- **Rename to a private quarantine and immediately delete.** Deferred. Rename can make later deletion
  recoverable and easier to isolate, but name-based rename can itself race with replacement. It needs
  separate mismatch-restoration and private-quarantine proofs before it can create delete authority.
- **Delete first and rely on the journal for recovery.** Rejected because a journal cannot restore an
  object after a genuinely permanent delete and journal failure can itself occur after mutation.
- **Broaden automatic cleanup instead of using Trash.** Rejected because reversible OS-Trash staging
  is the product's established safety and recovery boundary for regenerable cache content.

## Evidence and acceptance

The production CLI regression creates a real cache-shaped directory under a temporary Trash,
invokes `--execute --purge-proven-cache-trash`, and requires the refusal code while proving both the
cache object and journal remain untouched. Documentation contract coverage requires the runbook and
ADR index to describe the same fail-closed behavior.

A superseding implementation must add platform-specific real-filesystem acceptance. At minimum, the
suite must cover pathname replacement between review and execution, symlink/reparse substitution,
hard links, mount/volume boundaries, permission changes, concurrent rename/delete, partial recursive
failure, crash recovery, idempotent rerun, and a proof that a mismatched object is never permanently
removed. Windows enablement additionally requires same-handle identity and delete evidence; Linux
and macOS remain unavailable until an equivalent object-bound final mutation or an independently
accepted recovery protocol exists.

This ADR remains Proposed while the implementing PR is unmerged. Acceptance requires an unchanged
exact head with the repository's required tests, security gates, coverage, review, and release
verification all passing under live protection rules.

## References

- [ADR-0002: Cache cleanup is per-item active-use evidence bound](0002-cache-cleanup-is-per-item-evidence-bound.md)
- `src-tauri/src/bin/disksage-cache-cleanup.rs`
- `src-tauri/src/cache_cleanup.rs`
- `src-tauri/tests/cache_cleanup_cli_purge_fail_closed.rs`
- [Cache cleanup operator runbook](../../development/cache-cleanup-operator-runbook.md)
- Linux man-pages project. (2026). *openat2(2) — Linux manual page*. https://man7.org/linux/man-pages/man2/openat2.2.html
- Microsoft. (2024, February 22). *FILE_DISPOSITION_INFO structure (winbase.h)*. Microsoft Learn. https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_disposition_info
- Microsoft. (2024, February 22). *FILE_ID_INFO structure (winbase.h)*. Microsoft Learn. https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_info
- Microsoft. (n.d.). *SetFileInformationByHandle function (fileapi.h)*. Microsoft Learn. https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setfileinformationbyhandle
- The Open Group. (2024). *open, openat — open file*. The Open Group Base Specifications Issue 8, POSIX.1-2024. https://pubs.opengroup.org/onlinepubs/9799919799/functions/open.html
- The Open Group. (2024). *rename, renameat — rename file*. The Open Group Base Specifications Issue 8, POSIX.1-2024. https://pubs.opengroup.org/onlinepubs/9799919799/functions/rename.html
- The Open Group. (2018). *unlink, unlinkat — remove a directory entry*. The Open Group Base Specifications Issue 7, 2018 edition. https://pubs.opengroup.org/onlinepubs/9699919799/functions/unlink.html
