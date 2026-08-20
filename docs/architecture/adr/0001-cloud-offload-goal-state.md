# ADR-0001: Provider evidence drives the cloud-offload Goal

**Status:** Accepted  
**Date:** 2026-08-13

## Context

A File Provider destination that is local and current is not necessarily uploaded. In particular,
`is_local_current=true` with `is_uploaded=false` must remain distinguishable from a completed
provider sync. Manual notes allow the displayed Goal and the evidence protecting the source to
drift.

## Decision

DiskSage stores the provider state (`pending-upload`, `uploading`, `not-local-current`, and other
fail-closed states) in content-bound evidence. The runtime Goal is derived from the same receipt
and immutable evidence:

`copy-verified → pending-provider-sync → provider-sync-confirmed → eviction-ready → source-evicted`.

After copy, DiskSage atomically writes `cloud-goals/<receipt-id>-latest.json`. After each provider
attestation and the explicit OS-Trash step, it atomically writes both that Goal projection and
`cloud-adr/<receipt-id>-latest.json`. The projections contain no credentials and are never used as
the authority for eviction; the receipt and immutable evidence are revalidated at every mutation.
If an attestation finds the destination valid but the receipt's source is absent or unsafe, the
runtime writes a blocked Goal projection, records the source-state blocker in the ADR, and issues
no eviction permit. If a prior projection has a higher monotonic state, that historical state is
preserved while the replaceable Goal is updated to `blocked` and its explicit eviction gate is
revoked; a terminal `source-evicted` projection is not rewritten merely because its original path
is now absent.

Production-time lineage is recorded with explicit precedence: embedded file metadata first, then an
unambiguous filename date token, then filesystem creation time, and finally filesystem modification
time. Tokens such as `2026-04-28` or `251210` are stored as `filename:path-token` evidence with
low confidence and force review when they are selected; they are planning evidence, not proof of
cloud sync, ownership, or permission to evict the source. An embedded/filename disagreement is
also retained as a review blocker rather than silently resolved.

For personal OneDrive and Google Drive roots, a running native desktop client may admit the
copy-only step when the only missing evidence is the separate OAuth quota connection. This mode is
explicitly marked as capacity-unverified, requires a fresh provider-wide sync admission, and
retains the source until per-item native sync evidence is attested. It never authorizes API upload,
remote-capacity claims, or source eviction; organization/shared roots and other OAuth failures
remain blocked.

Native File Provider copies also require a fresh source-volume headroom check: available bytes must
cover the candidate plus a fixed 1 GiB staging reserve. DiskSage applies this gate in the preview
UI and immediately before the copy, returning `local-volume-headroom-insufficient` rather than
leaving Finder or a provider staging operation waiting indefinitely. Explicit provider-API uploads
are a separate path and do not use this local staging requirement.

On macOS, a DiskSage-initiated native copy does not delegate the transfer to a Finder folder
operation. It invokes fixed `/bin/mkdir -p` and `/bin/cp` helpers in private process groups with a
size-derived timeout capped at 30 minutes, then re-hashes both source and destination and rechecks
the source identity. A timeout or helper failure removes only the child-created destination and
records `cloud-copy-timeout` or `cloud-copy-helper-failed`; the source and all provider evidence
remain unchanged. This bounds File Provider writes without turning a timed-out copy into an
attestation.

Source enumeration is also forbidden inside managed File Provider trees (`Library/Mobile
Documents`, `Library/CloudStorage`, `Library/Application Support/FileProvider`, and
`File Provider Storage`). If one of these trees is supplied as the scan root, the bounded collector
returns an incomplete scan with `source-scan-managed-file-provider-root` and produces no transfer
candidate. This prevents DiskSage diagnostics from competing with, or materializing, provider
state.

Regenerable development artifacts are a separate local reclaim domain. The headless
`disksage-dev-artifacts` command reuses the GUI's bounded metadata manifest and identity-bound
OS-Trash operation: inventory is read-only by default, and `--execute` requires a fresh rescan,
an unchanged filesystem object identity, and a journal path. It never removes source data,
provider-managed trees, or cloud placeholders, and it never claims cloud write or source eviction.

Archive production time is metadata-first inside the container as well as at the filesystem
boundary. ZIP central-directory timestamps are context evidence; bounded embedded formats such as
RFC 5322 `.eml` headers are inspected without extraction and take precedence when complete. If an
archive's bounded inner-header scan is incomplete or malformed, metadata evidence is incomplete
and the candidate remains blocked until a complete review is available.

DiskSage repositories, Git worktrees, and temporary evidence are operated from a local volume
outside managed File Provider roots. A provider-domain marker on the parent or a dataless `.git`
entry is treated as provider materialization evidence, not as proof of a stale worktree; the
worktree audit stops and must be relocated before it can continue.

