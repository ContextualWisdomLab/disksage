# ADR 0012: Separate photo-duplicate and keeper evidence without composite scoring

- Status: Proposed
- Date: 2026-08-29

## Context

Pixel dimensions, bit depth, codec losslessness, edit lineage, metadata completeness, perceptual
similarity, and perceptual quality answer different questions. Combining them with undocumented
weights would turn descriptive evidence into deletion authority. Pixel count is also not a
validated substitute for spatial-frequency response or perceptual quality. A perceptual hash
distance requires a descriptor-specific, population-calibrated decision threshold; BRISQUE
requires the trained model used by the published method.

Photos libraries and File Provider trees have additional ownership and materialization semantics.
Reading package internals or a dataless placeholder can trigger provider activity and cannot
authorize cleanup. Encoded files and aggregate audit batches also need explicit work budgets before
any content allocation so hostile inputs cannot convert an audit into an availability failure.

## Decision

DiskSage records byte identity with BLAKE3 and groups currently materialized PNG files only when
raster dimensions and normalized decoded RGBA16 pixels have the same domain-separated digest.
The decoder expands palette/low-depth samples and `tRNS` transparency before normalization, while
source bit depth remains separate keeper evidence. This permits semantically identical lossless
encodings with different compression or ancillary metadata to share a group without a
perceptual-distance threshold, while preventing different raster shapes or transparency semantics
from collapsing into one exact identity.

Before content allocation, the audit rejects symlinks, provider-managed paths, Photos library
packages, dataless objects, unsupported codecs, and encoded inputs above the fixed per-file budget.
The already-open handle is read through the same byte ceiling and filesystem-object identity,
length, and modification evidence are rechecked around hashing. Aggregate input count and declared
bytes are also bounded. Active-use evidence is collected fail-closed only by a fresh execution
preflight; read-only audit does not turn platform-specific process inspection into a discovery
prerequisite.

The public wire contract is `disksage.photo-duplicate-audit.v2`. Version 2 makes the evidence-state
serialization and keeper metadata semantics explicit and changes exact grouping to
`decoded-pixel-rgba16-raster-exact-v2`; consumers must not interpret it as the earlier draft v1
shape.

The audit reports dimensions, source bit depth, losslessness, metadata-field count, lineage
availability, no-reference IQA availability, and perceptual-descriptor availability as separate
evidence. It does not calculate a composite score. Perceptual grouping remains unavailable until a
versioned descriptor and dataset-calibrated threshold artifact include provenance and a
cryptographic checksum. BRISQUE remains unavailable until its exact trained model artifact has
equivalent provenance and checksum.

Keeper evidence uses Pareto dominance only: losslessness, source bit depth, metadata completeness,
and original/edit lineage must all be no worse and at least one must be better for exactly one
member. File size, modification time, and filename have no quality authority. Ties and incomparable
members require customer selection; byte-identical members therefore never receive an arbitrary
automatic keeper. The audit may display a unique Pareto keeper but does not mutate files. Cleanup
execution and permanent deletion remain unavailable. A later execution decision must bind an exact
group identity, exact unique keeper identity, fresh approval, current inactive/materialized
evidence, reversible Trash or quarantine, and a durable journal with undo.

This ADR remains Proposed until the exact PR head passes the applicable Test/Release/Security/SAST
checks and all valid review findings are resolved. Merge is the acceptance boundary.

## Consequences

Customers can establish bounded exact-duplicate evidence without intentionally hydrating Photos or
cloud-provider data. They are told why near-duplicate grouping and cleanup are unavailable and what
evidence must be installed next. This initial capability deliberately recovers no bytes by itself.
Automation receives a non-zero CLI exit when the supplied audit is incomplete while retaining the
structured JSON rejection evidence.

## Rejected alternatives

- A hand-tuned weighted “quality score”: rejected because its weights and construct validity are
  unsupported.
- A fixed perceptual-hash distance copied from examples: rejected because it is not calibrated to
  the operating image population.
- Selecting the largest image automatically: rejected because dimensions alone do not establish
  fidelity, originality, or perceptual quality.
- Substring matching for cloud-provider brands: rejected because it both misses canonical managed
  roots and rejects ordinary local names; DiskSage reuses the filesystem/platform classifiers
  owned by the cloud bounded context.
- Unbounded `read_to_end`: rejected because decode limits do not protect the preceding encoded-file
  allocation.
- Direct Photos library mutation or permanent deletion: rejected because it bypasses provider and
  reversible-recovery contracts.

## References

Camera & Imaging Products Association. (2026). *Exchangeable image file format for digital still
camera: Exif Version 3.1 (CIPA DC-008-Translation-2026)*.
https://www.cipa.jp/e/std/std-sec.html

International Organization for Standardization. (2024). *Photography—Electronic still picture
imaging—Resolution and spatial frequency responses (ISO 12233:2024)*.
https://www.iso.org/standard/88626.html

Mittal, A., Moorthy, A. K., & Bovik, A. C. (2012). No-reference image quality assessment in the
spatial domain. *IEEE Transactions on Image Processing, 21*(12), 4695–4708.
https://doi.org/10.1109/TIP.2012.2214050

Zauner, C. (2010). *Implementation and benchmarking of perceptual image hash functions* [Master's
thesis, University of Applied Sciences Upper Austria]. https://www.phash.org/docs/
