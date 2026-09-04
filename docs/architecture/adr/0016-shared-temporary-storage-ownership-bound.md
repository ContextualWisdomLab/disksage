# ADR-0016: Bound shared temporary storage cleanup to ownership evidence

**Status:** Accepted
**Date:** 2026-08-28

## Context

The current low-disk incident includes space under the shared temporary directory. On macOS,
`/tmp` is a symlink to `/private/tmp`; on other Unix platforms the shared path is `/tmp`. The
existing catalog only exposed the process-specific temporary directory, so a user could not review
the shared temporary bytes through DiskSage. A shared directory also contains objects belonging to
other users and system services, so path presence or modification time is not deletion authority.

## Decision

Add a `shared-temp` inspection entry for the platform's real shared temporary root when it is not
already the process temporary directory. A direct child becomes a cleanup candidate only when:

- the root is a real directory and the child is not a symbolic link;
- every object in the child tree is owned by the current effective user and is readable for the
  bounded ownership walk;
- per-item active-use evidence is complete and idle; and
- the existing filesystem identity, size, recheck, journal, and OS-Trash gates succeed.

The shared root itself, foreign/system-owned trees, linked objects, and incomplete ownership walks
remain protected. The candidate's displayed bytes are the sum of the ownership-qualified children,
not an estimate for the whole shared directory. No age threshold or quality heuristic is used.

## Consequences

- `/tmp`/`/private/tmp` space is visible as a separate reclaim domain and can be reclaimed through
  the reversible Trash path when evidence is complete.
- A system or another user's temporary object cannot be selected by this catalog, even when it is
  large or old.
- Ownership traversal adds bounded inspection work; over-limit or unreadable trees remain visible
  only as unresolved shared temporary space.

## Alternatives rejected

- **Expose only the process temporary directory:** hides the incident's shared temporary bytes.
- **Allow every child under `/tmp`:** grants a shared system directory deletion authority.
- **Delete by age or filename:** uses a heuristic without proving ownership or active use.
- **Permanently delete temporary entries:** bypasses the existing reversible, journaled Trash path.

## References

- [ADR-0002: Cache cleanup is per-item active-use evidence bound](0002-cache-cleanup-is-per-item-evidence-bound.md)
- `src-tauri/src/rules.rs`
- `src-tauri/src/safety.rs`