Provider-wide File Provider dumps are bounded by both output size and wall-clock time. If a timed-out
dump has already emitted safe aggregate markers, DiskSage may retain only those markers as
incomplete evidence; it records `provider-global-sync-probe-timeout`, marks the provider state
`unavailable`, and continues to block new copies. A partial dump can never become authoritative
clear evidence. The iCloud activity probe also drains residual pipe output for at most one second
after normal or graceful child exit, never retaining more than 256 KiB; a stalled drain terminates
the private process group and remains incomplete evidence. Raw provider dumps are never written to
disk by the product.

The local CloudDocs client database is also bounded before any query or snapshot. When `client.db`
exceeds the snapshot ceiling, DiskSage skips both the expensive snapshot and the fallback query,
returns incomplete evidence, and still runs the bounded File Provider activity probe. Pipe readers
are non-blocking and every provider subprocess is terminated with its private process group on
timeout; a health check cannot remain stuck behind a provider copy.

When provider-wide evidence reports a user-space OneDrive or Google Drive client failure, DiskSage
may request a bounded desktop-client recovery from the UI. The action uses only the fixed, verified
application bundle, asks the client to quit through `osascript`, waits for the process observation to
clear, and launches the same bundle through `/usr/bin/open`. iCloud `bird` is system-managed and is
never force-terminated. The recovery result is diagnostic only: it records whether the launch was
observed afterward and can never claim cloud write, upload attestation, or source-eviction authority.
The same recovery contract is available to automation through the
`disksage-provider-recovery --provider onedrive|google-drive` CLI. Its optional 0600 output is a
recovery receipt, not a cloud-transfer receipt; a quit timeout or failed quit remains a blocker and
must not be retried through Finder or a force-kill. An operator may explicitly add
`--allow-graceful-term` to request SIGTERM for that same verified app when AppleScript cannot quit
it; SIGKILL, arbitrary PID selection, and iCloud termination remain unavailable.

Podman VM storage is a separate local reclaim domain, not cloud data. DiskSage exposes read-only
machine, guest-filesystem, image, container, volume, and raw-image evidence through the same
cleanup surface. Shared layers, sparse VM allocation, and unlinked volumes are never treated as
physical reclaim proof; prune, trim, stop, and delete remain outside the inspection command.

Capacity and provider-client process observations produced during a cloud plan are also persisted
as path-free, create-only records below the app-data directory in
`volume-pressure-evidence` and `provider-client-runtime-evidence`. Each record is bounded to
64 KiB, content-fingerprinted, fsynced, permissioned `0400` (directory `0700` on Unix), and
retained for at most 128 DiskSage-shaped snapshots. Retention only removes those exact record
names; unrelated app-data files and all provider databases are outside the cleanup boundary.
Provider activity evidence remains a separate bounded receipt and is never copied into these
capacity/process records or reconstructed from an incomplete probe.

### Operational amendment (2026-08-21)

The provider-global-sync panel records the local observation time returned by each bounded
diagnostic and tells the operator that the next automatic recheck is one minute later. A
temporarily disconnected or unreadable File Provider root must not prevent the read-only provider
diagnostic or the fixed OneDrive/Google Drive client-recovery request from running; destination
readability remains mandatory for copy, attestation, and eviction mutations. This keeps a Finder
“copy preparing” incident actionable without treating a recovery request as cloud-write evidence.
The 2026-08-21 incident observation also recorded a `real_datasets` Finder copy still preparing
after hours, Google Drive `temporarily disconnected`/`needs-indexing` with File Provider `-1004`,
and only 150 MiB of local headroom; the operator guidance is to cancel that pending Finder copy
before any new DiskSage plan is attempted.
The follow-up bounded dump also exposed repeated File Provider `-1005 itemNotFound` entries while
Google Drive upload/download and reconciliation were still active; DiskSage classifies that
path-free condition as `provider-global-sync-item-not-found` and keeps copy, attestation, and
eviction fail-closed.
The provider UI keeps a path-free blocker fingerprint across bounded observations and escalates
the operator guidance after 15 minutes of the same blocker; this is advisory state only and never
authorizes a cloud write, provider restart, or source eviction.
The latest bounded observation still reports the same transfer/reconciliation and `-1005`
missing-item cohort with only about 1.2 GiB of local headroom; a Homebrew cleanup dry-run found no
reclaimable entries, so DiskSage keeps the Finder copy cancellation and provider-quietness gate in
force rather than inventing a local cleanup authority.

## Consequences

- `is_local_current=true` and `is_uploaded=false` produces `pending-upload` and no eviction permit.
- Goal completion gates remain false until their corresponding evidence exists.
- Filename dates can place a candidate in a provisional archive period, but never authorize automatic transfer or eviction.
- A personal native-client copy may proceed without OAuth quota evidence only while the matching desktop client is observed running; provider sync attestation still gates eviction.
- Managed File Provider roots are never recursively scanned; the explicit incomplete-scan blocker is non-overridable.
- Worktree audits stop on provider-managed parents or dataless Git metadata; stale-worktree removal
  is never inferred from a materialization wait.
