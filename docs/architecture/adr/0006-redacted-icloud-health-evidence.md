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

## Consequences

### Positive

- The current iCloud incident remains comparable after a restart or UI refresh.
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
