# ADR-0006: Persist redacted iCloud health evidence as a bounded observation stream

**Status:** Accepted  
**Date:** 2026-08-20

## Context

DiskSage already records local-volume pressure and provider-client process observations, but the
iCloud queue and File Provider activity probe existed only in the current process response. During
the `real_datasets` incident, `fileproviderctl` exposed expired `create`/`fetch` requests with no
progress while `bird` and `fileproviderd` remained active. A later loop could not compare that
provider observation with the capacity and runtime observations after the UI was closed.

The private CloudDocs database and File Provider dump can contain paths, filenames, item identifiers,
and provider-internal details. Persisting either raw source would create an unnecessary privacy and
integrity boundary, and it could be mistaken for per-item upload attestation.

## Decision

Persist a small, path-free `IcloudSyncHealthEvidenceSnapshot` after every successful read-only
iCloud health probe. The snapshot contains the observation timestamp, aggregate queue counts and
bytes, admission blockers, bounded native-status state, and redacted File Provider activity counts.
It contains no raw dump, path, filename, item identifier, account identifier, remote-capacity claim,
cloud-write claim, or eviction authority.

Records are written below `icloud-sync-health-evidence` in the application-data directory using
create-new files named by the observation timestamp and SHA-256 fingerprint. Each record is capped
at 64 KiB, fsynced, assigned Unix mode `0400` in a mode `0700` directory, and retained only while it
matches DiskSage's exact bounded record name (at most 128 records). A duplicate observation cannot
overwrite a prior record; an unsafe report is rejected before writing. An incomplete report may be
retained with `evidence_complete=false`, but it remains an explicit blocked observation and can
never promote copy or eviction authority.

The writer is advisory evidence only. Copy admission still requires the live fail-closed iCloud
health report, current local headroom, capacity evidence, review approval, and per-item provider
attestation. A persistence failure is surfaced to the UI and does not grant or revoke authority.
The timestamped records are the third evidence stream alongside `volume-pressure-evidence` and
`provider-client-runtime-evidence`. iCloud plans combine the three records with the bounded
freshness comparator in [ADR-0007](0007-pre-copy-evidence-cohort.md); a missing, incomplete,
malformed, or skewed stream remains blocked without reconstructing a provider dump.
After the current observation is written, the command returns the earliest retained timestamp for
the same admission-blocker set as `admission_blocked_since_ms`. The UI uses that diagnostic value
when starting its stall clock, falling back to the current observation only when durable evidence
is unavailable. This preserves a visible stall duration across an application or system restart;
it never changes copy, attestation, or eviction authority.

## Consequences

### Positive

- The current iCloud incident remains comparable after a restart or UI refresh.
- A restarted UI retains the provider stall duration when the bounded evidence journal is readable,
  instead of presenting a long-running Finder preparation as a newly observed block.
- Provider evidence is durable without copying private provider databases or raw output.
- Bounded create-only records preserve provenance and fail closed on malformed claims.
- The UI can tell the operator when current evidence was observed and when durable comparison failed.

### Negative

- Three small evidence streams must be correlated by observation time; they do not become one
  authoritative cloud-sync claim.
- Retention is bounded, so very old incident history is intentionally discarded.
- Native provider schema changes can make a snapshot incomplete and block new-copy admission.

## Rejected alternatives

- **Persist the CloudDocs database or raw `fileproviderctl` dump:** rejected because it leaks private
  paths/provider internals, can consume disk during pressure, and is not per-item attestation.
- **Treat a quiet queue as upload completion:** rejected; provider-native per-item evidence remains
  mandatory before any source eviction.
- **Write to Apple's managed database:** rejected; DiskSage remains read-only against provider-owned
  state.

## Related decisions

- [ADR-0001](0001-cloud-offload-goal-state.md) — provider evidence and fail-closed eviction gates.
- [ADR-0005](0005-hourly-agent-loop-is-advisory.md) — scheduled loops remain advisory and cannot
  authorize mutation.

## Operational evidence update — 2026-08-24

The post-restart bounded observation recorded `pending-indexable-count=32377`, a `28123`-entry
reconciliation queue, upload progress `6229217391/6540678102`, `scheduling state: running`,
`disk import: yes`, and `stream reset: yes`; `brctl` still reported `needs-sync-up|needs-sync-down`.
These aggregate values are incident evidence only. They do not identify a `real_datasets` item or
prove a cloud write, so the existing decision continues to require per-item provider evidence and
keeps copy, attestation, and source eviction fail-closed.

