# macOS current-user temporary-directory guard

## Decision

DiskSage continues to protect `/private` by default. A single narrow exception permits only strict
descendants of the canonical temporary root returned for the current process. The temporary root
itself, `/private/var/folders`, sibling user trees, and every other `/private` path remain protected.

## Security properties

1. The exception is evaluated only on normalized paths. Callers canonicalize existing targets or
   canonicalize the nearest existing ancestor before appending a missing suffix.
2. The current process temporary root must canonicalize under `/private/var/folders`; otherwise the
   exception fails closed.
3. Equality with the temporary root is rejected. Only `path.starts_with(temp_root)` strict
   descendants are allowed.
4. A symlink inside the allowed tree cannot authorize a protected destination because its target is
   canonicalized before the guard runs.
5. This change does not add deletion primitives. DiskSage still routes destructive cleanup through
   the operating-system Trash and the existing journal.

## Verification

macOS-only regression tests cover the platform root, the current temporary root, a strict current
user descendant, an unrelated sibling user tree, and a symlink whose canonical target is `/System`.
Linux and Windows behavior is unchanged at compile time.
