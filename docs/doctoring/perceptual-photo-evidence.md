# Perceptual photo evidence

DiskSage uses perceptual hashing only to create review candidates. The current DCT construction and
Hamming comparison follow Zauner (2010), and the distance bound of 22 is the pHash project's
published intra/inter-image threshold rather than a locally invented weight. Exact aspect ratio
prevents unrelated portrait and landscape shapes from sharing a group. A BLAKE3 digest keeps exact
copies in the existing exact-content workflow.

Resolution, sample bit depth, format, lossless-format knowledge, and encoded bytes are retained as
separate observations. No weighted score is calculated. A unique recommendation exists only when
one item Pareto-dominates all others on the preservation dimensions; the customer still selects the
survivor after viewing the images. SSIM (Wang et al., 2004) remains a documented rejected grouping
gate until a representative labeled corpus can calibrate an operating point.

Managed `.photoslibrary` and `.photolibrary` trees are never entered, and dataless cloud
placeholders are rejected before decoding so an audit cannot hydrate them. Execution re-collects
the same report, checks active use and exact filesystem identity, then uses DiskSage's existing atomic
OS-Trash boundary and append-only journal. There is no permanent-delete mode.

Apple Photos libraries use the separate PhotoKit boundary documented in ADR 0023. PhotoKit asset
identifiers and resource reads replace package paths; network access is disabled during evidence
collection, so an iCloud-only original is neither downloaded nor admitted to a deletion plan.

## References

pHash. (2010). *pHash design*. https://www.phash.org/docs/design.html

W3C. (2025). *Portable Network Graphics (PNG) specification (third edition)*.
https://www.w3.org/TR/png-3/

Wang, Z., Bovik, A. C., Sheikh, H. R., & Simoncelli, E. P. (2004). Image quality assessment: From
error visibility to structural similarity. *IEEE Transactions on Image Processing, 13*(4),
600–612. https://doi.org/10.1109/TIP.2003.819861

Zauner, C. (2010). *Implementation and benchmarking of perceptual image hash functions* [Master's
thesis, Upper Austria University of Applied Sciences]. https://phash.org/docs/pubs/thesis_zauner.pdf
