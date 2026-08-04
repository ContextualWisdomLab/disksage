# iCloud local-copy batch eviction

DiskSage treats iCloud local-copy eviction as a destructive, evidence-bound operation. Planning remains read-only; execution is unavailable until every selected item has been replanned, the exact batch fingerprint has been approved by an attributed human, and the immutable record directory is outside all cloud-controlled paths.

## Fail-closed execution contract

- Every item receives a fresh clock reading; timestamps are never synthesized from a batch start time.
- The executor stops at the first failed or verification-incomplete item.
- A successful item result and a refreshed batch checkpoint are written before the next item begins.
- Failure to persist an item result marks verification incomplete, records the bounded failure code in the batch checkpoint, and halts execution.
- Manifest item-count and byte-size limits are enforced before parsing untrusted batch input.
- Record, manifest, and lock paths reject cloud-controlled locations, including symlinked ancestors.
- Control-path diagnostics remain distinct so operators can identify the exact rejected boundary without exposing source paths.

## Evidence and interoperability

The public Rust coordinator exposes injected planner, executor, record-writer, and clock seams. These seams make halt order, checkpoint order, record failures, and per-item clock reads deterministic in tests without changing the production executor. JSON records continue to use the existing versioned DiskSage schemas and bounded error codes so standalone use and CWL service ingestion remain compatible.

## Verification

Release acceptance requires the focused `cloud_local_eviction_batch::tests::` suite, the `disksage-icloud-local-eviction-batch` binary suite, formatting, whitespace validation, ordinary repository tests, security scans, and exact-head review gates to pass. The temporary repair workflows and scripts used to reproduce the regression are intentionally absent from the final source tree.
