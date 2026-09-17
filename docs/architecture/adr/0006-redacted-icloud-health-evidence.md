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

## Operational evidence update — 2026-08-24 15:34

The next bounded read-only receipt still reported `evidence_complete=true` and
`new_copy_admission_state=blocked`. The 343-item sync-up backlog and one active upload at 95.24%
were unchanged, while File Provider pending indexable items increased to 128,917; one download,
disk import, transfer activity, and the 28 filename/2 root exclusions remained present. Native
status continued to report `client_state=needs-sync` with `needs-sync-up|in-sync-down|prefer-sync-down|oob-sync-ack`.

This increasing aggregate queue is stronger provider-stall evidence but still cannot identify the
seven Finder items or prove a cloud write. The observation remains read-only and keeps
`provider_sync_attested=false`, `local_eviction_authorized=false`, and `mutation_performed=false`.

## Operational evidence update — 2026-08-24 15:46

The latest bounded read-only receipt still reported `evidence_complete=true` and
`new_copy_admission_state=blocked`. The sync-up backlog remained 343 items and the active upload
remained at 95.24%; one active download remained. File Provider pending indexable items increased
again to 130,571, while disk import, transfer activity, and the 28 filename/2 root exclusions
remained present. Native status continued to report `client_state=needs-sync` with sync-up pending.

The growing aggregate backlog is consistent with the Finder “preparing to copy” stall, but it does
not identify the seven Finder items or attest a cloud write. DiskSage therefore continues to keep
`provider_sync_attested=false`, `local_eviction_authorized=false`, and `mutation_performed=false`;
the probe performed no Finder, provider, source, or cloud mutation.

## Decision maintenance — 2026-08-24 16:03

The latest product review queue keeps the same safety decision: #247 is ready for review at
`59057c08eb5017ac57b640419a0c7e4779f443d7`, but queued checks and protected approvals are not yet
complete. The health evidence remains diagnostic only; no readiness, review, or queue state can
promote aggregate iCloud evidence into per-item upload attestation or local-eviction authority.

## Operational evidence update — 2026-08-24 16:03

The next bounded read-only receipt still reports `evidence_complete=true` and
`new_copy_admission_state=blocked`. The sync-up backlog remains 343 items; one upload remains at
95.24% and one download remains active. Pending File Provider indexable items reached 131,214,
with disk import, transfer activity, and the 28 filename/2 root exclusions still present. Native
status remains `client_state=needs-sync` with `needs-sync-up`.

The aggregate queue continues to grow, but the receipt still does not identify the seven Finder
items or attest a remote write. `provider_sync_attested=false`, `local_eviction_authorized=false`,
and `mutation_performed=false` remain invariant.

## Decision maintenance — 2026-08-24 17:49

The exact-head PR #247 integration run exposed and repaired a test-fixture defect in the mixed
destination-headroom regression. The unsafe symlink is now placed at the actual dated destination
ancestor derived by the same Rust production-date decomposition used by the planner; the verified
media candidate remains eligible while the unsafe document candidate remains diagnostically
partial. The focused suite passed 11/11 on Rust 1.97.1. No provider, Finder, source, or cloud
mutation rule changed.

## Decision maintenance — 2026-08-24 17:59

The Finder-copy cancellation control now tells the operator why macOS Accessibility/System Events
permission is required to send the fixed Escape request, and explicitly states that a denied request
does not mutate files or cloud data. The focused UI contract/privacy tests passed 6/6 and
`npm run check` reported zero diagnostics. This is explanatory UX only; provider admission,
attestation, and eviction remain fail-closed.

## Operational evidence update — 2026-08-24 16:21

The latest bounded read-only receipt still reports `evidence_complete=true` and
`new_copy_admission_state=blocked`. The sync-up backlog remains 343 items; one upload remains
active at 95.24% and one download remains active. File Provider pending indexable items increased
to 132,783, while disk-import, transfer, filename-exclusion, and root-exclusion notices remain.
Native status remains `client_state=needs-sync` with sync-up pending.

This is provider-global reconciliation evidence consistent with Finder remaining at “preparing to
copy”, but it neither identifies the seven items nor proves that DiskSage is holding a Finder lock
or that a cloud write completed. `provider_sync_attested=false`, `local_eviction_authorized=false`,
and `mutation_performed=false` remain required; no Finder, provider, source, or cloud mutation was
performed.

## Decision maintenance — 2026-08-24 16:32

The exact-head review loop repaired two independent safety/documentation findings without changing
the iCloud fail-closed decision: #246 restored the coverage dead-code allowance to
`node_navigation` (head `1972614`), and #227 renamed the bound audit parameter to `stable_root`
(head `5ad1197`) while retaining the intentionally nested private module contract. Both focused
Rust test slices passed locally; hosted checks and protected approvals remain authoritative gates.

## Decision maintenance — 2026-08-24 16:47

The exact-head loop also repaired #249's process-test storage gap at head `db95c54`: the three
feature-gated Git-worktree CLI integration tests now reuse deterministic private target directories
and remove stale output before each nested build, preventing process-id-named target accumulation.
This test-only cleanup does not alter provider, source, Finder, or cloud mutation boundaries.