- A timed-out provider-wide dump may explain active transfer or reconciliation markers, but its incomplete evidence never admits a new copy.
- A provider-wide `errno 28`/disk-full marker is retained as
  `provider-global-sync-local-disk-full`; it blocks new copies until local headroom is restored.
- A provider-wide File Provider `-1005 itemNotFound` marker is retained as
  `provider-global-sync-item-not-found`; it is an error blocker even when the provider reports
  active transfer or reconciliation progress.
- Every cloud plan attempts to persist a redacted local-volume snapshot for incident comparison;
  a persistence failure is surfaced as `local-volume-evidence-persistence-failed` and does not
  grant or revoke transfer authority by itself.
- Every cloud plan attempts to persist a path-free provider-client process snapshot for incident
  comparison; a persistence failure is surfaced as
  `provider-client-runtime-evidence-persistence-failed` and does not grant or revoke transfer
  authority by itself.
- A macOS copy helper timeout is a failed copy, not a partial success: the destination is removed,
  the source is retained, and provider attestation cannot begin until a fresh plan is made.
- An iCloud native `needs-sync-up` or `needs-sync-down` state blocks new-copy admission until the
  bounded native status is quiet; neither direction is treated as completed provider evidence.
- A timeout while collecting the bounded iCloud native status also blocks new-copy admission;
  timeout is not interpreted as a quiet provider.
- The bounded iCloud File Provider activity probe records only redacted aggregate evidence: counts
  of `no progress` fetch/create markers and active upload/download progress fractions. Any such
  marker, an active transfer, a probe timeout, or unavailable probe evidence blocks new-copy
  admission; no path, filename, item identifier, or content is retained.
- When iCloud evidence is blocked or unavailable, the UI backs off automatic File Provider probes
  to five minutes; a quiet provider returns to the normal one-minute refresh. This prevents
  DiskSage from adding repeated database readers while preserving fail-closed copy admission. An
  operator may explicitly trigger one bounded read-only refresh; this never restarts `bird`, writes
  cloud data, or authorizes source eviction.
- An oversized CloudDocs `client.db` produces incomplete, fail-closed evidence without running a
  long SQLite fallback query; the File Provider probe still reports whether the provider is stalled.
- A provider-client recovery request can restart only the fixed user-space OneDrive or Google Drive
  bundle. A missing post-restart process observation remains a blocker, and recovery never changes a
  receipt, writes cloud data, or authorizes source eviction. iCloud system services remain untouched.
- Podman reclaim evidence reports the VM/store candidates and requires separate human review for
  unused images or volumes. The executable image-cleanup boundary revalidates the exact reviewed
  candidate fingerprint immediately before mutation and removes only those immutable image IDs with
  `image rm --no-prune`; it never invokes broad `image prune`, never uses `--force`, never removes
  parent images, containers, volumes, tagged candidates, or the VM, and host-space recovery is
  reported only from before/after filesystem observations. User-data volumes are not moved to a
  provider or deleted by inspection.
- A `source-not-present`, `source-content-not-local`, or unsafe-source observation blocks the Goal
  even when provider sync is complete; DiskSage never infers that an externally removed or
  File-Provider-dataless source was safely evicted.
- `eviction-ready` permits only the separately approved, reversible OS-Trash operation.
- A stale projection is replaceable state and must be reconciled against immutable evidence.
- Ontology-based local organization uses the same lineage precedence as cloud planning (embedded
  metadata, explicit filename date, filesystem creation time, then modification time). Its move
  plan carries a path-free lineage fingerprint plus the source size/mtime snapshot and is rejected
  if the source changes; File Provider dataless sources are not moved. The organization walk is
  bounded at 10,000 entries or 10 seconds and rejects partial results.
- A complete organization plan can be exported as `disksage.organization-lineage-batch` v1. The
  handoff contains only lineage fingerprints, size/mtime, production-time evidence, ontology class,
  `targetFolder`, and the planned `move` action. Naruon stores it encrypted and returns a redacted
  summary; it never receives paths, names, OAuth material, or move/eviction authority.

## Standards references

The lineage vocabulary is aligned with PROV-O's Entity/Activity/Agent and derivation relations;
catalog exports use DCAT 3 concepts for datasets, distributions, checksums, and versioning. These
standards describe interchange semantics only; they do not grant cloud-write or source-eviction
authority. The APA 7 records and original URLs are kept in the Zotero Local API manifest.

- Lebo, T., Sahoo, S., & McGuinness, D. (Eds.). (2013). *PROV-O: The PROV ontology*. W3C
  Recommendation. https://www.w3.org/TR/prov-o/
- Albertoni, R., Browning, D., Cox, S. J., Gonzalez Beltran, A., Perego, A., & Winstanley, P.
  (Eds.). (2024). *Data Catalog Vocabulary (DCAT) - Version 3*. W3C Recommendation.
  https://www.w3.org/TR/vocab-dcat-3/
