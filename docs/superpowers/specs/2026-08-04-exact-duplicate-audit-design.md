# Exact duplicate audit evidence

## Problem

The interactive duplicate browser groups files efficiently, but it silently omits traversal and
hash failures and does not bind pre/post-hash stability evidence. That behavior is useful for a UI,
but it cannot support a disk-reclamation decision where incomplete evidence must fail closed.

## Interface

`disksage-duplicate-audit --root ABSOLUTE_PATH [--min-bytes N] [--max-entries N]
[--private-output ABSOLUTE_NEW_FILE.json]` performs a bounded, read-only recursive audit.

- Size grouping is only a prefilter. Exact matches require streaming BLAKE3, SHA-256, and QuickXor
  agreement over stable full-file bytes.
- Every hashed file is checked before opening, on the opened file descriptor, and again after the
  hash and metadata probe. Unix builds also bind device and inode identity.
- Canonical-review evidence probes embedded content metadata and assigns the provisional
  production date in this strict order: embedded metadata, explicit filename date, filesystem
  creation time, then filesystem modification time. The private report retains the selected
  source, confidence, title, authors, context, duration, and underlying embedded evidence.
- Symlinks are not followed. Traversal, metadata, stability, hash, depth, and entry-limit failures
  make the overall evidence incomplete.
- The public summary contains only aggregate counts, bytes, issues, and fingerprints. The optional
  private report retains relative paths and digests in a create-new file with mode `0600` on Unix.
- Public production-date reporting is limited to generic source counts; raw metadata values remain
  private.

## Safety boundary

Exact byte identity does not prove identical lineage context and does not choose a canonical copy.
Every cluster requires private metadata review and human canonical selection. The audit cannot
delete, rename, move, or create an approval; all automatic-delete and mutation claims remain false.
`logical_redundant_bytes` is not reported as physically reclaimable space: hard links and APFS
clone/shared-block allocation require a separate physical-storage proof, so
`physical_reclaimable_bytes` remains null.

## Verification

Tests cover exact full-content grouping, equal-size nonmatches, entry-limit fail-closed behavior,
symlink exclusion, production-date precedence, tamper rejection, path-redacted summaries, and
explicit non-approval/non-mutation claims. Release artifacts must include the CLI and its SHA-256
sidecar on macOS, Linux, and Windows.