## Decision maintenance — 2026-08-24 16:50

The current-head review queue was refreshed after the accessibility and compiler-baseline PRs were
marked ready: #203 is at `5f0bd51`, #244 at `13caeb0`, and #249 at `db95c54`. All remain blocked by
live hosted gates and protected approvals; none of these states changes the provider evidence
decision or authorizes source/cloud mutation.

## Operational evidence update — 2026-08-24 16:50

The latest bounded read-only receipt still reports `evidence_complete=true` and
`new_copy_admission_state=blocked`. The 343-item sync-up backlog, one active upload at 95.24%, one
active download, native `client_state=needs-sync`, and sync-up pending remain unchanged. Pending
File Provider indexable items increased to 135,334. The receipt remains aggregate provider evidence
only; `provider_sync_attested=false`, `local_eviction_authorized=false`, and
`mutation_performed=false` remain invariant.

## Decision maintenance — 2026-08-24 16:55

The current #249 exact head is `aa5c37d`. Its test-only target helper now keeps concurrent
process-scoped build directories while pruning dead-process or aged stale output; this preserves
the disk-reclamation goal without changing any provider, Finder, source, or cloud mutation rule.

## Operational evidence update — 2026-08-24 17:00

A fresh read-only `/usr/bin/brctl status` completed at 17:00. The iCloud container reports
`client:needs-sync` and `sync:needs-sync-up|in-sync-down|prefer-sync-down|oob-sync-ack`; the
bounded summary contains 1,740 `pending-scan` entries, 343 `pending-sync-up` entries, 1,807
scheduled sync-up markers, and 5 upload errors. Several queued uploads have not run for roughly
60–66 hours, including `CKErrorDomain:4` “Saving asset failed” records.

This is provider-global reconciliation/error evidence consistent with the Finder
`real_datasets` “복사 준비 중” dialog persisting for hours. It does not identify the seven Finder
items or attest a cloud write, so the evidence remains diagnostic only:
`provider_sync_attested=false`, `local_eviction_authorized=false`, and `mutation_performed=false`.
DiskSage must continue to expose only the explicit bounded Finder-cancel action and must not
restart provider processes or mutate Finder, source, or cloud state automatically. The root volume
had about 36 GiB available at the same observation, so disk-full is not the current root cause.
At 17:02, a read-only process inventory showed Finder (PID 1422), `fileproviderd` (1450), and
`bird` (1462) all started at 10:43:49, about 6h18m earlier. This confirms a long-lived provider
session but does not establish DiskSage ownership or a Finder lock.

## Decision maintenance — 2026-08-24 17:33

The exact-head PR #249 test repair is now `dc9ccf2`. Its three process-contract tests use Cargo's
`CARGO_BIN_EXE_disksage-git-worktree-audit` instead of launching nested feature-gated builds;
the focused slices passed 8/8, 2/2, and 1/1, with no new `disksage-git-worktree-*` temporary
targets created. This removes a local test-side source of disk pressure without changing the
provider, Finder, source, or cloud mutation boundaries. The PR is draft, blocked, review-required,
with hosted checks pending and no unresolved review threads.

## Decision maintenance — 2026-08-24 17:38

At exact head `b8a17eb`, retained iCloud health snapshots now accept the exact pre-
`pending_indexable_count` fingerprint encoding when that optional field is absent. New snapshots
still use the current fingerprint, and the 29-test iCloud health slice passed on Rust 1.97.1. This
preserves the restart-safe stall clock across upgrades without weakening evidence integrity or
changing the fail-closed provider/Finder/source/cloud mutation boundary.

## Operational evidence update — 2026-08-24 17:40

A fresh bounded read-only `/usr/bin/brctl status` still reports `client:needs-sync` with
`pending-scan=1,740`, `pending-sync-up=343`, and `sync-up-scheduled=2,150`; 20 lines matched the
bounded upload-error/“Saving asset failed” markers. Finder, `fileproviderd`, and `bird` remain the
same long-lived provider session started at 10:43:49. The root volume currently has about 21 GiB
available (926 GiB total, 12 GiB used), so this is not a full-root condition, but headroom is
lower than the earlier 36 GiB observation. The Finder copy remains diagnostic-only: no item-level
remote write is identified, and `provider_sync_attested=false`, `local_eviction_authorized=false`,
and `mutation_performed=false` remain invariant.

## Operational evidence update — 2026-08-24 21:00

The native status contract now retains only a bounded `pending_scan_count` derived from
`brctl status` apply markers and exposes the stable blocker
`icloud-native-status-pending-scan`. It never persists the marker's path or item identifier. The
same blocker is validated in Naruon readiness and displayed beside the Finder cancellation
guidance; it does not authorize cancellation, cloud writes, attestation, or source eviction.

## Runtime projection update — 2026-08-25 00:00

The iCloud health persistence path now propagates the selected bounded blocker to existing iCloud
receipt-linked Goal/ADR projections. Goal status and completion gates therefore reflect the current
provider-sync hold after restart or a manual health inspection, while receipt/evidence authority is
unchanged. Projection directory, receipt contents, and provider identifiers are not included in
the emitted notices.
