# Native File Provider Attestation Design

## Goal

Verify that a DiskSage copy in OneDrive or Google Drive is uploaded without requiring OAuth for
the common macOS case. The result must remain bound to the immutable copy receipt and must never
hydrate, evict, upload, delete, or otherwise mutate either file.

## Evidence order

1. Validate the immutable receipt before trusting its paths.
2. Confirm that the receipt destination remains inside the currently discovered root for the same
   provider.
3. Ask macOS File Provider for per-file status with a bounded, argument-only
   `/usr/bin/fileproviderctl evaluate` invocation.
4. Require exactly one provider-reported `documentSize` and require it to match the local logical
   size. Reject missing, malformed, duplicate, or conflicting status fields.
5. Require the destination to be downloaded, not downloading, and the most recent local version
   before reading it. A cloud-only placeholder therefore fails closed instead of being hydrated.
6. Hash the already-local file and require its size and BLAKE3 to match the receipt.
7. Ask for status again, and reject a changed file identity, size, or modification time.
8. Mark native evidence complete only when the file is uploaded, not uploading, not excluded from
   synchronization, and synchronization is not paused.
9. If native evidence is incomplete or unavailable and the user supplied a provider object ID,
   fall back to the existing read-only OAuth API revision and checksum proof.

For iCloud, the Foundation adapter binds both `NSURLUbiquitousItemIsUploadedKey` and
`NSURLUbiquitousItemIsUploadingKey`. Even an anomalous state that reports uploaded and uploading
at the same time fails closed. Separately, the attestation output compares its provider observation
time with the immutable receipt copy time. An incomplete observation is `pending` for the first 24
hours and `overdue` afterward. Timeliness is diagnostic only: it never changes `sync_complete`,
creates a permit, retries an upload, or mutates either copy.

## Trust boundaries

- Native evidence is accepted for iCloud, OneDrive, and Google Drive only when it contains no
  synthetic remote-content proof. API evidence remains mandatory when remote-content fields are
  present.
- The command uses no shell, has a five-second timeout, caps output at 256 KiB, suppresses stderr,
  and parses every required size and boolean field exactly once and fail-closed.
- Native evidence records the provider, receipt ID, destination, observed bytes, destination hash,
  status bits, and confirmation time in its evidence identifier.
- Every attestation first writes the complete observation into a bounded, read-only,
  integrity-bound provider evidence record. A successful attestation validates that record and
  creates a local-eviction permit bound to its integrity ID. DiskSage still retains the source and
  performs no removal action.

## User experience

File Provider metadata is checked first. OneDrive derives its API fallback from the exact receipt
path and rejects an item ID. Google Drive accepts a file ID only as the starting point for a twice-
stable parent-chain proof to the My Drive root; ID-only checksum evidence cannot authorize source
eviction. Both use an existing OS-keychain OAuth connection. The headless `--attest-receipt` path
supports all three providers using native status and the same provider-specific fallback rules.
The output and desktop UI distinguish a normal confirmation wait from an overdue unconfirmed copy,
report the exact pending age, and continue to retain the source in both cases.
The desktop app stores records under its application-data `cloud-provider-evidence` directory;
the headless command requires an explicit absolute `--evidence-dir`.

## Why this remains metadata-first

Filenames and filename-like dates do not establish provenance or successful upload. Candidate
selection continues to prioritize embedded metadata, bounded dataset schemas, acquisition origin,
and explicit review. This attestation slice adds provider-owned synchronization metadata and binds
it to content hashes; it does not upgrade filename dates into production evidence.
