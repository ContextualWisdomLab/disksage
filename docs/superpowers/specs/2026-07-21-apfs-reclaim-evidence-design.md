# APFS-aware reclaim evidence

DiskSage must not present file length, `du`, or allocated block totals as bytes that a cleanup will
physically free. Hard links, APFS copy-on-write clones, sparse allocation, compression, snapshots,
and Trash retention can make the result smaller. The July 21 Naruon cleanup proved the gap: four
ignored `node_modules` trees accounted for about 4.0 GiB per-path but deleting them increased APFS
free blocks by only about 66.8 MiB.

`disksage-reclaim-plan` is a Rust, read-only evidence command. For each supplied file or directory it
reports:

- the stable `schema_kind: disksage.reclaim-plan` discriminator and `schema_version: 1`;
- logical selected bytes;
- observed allocated bytes, with observable Unix hard-link identities deduplicated;
- `physically_reclaimable_bytes: null` and `status: unverified` before the operation;
- stable reason codes explaining shared-extent uncertainty and Trash retention.

When an operator needs to distinguish an idle cache from a build or editor tree that is currently
in use, the command accepts `--check-active-use`. This opt-in adds bounded, path-local `lsof`
evidence per normalized root (`evidence_complete`, `active`, and a capped PID list). Regular-file
roots use an exact-file `lsof` query (`lsof-file-pid`), while directory roots use recursive
`lsof` (`lsof-recursive-pid`). The probe is diagnostic only and never treats an idle result as
permission to delete. The default output omits this optional field for compatibility and to avoid
the extra process/file scan.

Nested selected roots are deduplicated and symbolic-link roots are rejected. The command never
moves, unlinks, or writes to supplied paths. APFS clone sharing is intentionally not inferred from
content equality or per-inode allocated blocks because those are not proof of unique extents or
physical reclaimability. The JSON interchange contract permits at most 1,000 normalized roots and
4,096 UTF-8 bytes per canonical path. Non-UTF-8 paths, control-character paths, and directory roots
that disappear before a directory entry can be observed fail closed. Filtered symbolic links and
reparse points are included in `skipped` rather than silently disappearing from the evidence.

The GUI must label selection totals as logical size. Moving an item to Trash preserves its blocks;
actual physical recovery can only be claimed from a post-lifecycle filesystem free-space
observation after Trash is emptied or from an equally strong filesystem-native unique-extent proof.
The progress summary therefore keeps the global available-space change in a separate field from
`action_attributable_bytes`, which sums only unique completed action receipts. The two values are
never substituted for one another or silently reconciled.

For example, a read-only cache review can be run with:

```sh
cargo run --locked --manifest-path src-tauri/Cargo.toml \
  --bin disksage-reclaim-plan -- \
  --operation trash --check-active-use \
  "$HOME/Library/Caches/codec-carver" \
  "$HOME/Library/Caches/trivy"
```
