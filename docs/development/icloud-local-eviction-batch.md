# Cloud local-copy batch eviction

DiskSage treats iCloud and OneDrive local-copy eviction as destructive, evidence-bound operations. Planning remains read-only; execution is unavailable until every selected item has been replanned, the exact batch fingerprint has been approved by an attributed human, and the immutable record directory is outside all cloud-controlled paths.

## Fail-closed execution contract

- Every item receives a fresh clock reading; timestamps are never synthesized from a batch start time.
- Planning excludes sync-incomplete or otherwise unsafe items by index and bounded error code, so
  one unsafe item cannot prevent separately verified items from reaching human approval.
- The executor stops at the first failed or verification-incomplete item.
- A successful item result and a refreshed batch checkpoint are written before the next item begins.
- Failure to persist an item result marks verification incomplete, records the bounded failure code in the batch checkpoint, and halts execution.
- The manifest byte-size limit is enforced before parsing untrusted batch input. The item-count limit is checked immediately after `serde_json::from_slice` constructs the `InputManifest` and before any item is planned or executed.
- Record, manifest, and lock paths reject cloud-controlled locations, including symlinked ancestors.
- Control-path diagnostics remain distinct so operators can identify the exact rejected boundary without exposing source paths.

## Evidence boundary and interoperability

**Local-only evidence** includes canonical source paths, detected cloud-root details, record-directory locations, and immutable item or batch records that contain those paths. It remains on the operator-controlled system and is not a service-ingestion payload.

**Shareable evidence** is limited to the path-free CLI plan and result views: schema versions, counts, byte totals, fingerprints, approval identifiers, bounded error codes, completion flags, and stable notices. Shareable evidence must never include source paths, user-file content, or control-directory locations. A CWL service or future Naruon module can ingest this bounded contract without requiring DiskSage to be deployed as a service; DiskSage therefore remains independently operable while preserving a narrow MSA interoperability boundary.

The public Rust coordinator keeps its production dependencies internal. Private, test-only planner, executor, record-writer, and clock seams make halt order, checkpoint order, record failures, and per-item clock reads deterministic in tests without exposing an injection API or changing the production executor. JSON records continue to use the existing versioned DiskSage schemas and bounded error codes.

## Standards and design-principle mapping

This mapping documents engineering intent and is **not a certification claim**, conformity assessment, or assertion that DiskSage implements an entire control catalog.

| Source | Relevant requirement or principle | DiskSage implementation evidence |
| --- | --- | --- |
| NIST SP 800-53 Release 5.2.0, AC-3 | Enforce approved access before an operation is permitted. | Execution requires the exact batch fingerprint, attributed human approval, and successful live re-planning before the first local-copy eviction request. |
| NIST SP 800-53 Release 5.2.0, AU-9 | Protect audit information against unauthorized modification or deletion. | Approval, item-result, and batch-checkpoint records use create-new immutable writes; a record failure halts the batch and marks verification incomplete. |
| NIST SP 800-53 Release 5.2.0, SI-10 | Validate externally supplied information before use. | Manifest bytes and item counts are bounded, paths are canonicalized and checked against protected locations, and fingerprints are recomputed before execution. |
| ISO/IEC 27040:2024 | Apply documented risk controls to storage systems and storage-management activity across the data lifecycle. | The operator contract separates read-only planning from destructive execution, binds action evidence, records lifecycle outcomes, and preserves cloud item paths while verifying local allocation reduction. |
| Saltzer and Schroeder | Use fail-safe defaults, complete mediation, and least privilege. | Missing or stale evidence denies execution; every item is mediated by re-planning and checkpointing; planning and path-free reporting do not receive mutation authority. |

The implementation applies **fail-safe defaults** by treating absent, stale, malformed, unrecordable, or verification-incomplete evidence as denial. It applies **complete mediation** by checking every selected item immediately before execution and after each attempted mutation. It applies **least privilege** by keeping planning read-only and granting mutation capability only to the explicitly approved execution path.

## Verification

Release acceptance requires the focused `cloud_local_eviction_batch::tests::` suite, the `disksage-cloud-local-eviction-batch` binary suite, the documentation contract tests, formatting, whitespace validation, ordinary repository tests, security scans, and exact-head review gates to pass. Temporary repair workflows and scripts used to reproduce a regression are intentionally absent from the final source tree.

## References

International Organization for Standardization & International Electrotechnical Commission. (2024). *Information technology—Security techniques—Storage security* (ISO/IEC 27040:2024).

Joint Task Force. (2020). *Security and privacy controls for information systems and organizations* (NIST Special Publication 800-53, Revision 5; Release 5.2.0, August 27, 2025). National Institute of Standards and Technology. https://doi.org/10.6028/NIST.SP.800-53r5

Saltzer, J. H., & Schroeder, M. D. (1975). The protection of information in computer systems. *Proceedings of the IEEE, 63*(9), 1278–1308. https://doi.org/10.1109/PROC.1975.9939
