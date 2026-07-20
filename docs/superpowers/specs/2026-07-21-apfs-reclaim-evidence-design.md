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

Nested selected roots are deduplicated and symbolic-link roots are rejected. The command never
moves, unlinks, or writes to supplied paths. APFS clone sharing is intentionally not inferred from
content equality or per-inode allocated blocks because those are not proof of unique extents or
physical reclaimability.

The GUI must label selection totals as logical size. Moving an item to Trash preserves its blocks;
actual physical recovery can only be claimed from a post-lifecycle filesystem free-space
observation after Trash is emptied or from an equally strong filesystem-native unique-extent proof.
