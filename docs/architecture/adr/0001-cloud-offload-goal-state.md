# ADR-0001: Cloud offload provider state drives the goal

**Status:** Accepted
**Date:** 2026-08-13
**Scope:** DiskSage cloud copy, provider attestation, and source-eviction gate

## Context

A local File Provider copy is not proof that iCloud, OneDrive, or Google Drive
has uploaded the bytes. In particular, macOS can report a file as local and
current while `is_uploaded=false`. Manual re-checks and hand-maintained task
notes allow the displayed goal to drift from the evidence that protects the
source file.

## Decision

DiskSage records the provider-native state in every `ProviderSyncEvidence`:
`complete`, `pending-upload`, `not-ubiquitous`, `not-local-current`,
`uploading`, `excluded-from-sync`, `sync-paused`, `remote-unavailable`, or
`content-mismatch`. Legacy records deserialize as `unknown` and retain their
original boolean gate.

The runtime goal is derived from the same evidence and exposed by both the
Rust command output and the UI:

`copy-verified → pending-provider-sync → provider-sync-confirmed → eviction-ready → source-evicted`.

After each attestation, DiskSage atomically updates a per-receipt,
machine-readable ADR snapshot at the app-data `cloud-adr` directory. The
snapshot contains only identifiers, state, decision, consequences, and the
evidence record ID; the immutable provider evidence remains the authority for
content hashes and timestamps. `eviction-ready` never deletes the source.

## Consequences

- `local-current / not-uploaded` is visible as `pending-upload` and keeps the
  source-retention goal active.
- UI polling can update the Goal without another manual copy or attestation
  operation.
- ADR and Goal state are auditable from the same evidence record and cannot be
  silently edited in place by the provider check.
- A separate explicit trash operation is still required after an eviction
  permit; it is not automatic.
- The source-eviction command moves the source to the OS Trash only after a
  fresh provider attestation and updates the Goal/ADR to `source-evicted`.

## References

- `src-tauri/src/cloud_transfer.rs` (`ProviderSyncState`, `CloudOffloadGoalState`)
- `src-tauri/src/cloud_adr.rs` (dynamic ADR snapshot writer)
- `src-tauri/src/provider_sync.rs` (iCloud/File Provider/API classification)
