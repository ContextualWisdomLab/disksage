# Existing cloud copy adoption design

## Problem

The planner intentionally reports `destination-exists` when a lineage destination is already
present. Previously that condition could neither be copied nor verified, so DiskSage could not
issue a receipt for a cloud copy created before the current run. Treating equal logical size as
proof is unsafe, especially for a macOS File Provider placeholder.

## Safety contract

Adoption is a separate explicit action. It clears only the exact fresh-plan
`destination-exists` condition and retains every metadata, review, account-scope, and path gate
used by the normal copy action. The source must remain outside the selected cloud root, and the
existing destination must canonicalize inside it. Both must be regular non-symlink files.

DiskSage rechecks source size and modification time, then computes BLAKE3, SHA-256, and Microsoft
QuickXor over both files. It issues an immutable receipt only when all three digests and byte counts
match and neither file changed during verification. Reading a File Provider placeholder may hydrate
it; a failed or incomplete download therefore blocks receipt creation rather than weakening proof.
Neither source nor destination is removed or replaced, including when receipt persistence fails.

## Receipt lineage

The existing version 3 receipt remains the wire format. Its lineage snapshot adds
`copy_verification_method`. The default `copied-by-disk-sage` value is omitted during serialization
so previously issued version 3 lineage fingerprints remain valid. Adopted receipts record
`adopted-existing`, binding that distinction into the lineage fingerprint and receipt id.

## Interfaces

- Headless CLI: `--adopt-existing-fingerprint HEX64 --receipt-dir ABSOLUTE_PATH`, with
  `--review-dir` when an evidence-bound review is required.
- Tauri command: `adopt_existing_cloud_candidate`.
- UI: candidates blocked only by `destination-exists` expose a dedicated full-hash verification
  and adoption action after the normal metadata/review gates pass.

Adoption, copy, review, attestation, eviction, and root-inspection actions are mutually exclusive.
Provider-native sync attestation and explicit receipt-id-confirmed source trash remain separate
later steps.

## Integration decision

This path is deterministic local I/O and remains Rust-first. It does not require Noema, an LLM,
contextual-orchestrator, semantic-data-portal, pg-erd-cloud, or fast-mlsirm.
