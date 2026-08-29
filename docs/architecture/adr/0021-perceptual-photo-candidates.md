# ADR-0021: Require measured evidence and a selected survivor for perceptual photo candidates

Status: Accepted

## Context

Cryptographic content identity finds exact copies but not a resized or recompressed photograph.
Filename, file size, or a weighted “quality score” cannot establish that two files show the same
scene or which one preserves the best source evidence. A perceptual match is also probabilistic:
it must not become automatic deletion authority.

## Decision

DiskSage decodes supported raster images locally in Rust and produces the 64-bit DCT perceptual
hash described by Zauner (2010): 32×32 luminance, the 8×8 low-frequency DCT block, median
quantization, and Hamming distance. Candidate edges require an exact reduced aspect-ratio match and
Hamming distance no greater than 22, the pHash project's published separation threshold for its
intra- and inter-image evaluation. Connected candidates with different BLAKE3 content identities
form a deterministic review group; byte-identical copies stay in the exact-duplicate workflow.

Every member records dimensions, pixel count, sample bit depth, encoded format, whether that format
is known to be lossless, encoded byte length, content digest, filesystem identity, and modification
time. DiskSage does not combine these measurements with weights. It recommends a survivor only
when exactly one member Pareto-dominates every other member in pixel count, sample depth, and known
compression preservation. Otherwise the evidence is explicitly incomparable.

The user selects exactly one survivor in every group. The selection set and audit fingerprint form
an exact approval phrase. Execution repeats the full audit, checks active use and filesystem
identity, stages each non-survivor atomically, and moves it to OS Trash with journal and item
receipts. Permanent deletion is never available. Paths containing `.photoslibrary` or
`.photolibrary` are pruned before traversal, a managed-library root is rejected, and dataless cloud
placeholders are excluded before any content read that could hydrate them.

## Consequences

- Resize and compression variants become visible without treating visual similarity as equality.
- Quality evidence remains interpretable and unweighted; incomparable originals require direct
  human inspection.
- Candidate false positives cannot authorize unattended deletion.
- The current pairwise comparison is quadratic within one exact-aspect-ratio bucket. It can move to
  an exact Hamming-index implementation if measured corpora show that bound is too slow without
  changing the evidence contract.

## Rejected alternatives

- Filename, byte length, modification time, and arbitrary weighted quality scores are not image
  identity or preservation evidence.
- Automatic deletion at a perceptual threshold is rejected because perceptual hashes admit
  collisions and do not establish provenance.
- SSIM is not used as a grouping threshold because it requires a reference image and an operating
  threshold that this product has not calibrated on a representative labeled corpus.
- Photos Library package members are not scanned or moved individually because macOS manages their
  database and resource relationships.

## References

W3C. (2025). *Portable Network Graphics (PNG) specification (third edition)*.
https://www.w3.org/TR/png-3/

Wang, Z., Bovik, A. C., Sheikh, H. R., & Simoncelli, E. P. (2004). Image quality assessment: From
error visibility to structural similarity. *IEEE Transactions on Image Processing, 13*(4),
600–612. https://doi.org/10.1109/TIP.2003.819861

Zauner, C. (2010). *Implementation and benchmarking of perceptual image hash functions* [Master's
thesis, Upper Austria University of Applied Sciences]. https://phash.org/docs/pubs/thesis_zauner.pdf

pHash. (2010). *pHash design*. https://www.phash.org/docs/design.html