The same bounded observation also captured File Provider activity while the Finder dialog remained
at “preparing to copy” for hours: iCloud continued redacted item ingestion, while a separate
Google Drive File Provider request returned `NSFileProviderErrorDomain -1004` (device cannot
connect to the server) during root materialization. The provider name is therefore part of the
diagnosis; a Finder progress window alone cannot tell which provider is stalled. DiskSage records
this as provider-specific runtime evidence, exposes the existing explicit Finder-cancel action,
and never infers copy completion or grants eviction authority from the dialog.

## Operational evidence update — 2026-08-24 11:34

A later bounded read-only observation increased the aggregate iCloud queue to
`pending-indexable-count=39404` and `reconciliation=35150` while the same upload counter remained
at `6229217391/6540678102` (95.24%), with `scheduling state: running`, `disk import: yes`, and
`stream reset: yes`. `brctl` still reported `needs-sync-up|needs-sync-down` and pending scans were
about 55 hours old. This worsening aggregate state reinforces the existing fail-closed decision;
it still does not bind the Finder `real_datasets` dialog to an item-level cloud write.

## Operational evidence update — 2026-08-24 13:53

A bounded local recheck at `13:48:21 +0900` found about 96 GiB free on the root volume while Finder,
`fileproviderd`, and `bird` had remained alive for roughly three hours. The visible `real_datasets`
target remained 512 bytes with mtime `2026-08-20 03:28:07 +0900`; no target handle appeared in the
bounded process-handle sample. The latest complete iCloud health receipt available for this loop
reported 343 uploads blocked on sync-up, one active upload at 95.24%, one active download, and
74,946 pending indexable items. These facts are aggregate provider evidence, not per-item cloud
attestation. The decision therefore remains unchanged: DiskSage reports the reconciliation/indexing
backlog, offers only the explicit bounded Finder-cancel action, and keeps copy, attestation, and
source eviction fail-closed. No provider process, CloudDocs database, source, or cloud object was
mutated.

## Operational evidence update — 2026-08-24 14:11

The exact-head `disksage-icloud-sync-health` binary completed another read-only CloudDocs/WAL
snapshot with `evidence_complete=true` and `new_copy_admission_state=blocked`. Aggregate upload
backlog remained 343 items blocked on sync-up and one active upload remained at 95.24%; File
Provider pending indexable items increased from 74,946 to 103,013 while one download and the
disk-import/transfer notices remained active. The `real_datasets` target still had 14 entries,
512 bytes, and mtime `2026-08-20 03:28:07 +0900`, with about 94 GiB available on `/`.

The observation remains supplementary global provider evidence. It does not identify a Finder item
or attest a cloud write, so `provider_sync_attested=false`, `local_eviction_authorized=false`, and
`mutation_performed=false` remain required. DiskSage continues to expose only the explicit bounded
Finder-cancel action and never restarts provider processes or mutates provider, source, or cloud
state from this evidence.

## Operational evidence update — 2026-08-24 14:31

A fresh read-only health receipt observed `evidence_complete=true` and
`new_copy_admission_state=blocked`. Aggregate upload state remained 343 items blocked on sync-up
with one active upload at 95.24%; one active download and File Provider indexing, disk-import, and
transfer activity remained, while pending indexable items increased to 110,652. Native status
continued to report `client_state=needs-sync` with sync-up/down pending, and filename/root
exclusions were still present.

The root volume had about 83 GiB available and a bounded `lsof` sample found no handle on the
`real_datasets` target while Finder remained at “preparing to copy”. This is provider
reconciliation/indexing evidence, not disk exhaustion or per-item cloud-write proof. The decision
is unchanged: keep `provider_sync_attested=false`, `local_eviction_authorized=false`, and
`mutation_performed=false`; expose only the explicit bounded Finder-cancel action and never
restart providers or mutate provider, source, or cloud state from this aggregate receipt.

## Operational evidence update — 2026-08-24 14:55

The next bounded read-only CloudDocs/WAL snapshot completed with
`evidence_complete=true` and `new_copy_admission_state=blocked`. The upload queue still contained
343 items blocked on sync-up; one active upload remained at 95.24% and one active download was
present. File Provider pending indexable items increased to 121,859, with the same disk-import,
transfer, filename-exclusion, and root-exclusion notices. Native status continued to report
`client_state=needs-sync` and sync-up/down pending.

The root volume still had 66 GiB available, the 14-entry `real_datasets` directory remained 512
bytes with its 2026-08-20 mtime, and the bounded `lsof` sample found no handle on that directory.
This is a worsening provider reconciliation/indexing backlog, not local disk exhaustion or
per-item cloud-write proof. The existing decision therefore remains fail-closed:
`provider_sync_attested=false`, `local_eviction_authorized=false`, and `mutation_performed=false`.
