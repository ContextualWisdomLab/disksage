# Extraction-free archive Git tree proof

## Problem

DiskSage held `/Users/seonghobae/Downloads/naruon-develop.zip` until its contents could be proved
reproducible from a public Git commit. A normal extraction comparison is unsafe and incomplete on
the current macOS volume because the archive contains both `.Jules/palette.md` and
`.jules/palette.md`. Those paths are distinct in Git but collide on a case-insensitive filesystem.

## Decision

`disksage-archive-tree` computes the Git-compatible logical tree represented by a ZIP without
extracting any entry. Its default mode preserves the original GitHub-source proof contract: one
shared transport wrapper is required and stripped. An explicit `--keep-top-level` mode supports
generic multi-root archives whose top-level paths are logical content. The Rust implementation:

1. Validates a bounded ZIP central directory and, by default, a single shared top-level directory.
2. Rejects absolute, parent-traversal, backslash, NUL, non-UTF-8, duplicate, encrypted, oversized,
   and Git-unrepresentable entries.
3. Streams each regular, executable, or symlink entry into the canonical Git blob SHA-1 framing.
4. Rebuilds nested Git tree objects with Git's byte ordering and mode rules.
5. Reports case-insensitive path collisions without merging or extracting them.
6. Optionally compares the resulting 40-hex tree SHA with an operator-supplied commit tree SHA and
   exits nonzero on mismatch.

The proof contains paths, counts, byte totals, modes, and object digests. It does not retain file
contents, call a network service, mutate the ZIP, or authorize deletion.

## Safety limits

- At most 100,000 ZIP entries.
- At most 4,096 bytes per path.
- At most 16 GiB declared uncompressed file bytes.
- More than 1,000 case-collision groups fails closed rather than truncating evidence.
- One shared wrapper directory remains mandatory by default, matching GitHub source archive
  structure. `--keep-top-level` must be explicit and preserves every validated path component.
- Unsupported compression or an observed-size mismatch fails closed.

## Cleanup gate

An exact tree match proves only that the ZIP's logical file bytes, paths, and Git-representable
modes match the comparison tree. It does not prove provenance or intended canonical status. Local
removal still requires a separate approval naming both compared inputs (or the ZIP, commit, and
remote repository), exact tree, reclaimable bytes, and Trash-only action. Remote reachability is
checked fresh before a Git-backed approval is applied.

## Integration decision

This is deterministic bounded hashing in Rust. No Noema, LLM, LLM-as-a-Judge, external model,
semantic catalog, database, or ontology integration is needed. If archive proof records later
become a persistent cross-device catalog, model that schema with `pg-erd-cloud` and publish it
through `semantic-data-portal` rather than embedding a database in this proof command.
