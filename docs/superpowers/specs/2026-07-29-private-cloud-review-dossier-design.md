# Private cloud review dossier

## Problem

The headless decision summary is intentionally safe to share: it omits absolute paths, content
titles and authors, raw embedded metadata values, source context values, and dataset profiles.
Those fields are nevertheless required for a real human production-time and destination-context
review. Printing the full `CloudPlanReport` to a terminal provides the fields, but it also makes
accidental log and transcript disclosure likely.

## Interface

`disksage-cloud-plan --decision-summary --review-reason-set REASON|REASON
--private-review-output /absolute/new-file.json` performs one fresh, single-destination plan.

- Standard output remains the existing redacted `review-batch-summary`.
- The private output is a create-new regular file with mode `0600` on Unix.
- Platforms where DiskSage cannot enforce that private mode fail closed instead of writing the
  dossier with weaker default permissions.
- The file contains only candidates whose sorted review reasons exactly match the requested set.
- Each candidate keeps the full lineage evidence required for review: source and planned
  destination, source context, embedded production-time evidence, all observed fallback evidence,
  title, authors, contextual fields, duration, dataset profile, fingerprints, and blockers.
- The dossier binds the full-plan decision fingerprint and the stable reason-set review-batch
  fingerprint. Standard output reports the dossier SHA-256 without repeating private values.
- The flag is rejected for multicloud mode, root inspection, mutation actions, exact-duplicate
  review mode, relative paths, an existing output path, or a missing exact review reason set.

The dossier is inspection evidence only. It cannot create a review decision, copy a file, attest
provider sync, evict a local source, or authorize any later action as a batch. Approve/hold
decisions remain individual, attributed, immutable, and bound to each candidate's metadata and
review fingerprints.

## Integration decisions

This is deterministic serialization and filesystem safety logic implemented in Rust. No LLM,
LLM-as-a-Judge, Noema runtime, external orchestrator, semantic catalog, or database is needed.
`fast-mlsirm`, `semantic-data-portal`, and `pg-erd-cloud` remain out of scope until a judgment
engine or persistent cross-device catalog is actually introduced.
