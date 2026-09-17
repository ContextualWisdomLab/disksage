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

Cloud-provider OAuth consent is read-only by default in the personal desktop UI. A user must
explicitly enable the write-access checkbox before an OAuth connection may request upload scope;
the default path does not prompt for organization credentials or imply API upload authority.

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
The same incident reclaimed only generated local artifacts; `.codegraph` indexes are now included
in the bounded development-artifact inventory and identity-checked OS-Trash path, never in cloud
offload or provider-managed cleanup.
On 2026-08-21, a temporary DiskSage review worktree was first checked against the live process
table and current worktree registry, then removed with `git worktree remove` because no process
held it. This reclaimed the worktree's generated build state and raised APFS headroom from 133 MiB
to 4.3 GiB; unrelated repository worktrees remain untouched.
The provider UI keeps a path-free blocker fingerprint across bounded observations and escalates
the operator guidance after 15 minutes of the same blocker; this is advisory state only and never
authorizes a cloud write, provider restart, or source eviction.
The latest bounded observation still reports the same transfer/reconciliation and `-1005`
missing-item cohort with only about 1.2 GiB of local headroom; a Homebrew cleanup dry-run found no
reclaimable entries, so DiskSage keeps the Finder copy cancellation and provider-quietness gate in
force rather than inventing a local cleanup authority.
The latest iCloud File Provider read-only probe also exposed an active upload at fraction `0.9524`
(28,124,151,529 of 29,530,341,516 bytes) and an active download at fraction `0.0000`
(0 of 1,066,167,994 bytes), with scheduling still `running` and error generation `1143`. The
bounded probe did not finish within its wall-clock limit, so this partial observation is retained
as incomplete evidence and cannot admit a retry. CloudArchive now fingerprints the iCloud blocker
and transfer progress across refreshes, reports the duration, and escalates the same-blocker
guidance after 15 minutes; this remains advisory and never cancels Finder or restarts `bird`.
The one-minute reconciliation loop may refresh provider state and replaceable projections, but
immutable per-item evidence history is bounded to 128 records per receipt. The Naruon readiness
validator also mirrors the iCloud health contract for active File Provider upload/download
progress, exporting a blocked envelope instead of failing validation.

### Finder cancellation for third-party provider stalls (2026-08-21)

The bounded third-party File Provider dump remains the source of truth for a stuck Finder copy.
In the current incident, `fileproviderctl dump com.google.drivefs.fpext -l` returned a complete
read-only observation with Google Drive temporarily disconnected, active upload and download
progress, a 2,000-entry reconciliation backlog, and File Provider error `-1004`; the local
volume had about 6.9 GiB available. These markers produce the existing
`provider-global-sync-*` blockers. CloudArchive now exposes the same fixed Finder Escape
cancellation action for those blockers, then refreshes the provider-global observation; it does
not restart `fileproviderd`/`bird`, write cloud data, or authorize attestation or source eviction.
The cancellation remains an operator-action receipt only, and a fresh clear observation is still
required before a retry.

The follow-up read-only observation at `2026-08-21 04:31:47 +0900` retained only aggregate evidence
from the bounded iCloud dump: 97 `createItemBasedOnTemplate` and 46 `fetchContentsForItemWithID`
requests reported `no progress`, with no upload/download progress marker in the retained output.
The local volume subsequently measured about 2.3 GiB available while `bird` and `fileproviderd`
were active. This confirms the Finder “preparing” symptom is a provider-stall signal, not a safe
copy completion; DiskSage keeps the source, cancels only through the operator-visible Finder
control, and never treats the probe or process activity as write, attestation, or eviction authority.

The next bounded read-only observation at `2026-08-21 04:59:17 +0900` retained 125 aggregate
`no progress` fetch/create markers, an active upload at `0.9524` (28,136,385,681 of
29,543,186,689 bytes), and an active download at `0.0000` (0 of 1,060,097,218 bytes), while
the provider scheduler remained `running` with error generation `1143`. Local APFS headroom was
about 3.9 GiB. The coexistence of a large active upload, a zero-progress download, and repeated
no-progress requests explains the Finder “preparing” stall; it does not authorize a process kill,
copy retry, cloud mutation, or source eviction. DiskSage therefore keeps the Finder cancel-only
guidance and fail-closed admission until a fresh bounded observation is quiet and complete.

The provider-evidence authority boundary was also tightened at source fix `b9fe4f0`: lookup of an
API object identifier now rejects group- or other-writable evidence directories before reading any
record. This is a fail-closed integrity check only; it neither deletes evidence nor changes cloud
or source-eviction authority.

The bounded follow-up probe at `2026-08-21 05:42 +0900` observed at least 71 fetch and 161 create
requests marked `no progress` in five seconds. Because `fileproviderctl` exposes no supported
cancellation operation, the product surfaces the Finder cancel control as the only operator action;
it must not kill `fileproviderd` or `bird`, write a raw provider dump, retry the copy, or infer
eviction authority. This incomplete observation keeps new-copy admission, attestation, and source
eviction fail-closed and is bound to source head `f0dff03`, not to this replaceable ADR text.

At `2026-08-21 06:25:37 +0900`, a read-only `fileproviderctl dump -l 20` observation returned
progress markers and old File Provider `itemNotFound` errors while the Finder `real_datasets`
operation remained in “preparing”. The bounded dump was written to a temporary file and removed;
raw provider output, paths, item identifiers, and contents were not retained. APFS available space
was only 289 MiB (98–100% reported on the data volume), so DiskSage did not retry the copy or
interpret the provider activity as completion. After confirming no npm/uv/cargo cleanup operation
was active, only regenerable package/tool caches were removed, recovering approximately 1.6 GiB;
the subsequent bounded observation fluctuated between 1.7 and 1.9 GiB. The Cargo source cache,
CloudDocs databases, File Provider data, user files, active processes, Finder operation, and all
cloud objects were retained. Reproducible caches are not uploaded to a provider during an incident.
This evidence is bound to source head `e71ecd13e8c91acf10093271fd58414cae5fe349`; the Finder cancel
control remains the only supported cancellation action.

The current-head follow-up at `2026-08-21 08:21 +0900` found the Finder target labelled
`real_datasets` on the local volume, containing 14 ZIP files totalling about 7.2 GiB while the
volume reported about 2.3 GiB available. No process held an open handle below that target during
the bounded check. CloudDocs logs retained user-initiated download operations that had run for
about 5,535 seconds before ending as cancelled with `CKInternalError`; the default route was
`utun4`, while bounded HTTPS checks to iCloud, Apple, and Google endpoints still completed. This
is incomplete provider-transfer evidence, not a successful copy or a DiskSage process failure:
the source remains retained, the Finder cancel control remains the only supported cancellation
action, and local headroom plus a fresh quiet provider observation remain required before retry.

The latest read-only incident check at `2026-08-21 11:19 +0900` reproduced the same symptom: a
bounded `fileproviderctl dump` did not complete within 15 seconds and returned only partial output,
while the system log recorded repeated `no progress` fetch/create requests, materialization
failures, and file-coordination failures. `bird` and `fileproviderd` were busy during the check.
This is provider-side transfer/reconciliation stall evidence, not proof that the Finder copy
completed. DiskSage sent the fixed Escape cancellation request, which exited successfully; Finder
and provider daemons were left running, and no source, cloud object, or provider database was
mutated. Admission remains fail-closed until a fresh complete quiet observation is available.

At source head `6b9cd694ac9d34e8abc40de47b2ec1106ec55d90`, historical provider evidence remains
readable through the compatibility parser, but an evidence record with `sync_complete=true` and
an explicit `sync_state=unknown` is rejected at the current authorization boundary. It therefore
cannot produce a provider-sync confirmation or an eviction permit; the regression is covered by
the real public-boundary test `provider_sync_legacy_eviction_fail_closed.rs`.

Direct credential-bearing configuration files are now a first-class `sensitive-config` inventory
kind. Filename-only markers (`.env`/`.env.*` except examples, credential names, private-key names,
and key/certificate extensions) are collected for visibility without opening their contents, then
blocked at the shared planner boundary as `sensitive-config-file`. They never enter metadata
probing, cloud-copy approval, potentially-reclaimable-byte totals, or source eviction. This is a
name-based safety boundary, not a claim that every secret-bearing file can be recognized. The
runtime Goal vocabulary records the same blocker under `blocked_source_classes`, so a replaceable
Goal projection cannot silently omit the protection.

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
- A legacy provider record may be read for compatibility and re-check purposes, but an unknown
  explicit sync state never authorizes provider confirmation or source eviction, even when the
  legacy completion bit is true.
- Credential-bearing configuration names are visible as blocked `sensitive-config` candidates,
  but their contents are never opened and they cannot be copied, counted as reclaimable, or
  evicted automatically; unrecognized secret material remains an explicit review limitation.
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
- The bounded lineage probe count is aligned to the export limit of 200 items, so a realistic
  multi-file organize plan cannot contain unmaterialized default lineage after planning.

## Amendment: receipt-scoped API locator recovery (2026-08-21)

The persisted API object identifier is a locator hint, not independent transfer authority. Its
lookup must filter the immutable evidence directory by the exact receipt-id filename prefix before
reading records; an unrelated receipt must never consume a global directory scan window. Valid
records are retained per receipt (currently 128), and the lookup scans the complete matching prefix
so a temporarily interrupted retention pass cannot hide a valid locator. The remote response,
destination binding, local hash, and normal provider-sync gates remain mandatory. This amendment is
implemented at source head `9222558b4346d1a6be30ef17645f43124e1232e1` with a regression case that
places 4,096 unrelated records before the target record.

When a bounded iCloud health observation reports a stalled or active File Provider transfer, the UI
may request Finder cancellation through one fixed macOS AppleScript that activates Finder and sends
Escape. The command accepts no path, script, or process identifier, uses a five-second timeout, and
never kills `bird`, `fileproviderd`, a provider client, a cloud object, or a source file. A successful
request is only an operator-action receipt; a fresh quiet provider observation is still required
before any copy, attestation, or source eviction. This action is implemented at source head
`df097743eb75b9cc919d631db0ebdeffad8b7995`, with a regression test that preserves the newline
separator between Finder activation and the System Events Escape command.

## Amendment: ontology-bound orphan cache cleanup (2026-08-21)

DiskSage now exposes a bounded macOS orphan planner as an operator action. It compares installed
application bundle identifiers with directory names under the user's `Library/Caches` and
`Library/Application Support`, and records only metadata, deterministic fingerprints, and
path-free ontology relations. It never reads file contents, follows symlinks, or treats an LLM
label as authority. `Application Support`, incomplete manifests, incomplete installed-app
inventories, and active-use evidence remain review-only.

Only a fully scanned, unused cache candidate can be selected for the separate `trash-orphan-cache`
action. The backend re-plans immediately before mutation, requires the exact plan fingerprint and
approval phrase plus an audit rationale, revalidates candidate metadata, and moves it through the
existing reversible OS Trash boundary. Cloud providers, File Provider state, source files, and
Trash contents are never mutated by the planner. A stale plan or missing active-use evidence fails
closed.

The orphan implementation introduced at `3d2406c`, with subsequent provider-sync and cleanup-refresh safety fixes, also treats an incomplete installed-application inventory or
metadata manifest as non-authoritative. Directory-iteration errors and recursion-depth limits are
recorded as incomplete evidence, so an unvisited subtree can never make a cache eligible for
automatic Trash movement; the focused macOS Rust safety tests cover both bounded scans.

The planner's installed-application traversal now combines bounded fixed-root and Launch Services
(`mdfind`) bundle inventory, shares the five-second plan deadline, caps Info.plist reads before
parsing, and uses recursive `lsof +D` evidence for directory candidates;
timeouts, read failures, and active-use errors remain review-only. The replaceable Goal names the
actual Tauri commands `plan_orphan_cleanup` and `clean_orphan_candidates`, so operator automation
cannot drift from the registered command boundary.

The Launch Services `mdfind` probe also runs in a private process group; timeout cleanup kills the
group before the bounded stdout reader is joined, so a descendant cannot hold the planner past its
deadline.

The active-use probes share the enclosing planner deadline rather than starting an independent
timeout per candidate. Immediately before a Trash batch, existing candidate directories are
re-scanned against the reviewed metadata manifest; a changed, incomplete, or unsafe manifest
fails the whole batch before the first mutation. This remains metadata-only: cache contents are
never opened and no File Provider placeholder is materialized.

The cleanup mutation result is authoritative once the OS Trash operation succeeds. A follow-up
read-only plan refresh is deliberately separate: if it fails, the UI preserves the successful
cleanup receipt, clears the stale selection, and asks the operator to re-run the relationship
inspection instead of reporting a completed mutation as a failed cleanup.

## Amendment: repeated provider-stall evidence remains blocking (2026-08-21 12:35 +0900)

A fresh bounded read-only Google Drive File Provider dump still reported temporarily disconnected
domains, File Provider `-1004` server-unreachable errors, simultaneous upload/download progress,
and reconciliation queues of 14,558, 2,000, 201, and 168 entries. `bird` and `fileproviderd`
were CPU-active. This repeated observation keeps the provider-global transfer, reconciliation,
disconnect, and server-error blockers authoritative; a Finder “준비 중” dialog is not copy
completion. DiskSage must offer only the fixed bounded Finder Escape cancellation and a later fresh
quiet observation, never a daemon kill, cloud mutation, retry, or source eviction.

## Amendment: iCloud active-transfer observation (2026-08-21 13:38 +0900)

A capped read-only iCloud File Provider dump observed live Finder enumerators, `scheduling state:
running`, upload progress of 95.24% (118,950,548,354 of 124,897,444,934 bytes), and download
progress of 2.78% (30,311,669 of 1,091,221,225 bytes). This bounded head is not a quiet provider
attestation. The observation command was stopped without changing a provider daemon, cloud object,
or source file; raw cloud-placeholder scans remain prohibited because metadata inspection can
request materialization.

## Amendment: provider-runtime recovery evidence (2026-08-21)

OneDrive and Google Drive client recovery now requires an explicit runtime observation before
accepting a quit, graceful termination, or post-restart state. An unavailable observation is a
distinct blocker (`provider-recovery-runtime-evidence-unavailable` or
`provider-client-runtime-evidence-unavailable-after-restart`); it is never coerced to process
absence and never authorizes a retry, cloud mutation, or source eviction. This fail-closed boundary
is implemented at source head `ac299095854f4cd16f124a2b5dcb44023d8fffe5`, with regression coverage
for unavailable post-restart evidence.

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

- Chandramouli, R., & Pinhas, D. (2020). *Security guidelines for storage infrastructure* (NIST
  SP 800-209). National Institute of Standards and Technology.
  https://doi.org/10.6028/NIST.SP.800-209
- Buneman, P., Khanna, S., & Tan, W.-C. (2001). Why and where: A characterization of data
  provenance. In A. D. Bossi (Ed.), *Database Theory — ICDT 2001* (pp. 316–330). Springer.
  https://doi.org/10.1007/3-540-44503-X_20

## Amendment: iCloud File Provider materialization stalls (2026-08-21)
A bounded, read-only iCloud observation at `2026-08-21 14:32:54 +0900` timed out while retaining
58 `fetch` and 114 `create` requests marked `no progress`. The contemporaneous system log
also recorded extension termination after the stalled requests and materialization failures
with staged items missing. These are provider-reconciliation evidence only: they do not prove
a completed destination copy, cloud durability, remote capacity, or source-eviction authority.

The File Provider activity evidence schema is therefore version 3. DiskSage stores only aggregate
counts (`materialization_failure_count`, `staged_item_missing_count`, and sync-exclusion counts)
and redacted notices;
raw paths, filenames, item identifiers, contents, and provider dumps are not persisted. Either
materialization failure or staged-item loss adds `icloud-file-provider-materialization-failed`
to the new-copy admission blockers. Copy, attestation, and local eviction remain fail-closed
until a fresh bounded observation is complete and quiet. This behavior is implemented at source
head `0ded557893191606ff6f91d4303fb54d5112fe45` and covered by parser, readiness, integration,
and frontend tests.

## Amendment: headless readiness and copy race hardening (2026-08-21)

The headless `disksage-cloud-plan --export-naruon-copy-readiness` path now constructs the same
three-stream pre-copy cohort as the GUI path before exporting iCloud readiness. A missing or
incomplete volume, runtime, or native-health stream remains an explicit blocker. Provider
attestation retention never removes the newly fsynced record when pruning fails; maintenance is
retried on the next reconciliation pass. macOS native copy uses `/bin/cp -n` and never removes a
destination after a failed copy because a concurrent File Provider object may own that path.
Native-status early termination requires only the stable client/server/sync fields; optional
`last-sync` remains parsed evidence but is not a latency gate.

## Amendment: fresh headless materialization-stall receipt (2026-08-21)

At `2026-08-21 17:07:34 +0900`, the exact-head `disksage-icloud-sync-health` binary completed a
bounded read-only observation with `schema_version=5`, incomplete evidence, no native status,
and a timed-out File Provider dump. The redacted activity aggregate retained 85 fetch and 144
create requests marked `no progress`, with no active upload or download progress. New-copy
admission therefore remains blocked by `icloud-sync-health-evidence-incomplete` and
`icloud-file-provider-no-progress`; this is provider-reconciliation evidence, not proof of a
completed copy, remote durability, or eviction authority. The observation records the stale
Finder copy symptom without killing Finder, `bird`, `fileproviderd`, or touching provider data.

## Amendment: child-owned macOS copy staging and OneDrive runtime evidence (2026-08-21)

The native macOS copy boundary now creates a private `tempfile` directory inside the validated
destination parent, copies into its `payload` with bounded `/bin/cp`, rechecks source identity and
digests, and finalizes with bounded `/bin/mv -n`. The staging directory is owned by the command and
is dropped on timeout or helper failure; a provider-owned final destination is never removed by
failure cleanup. The successful path still writes the immutable copy receipt before any later
attestation or eviction decision. This hardening is bound to source head `3704dd1`.

A bounded OneDrive observation during the Finder `real_datasets` incident reported
`temporarily disconnected` (the desktop client was not running), active upload/download markers,
`databaseInitError`, and root reconciliation failures with File Provider `-1004`
(`serverUnreachable`). Copy admission remains blocked. Starting the client while only 340 MiB was
available was stopped immediately; no Finder, `bird`, `fileproviderd`, provider object, or source
file was mutated. A fresh quiet provider observation and safe local headroom remain required.

## Amendment: paired projection readers use the receipt lock (2026-08-21)

ADR and Goal projection writers already serialize the pair under a receipt-scoped interprocess
lock. Readers now acquire the same lock before reading both files, so reconciliation cannot observe
the interval between the two atomic replacements and mistake a transient mismatch for current
state. The lock file is bounded coordination metadata; immutable receipts and provider evidence
remain authoritative, and no read path grants copy or eviction authority. This is implemented at
source head `9cf9665a9041b8b00a66b195f1236c8683f8a951` and covered by the existing paired-writer
and projection-state tests.

## Amendment: OneDrive reconciliation remains incomplete after headroom recovery (2026-08-21)

A fresh bounded File Provider observation at `2026-08-21 20:07:09 +0900` found 39 GiB of local
headroom, but the OneDrive personal domain still required indexing with 77,393 pending indexable
items and 81,524 reconciliation entries. Upload and download progress remained active; the
provider database history retained SQLite error 11 (`databaseInitError`), and the File Provider
domain was observed through `fileproviderd`/ExtensionKit rather than a quiet provider state. This
shows that low disk pressure was not the only cause of the Finder stall. DiskSage must continue to
report provider-sync-incomplete and must not attest, evict, or treat the Finder preparation dialog
as a completed cloud copy until a fresh complete, quiet observation and immutable receipt exist.

## Amendment: iCloud sync-exclusion evidence for Finder preparation stalls (2026-08-21)

A fresh bounded iCloud File Provider dump recorded active upload and download progress together
with 18 `Excluded From Sync Due To Filename` and 2 `Excluded From Sync Under Root` errors.
These are aggregate, path-free provider evidence: DiskSage records only the two counts and redacted
notices, never filenames, item identifiers, or raw provider output. Either count adds a dedicated
new-copy admission blocker (`icloud-file-provider-filename-excluded` or
`icloud-file-provider-root-excluded`) in addition to any transfer or materialization blocker.
The Finder preparation dialog therefore remains an incomplete provider operation, not a successful
copy receipt, and copy, attestation, and eviction stay fail-closed until the provider is quiet.

## Amendment: current iCloud indexing and transfer receipt (2026-08-21 22:22 +0900)

A fresh bounded read-only `fileproviderctl` observation completed at `2026-08-21 22:22:53 +0900`.
The path-free aggregate reported `needs-indexing=no`, `pending-indexable-count=13,737`, a
12,449-entry reconciliation backlog, one active upload marker, one active download marker with
99.16% observed progress, one no-progress fetch, and 18 filename plus 2 root sync exclusions.
The parser retained `icloud-file-provider-indexing-pending`, transfer, no-progress, and exclusion
admission blockers; the bounded dump was truncated and no mutation was performed. This is still
provider-sync-incomplete evidence: the Finder preparation dialog is not a completed copy receipt,
and native copy, attestation, and eviction remain blocked.

## Amendment: bounded planning and attestation-retention edge cases (2026-08-21)

Exact-content duplicate clusters are now computed before the presentation `limit` is applied. A
duplicate pair split across that limit therefore still marks the visible member for canonical
selection and remains represented in the path-free cluster summary; the limit controls presentation,
not safety evidence. The retention pass also protects the just-written immutable provider record
when its clock is older than the existing bounded history, so clock regression cannot delete the
attestation that was just persisted. Both boundaries remain fail-closed and are covered by focused
Rust regression tests. The local implementation is `c5aa3a1`; its protected-branch publication is
still pending the repository ruleset's normal PR workflow.

## Amendment: iCloud indexing backlog is an admission blocker (2026-08-21)

A repeated read-only File Provider observation remained unchanged for 21 seconds: the provider
reported `pending-indexable-count=12474` and upload progress `0/5038` at `0.0000`, alongside the
existing 18 filename and 2 root exclusions. DiskSage now retains this aggregate count in the
path-free activity evidence and adds `icloud-file-provider-indexing-pending` to new-copy blockers;
the UI includes it in the stable-block fingerprint and warns that Finder may remain in “복사 준비
중”. No Finder/provider process is killed and no cloud or source mutation is performed. The
extension remains additive to activity schema v3 and is covered by a parser regression test.

## Amendment: restart-safe iCloud admission duration (2026-08-21)

Each successful iCloud health inspection now persists a bounded, path-free observation before
deriving `admission_blocked_since_ms` from the earliest contiguous retained record with the same
admission-blocker set. Invalid, unreadable, or changed historical evidence stops the walk, so a
restart cannot manufacture a longer stall interval. The field is diagnostic only: provider-native
completion, copy receipts, attestation, and local eviction remain independent fail-closed gates.
The implementation and regression test are at source head `ad850e9`.

The Naruon readiness allow-list now includes `icloud-file-provider-indexing-pending`, keeping the
provider-derived blocker set closed under export and rejecting the same blocker on non-iCloud
envelopes. The binding repair is at source head `6f95ca3`.

## Amendment: bound active-use probes without touching provider state (2026-08-22)

The exact-head macOS fix `a6ec6e2` starts the bounded `lsof` active-use probe in its own Unix
process group. On timeout, the group is killed before bounded stdout/stderr readers are joined,
so a shell wrapper or descendant cannot keep a pipe open and starve the independent `ps` probe.
Only the command group created for the diagnostic is terminated; Finder, `bird`, `fileproviderd`,
File Provider databases, cloud objects, and user files remain outside the mutation boundary. The
focused Rust regression test passed 3/3. A timeout remains incomplete active-use evidence and
keeps cache cleanup and cloud eviction fail-closed; this process-group cleanup is not a provider
recovery or copy-cancellation operation.

## Amendment: keep the readiness verifier boundary testable (2026-08-22)

The shipped Naruon readiness verifier uses a plain source comment rather than a crate-inner doc
comment so the same parser can be included by its integration boundary test module. This is a
compile-boundary repair only; the verifier's path-redacted output and readiness authority do not
change.

## Amendment: exact numeric disk-full markers (2026-08-21)

Provider-global File Provider parsing now applies numeric-boundary matching to `errno 28`,
`odresult_errno 28`, and `OSStatus -34` in addition to the existing `code=28` forms. Longer
values such as `errno 280` and `OSStatus -340` remain ordinary provider errors and cannot create
the actionable local-disk-full blocker. The focused regression covers both exact and extended
markers; this parser remains diagnostic evidence only and does not grant copy, attestation, or
eviction authority.

## Amendment: live Finder preparation stall receipt (2026-08-21 23:30 +0900)

The exact-head headless iCloud health probe completed a bounded read-only observation with complete
evidence. macOS reported `needs-sync-up` and `needs-sync-down`; File Provider reported one
no-progress fetch, active upload/download progress (`953100`/`988500` millionths), 17,547 pending
indexable items, 18 filename exclusions, and two root exclusions. The CloudDocs upload queue retained
six items blocked on sync-up. New-copy admission is therefore `blocked`. These aggregate facts explain
why Finder can remain at “복사 준비 중”, but they do not identify or attest the seven displayed items.

DiskSage records only bounded, path-free counters and exposes the existing Finder cancellation
request. It does not kill `fileproviderd`, `bird`, or Finder, modify CloudDocs/provider state, or
convert this observation into copy, attestation, or eviction authority. A complete quiet observation
and independent per-item provider evidence remain required. This evidence was observed while PR #246
was at `fc9f4a4c465fc5ef355f7fbf552ff4295cf4f609` and PR #247 at
`45214018dff43c6ba7c71253bc50e8c0eab0e1bd`; hosted checks remain authoritative.

## Amendment: detect macOS File Provider disk import during Finder preparation (2026-08-22)

A bounded read-only iCloud File Provider dump can report `disk import: yes` while Finder remains
in “복사 준비 중”. DiskSage now retains only the boolean aggregate as the
`icloud-file-provider-disk-import-active` notice, derives the same new-copy admission blocker,
and surfaces it with the existing fixed Finder-cancel action. The notice contains no path,
filename, item identifier, or raw provider output. Disk import is provider-progress evidence, not
a copy receipt; copy, attestation, and source eviction remain fail-closed until a complete quiet
observation and independent per-item evidence exist.

## Amendment: isolate third-party stall clocks and preserve prior onset on journal faults (2026-08-24)

One shared path-free `provider-global-sync-evidence` journal can contain interleaved OneDrive and
Google Drive observations. The restart-safe onset walk therefore ignores valid records belonging to
another provider instead of treating them as a blocker transition. Read, parse, integrity, or
incomplete-record failures stop the walk while retaining the onset already accumulated from newer
valid records; they can never manufacture a longer duration. Rust regressions cover both an
interleaved provider record and a malformed older record. This is diagnostic continuity only: it
does not grant copy, attestation, cloud mutation, or source eviction authority.

## Amendment: unchanged iCloud preparation queue after restart (2026-08-22 04:05 +0900)

A subsequent bounded, read-only observation found the same iCloud File Provider aggregate state:
`pending-indexable-count=31,024`, upload progress `5,202,024,494/5,462,125,152` (95.24%),
download progress `0/828`, and `disk import: yes`. The native sync summary still reported
`needs-sync-up`/`needs-sync-down` with the last sync at `2026-08-21 20:20:10.166 +0900`; multiple
items remained in `pending-scan` for roughly three or more hours. The unchanged counters are
provider-stall evidence, not a per-item receipt and not proof that the visible Finder operation
completed. DiskSage performed no Finder cancellation, daemon restart, CloudDocs/provider-database
write, cloud mutation, materialization, or source mutation. New copy, attestation, and eviction
therefore remain fail-closed; the existing bounded Finder-cancel action remains operator initiated.

## Amendment: current protected PR inventory is evidence-bound (2026-08-22 04:23 +0900)

The product baseline records the exact protected PR queue at DiskSage head `dac324d` (PR #247).
That inventory is operational evidence only: each row binds its own head SHA, and a later push
invalidates predecessor checks and approvals. A clean mergeable flag, bot comment, or queued review
never authorizes a cloud copy, provider attestation, source eviction, or protected merge. The
review loop remains exact-head review → repair → checks → qualifying approval → normal protected
merge; no provider, Finder, cloud, or user-file mutation was performed for this amendment.

## Amendment: preserve third-party Finder-stall duration across restart (2026-08-24)

The screenshot-level symptom “복사 준비 중” can outlive both Finder and DiskSage. The existing
third-party provider-global probe now stamps each bounded OneDrive/Google Drive observation and
persists a path-free `ProviderGlobalSyncEvidenceSnapshot` under
`provider-global-sync-evidence`. Create-only, SHA-256-fingerprinted records are capped at 64 KiB,
stored as `0400` files in a `0700` directory, and retained at most 128 records. Invalid, tampered,
incomplete, or unsafe records cannot extend a blocker interval.

When the same provider, state, aggregate transfer/indexing flags, and stable blocker set are seen
again, the command returns `admission_blocked_since_ms`; CloudArchive uses it as the third-party
stall-clock origin after an application or system restart. The persisted clock is diagnostic only:
it does not cancel Finder, restart a provider, write cloud data, attest an item, or authorize source
eviction. A provider-global `-1004`/disconnect, active transfer, reconciliation backlog, or local
disk-full marker therefore remains a visible fail-closed blocker until a fresh complete quiet
observation and independent per-item evidence exist.

## Amendment: executed iCloud admission evidence for the current Finder stall (2026-08-24 12:39 +0900)

The exact-head `disksage-icloud-sync-health` binary completed a read-only CloudDocs/WAL snapshot
with complete evidence. It observed `needs-sync-up|needs-sync-down`, 343 uploads blocked on
sync-up, one active upload at 95.24%, one active download, and 58,183 pending indexable items.
New-copy admission is `blocked` for transfer activity, disk import, indexing backlog, root and
filename exclusions, and native sync-up/down state. The report explicitly retains
`provider_sync_attested=false`, `local_eviction_authorized=false`, and `mutation_performed=false`.

This is global provider evidence explaining Finder's “복사 준비 중” state, not a per-item cloud
receipt. The probe reads a copy-on-write snapshot including WAL files, redacts paths, and never
writes CloudDocs/provider state. Copy, attestation, and source eviction remain fail-closed until
quiet provider evidence and independent per-item receipts exist.

## Amendment: reject impossible persisted stall-onset values (2026-08-24 12:51 +0900)

CloudArchive treats the persisted blocker onset as diagnostic input, not trusted authority. It now
accepts that value only when it is a safe, non-negative integer no later than the backend observation;
negative, future, non-finite, and unsafe-integer values fall back to the current observation. This
prevents malformed history from suppressing a prolonged Finder “복사 준비 중” warning while keeping
copy, attestation, cloud-write, and source-eviction authority fail-closed.

## Amendment: latest iCloud preparation queue remains blocked (2026-08-24 12:58 +0900)

The latest exact-head read-only probe still reports `new_copy_admission_state=blocked` and
`mutation_performed=false`. Native iCloud remains `needs-sync-up|needs-sync-down`; 343 uploads are
blocked on sync-up, one upload and one download are active, and pending indexable items increased
to 64,969 from 58,183 at 12:39. Disk import, transfer activity, and the filename/root exclusions
remain present. This aggregate provider evidence explains the Finder preparation stall but is not a
per-item receipt, so copy, attestation, cloud-write, and source-eviction authority remain
fail-closed; DiskSage performs no Finder, provider, source, or cloud mutation.

## Amendment: iCloud indexing backlog increased during the Finder stall (2026-08-24 13:04 +0900)

A subsequent exact-head read-only probe observed the same `needs-sync-up|needs-sync-down` native
state, 343 uploads blocked on sync-up, one active upload at 95.24%, one active download, and
`new_copy_admission_state=blocked`. FileProvider pending indexable items increased from 64,969 to
67,017 while disk import, transfer activity, and the filename/root exclusions remained present.
The evidence is aggregate and `provider_sync_attested=false`, `local_eviction_authorized=false`,
and `mutation_performed=false`; therefore it cannot attest the seven Finder items or authorize any
copy, cloud write, or source eviction.

## Amendment: iCloud backlog continues to grow without a DiskSage database handle (2026-08-24 13:12 +0900)

The next exact-head read-only probe observed `pending_indexable_count=74,946` (up from 67,017),
the same native `needs-sync-up|needs-sync-down` state, 343 uploads blocked on sync-up, one active
upload at 95.24%, one active download, and `new_copy_admission_state=blocked`. Finder's
`real_datasets` destination remained 512 bytes with the same 2026-08-20 03:28:07 mtime and the
root had about 99 GiB available. Bounded process inspection found `fileproviderd` using 72–129% CPU,
but no DiskSage process, CloudDocs database, or source path was open in that process. This supports
a provider-side reconciliation/indexing backlog, not a proven DiskSage database lock. The probe
still reports `provider_sync_attested=false`, `local_eviction_authorized=false`, and
`mutation_performed=false`; no Finder, provider, source, or cloud mutation is authorized.

## Amendment: preview headroom follows the destination staging filesystem (2026-08-24 13:22 +0900)

The planner previously exposed source-volume pressure while the native mutation gate correctly
probed the destination staging ancestor. That could make a cross-volume preview disagree with the
actual copy boundary. The planner now performs the same bounded destination probe for each visible,
otherwise-unblocked candidate and emits `local-volume-headroom-insufficient` or
`local-volume-headroom-unverified`; the native UI gate follows those notices, while explicit
provider-API uploads remain a separate path. The source-volume snapshot remains diagnostic only.
Pinned Rust tests, the destination-headroom contract, the full frontend suite (134 tests),
`svelte-check`, and frontend coverage all pass; mutation, attestation, and eviction authority are
unchanged and still fail closed.

## Amendment: reconcile cwd-relative organize targets with current main (2026-08-24 18:28 +0900)

PR #225 exact head `ea6f82d914e4660319600acb614fccb4a701aec1` now contains the current `main`
lineage-aware organization contract and the original fail-closed target fix. `resolve_target_folder`
expands only an exact `~` or leading `~/` against an absolute home path; process-cwd-relative,
named-user tilde, parent/root traversal, and relative-home targets are rejected. Organization plans
retain embedded-metadata-first production evidence (filename tokens such as `2026-04-28`/`251210`
remain secondary), source size/mtime, and a lineage fingerprint; source drift is rechecked before
execution.

The exact-head local proof is 21 `organize::tests`, one Windows home-resolution contract test,
`actionlint`, and `git diff --check`. Hosted checks were restarted for this head and remain
authoritative; the draft PR has no qualifying approval. This amendment changes no Finder/provider,
cloud, source-file, or eviction state.

## Amendment: make platform fixtures and active-use CI evidence explicit (2026-08-24 18:51 +0900)

PR #225 exact head `3715a5ada760072d3675026fe7f264b4ee47964f` keeps the resolver fail-closed on
Windows by changing only the regression fixtures from POSIX `/home/u` to the existing platform
absolute-home helper. PR #227 exact head `753352a1d0cd7e297bb656d5edf9339235a628a3` installs `lsof`
in the Ubuntu test image so active-use evidence remains complete in CI; production behavior still
fails closed when the probe is unavailable. Local proofs are 21/21 organize tests and 735/735 Rust
tests with one ignored live-provider test. These CI/fixture repairs grant no copy, cloud-write,
attestation, source-eviction, Finder-cancel, or provider-restart authority.

## Amendment: re-confirm the live Finder preparation blocker without mutation (2026-08-24 19:04 +0900)

A fresh read-only `/usr/bin/brctl status` still reports iCloud `client:needs-sync` with
`needs-sync-up|in-sync-down|prefer-sync-down|oob-sync-ack`, 1,740 `pending-scan` entries, and
343 `pending-sync-up` entries. Finder, `fileproviderd`, and `bird` are present, while no DiskSage
process is running. The root volume has about 12 GiB available, so global fullness is not proven;
the screenshot's seven-item copy size and destination receipt remain unknown. This is aggregate
provider reconciliation evidence only: `provider-sync-incomplete`, copy/attestation, cloud-write,
and source-eviction authority remain fail-closed, and no Finder, provider, source, or cloud
mutation was performed.

## Amendment: exact-head health receipt keeps new copy admission blocked (2026-08-24 19:10 +0900)

The exact-head `disksage-icloud-sync-health` probe completed as a read-only report with
`evidence_complete=true`, `new_copy_admission_state=blocked`, and
`pending_indexable_count=151283`; one upload is active at 95.24% and one download is active.
The report records 343 uploads blocked on sync-up and retains
`provider_sync_attested=false`, `local_eviction_authorized=false`, and `mutation_performed=false`.
The aggregate receipt cannot attest the seven Finder items or a remote cloud write, so copy,
cloud-write, and source-eviction authority remain fail-closed.

## Amendment: record the exact-head Git worktree audit compile repair (2026-08-24 19:28 +0900)

DiskSage #249 exact head `c8ca669262f913de5719ebda377132f1135c06c8` repairs the hosted
all-features `E0425` by making the library-owned `MAX_REFERENCE_BYTES` bound available to its CLI
unit tests without duplicating the validation contract. Pinned Rust 1.97.1 proofs passed 7/7 CLI
tests and 10/10 black-box Git-worktree tests. The audit remains read-only and path-redacted; no
worktree removal, Finder/provider operation, cloud write, or source eviction is authorized by this
repair.

## Amendment: recheck the Finder preparation blocker after scheduler recovery (2026-08-24 19:38 +0900)

A fresh read-only `/usr/bin/brctl status` still reports the iCloud client as `needs-sync` with
`needs-sync-up|in-sync-down|prefer-sync-down|oob-sync-ack`; the last native sync remains
`2026-08-21 20:20:10.166`. Finder, `fileproviderd`, and `bird` are running, while no DiskSage
process is present. The root volume has about 12 GiB available. This is provider-global
reconciliation evidence consistent with the multi-hour Finder `real_datasets` preparation stall,
not a per-item receipt and not proof of a DiskSage lock or cloud write. The existing
`provider-sync-incomplete` admission, copy/attestation, and source-eviction gates therefore stay
fail-closed; no Finder, provider, source, or cloud mutation was performed.

## Amendment: expose a path-free lineage graph for catalog handoff (2026-08-24 19:52 +0900)

The CloudArchive receipt view now offers a JSON export only when a verified receipt contains the
modern lineage fingerprint. The export is a bounded client-side graph with stable content,
metadata, archive, provider, receipt, Goal, and optional eviction node identifiers; it carries
production-time source/confidence, provider sync state, and sorted blockers, while explicitly
setting `local_paths_included=false`. It never fabricates a provider item, attestation, Goal
completion, or eviction relation: legacy receipts without lineage are not exportable, and an
eviction edge appears only after the real eviction output exists. This is an export/view action
only; it performs no provider, source, cloud, ADR, or Goal mutation.

When a provider attestation contains a remote object proof, the graph adds a path-free
`provider-item` node and binds it to the provider and receipt. Without that proof the provider
item remains explicitly absent rather than inferred from a local File Provider path.

## Amendment: preserve the live Finder preparation diagnosis (2026-08-24 20:00 +0900)

The latest read-only `/usr/bin/brctl status` still reports iCloud `client:needs-sync` with
`needs-sync-up|needs-sync-down|in-sync-down|prefer-sync-down|oob-sync-ack`; the native last-sync
timestamp remains `2026-08-21 20:20:10.166`. Finder, `fileproviderd`, and `bird` are running, but
no DiskSage process is present. The persistent `pending-scan` queue and missing per-item receipt
keep the Finder `real_datasets` preparation operation at `provider-sync-incomplete`; no
provider, source, cloud, or Finder mutation was performed.

## Amendment: keep native copy headroom candidate-scoped (2026-08-24 20:18 +0900)

Destination-volume headroom is now evaluated per candidate during the Rust plan stage. A file that
does not fit (or whose destination probe is unavailable) receives its own
`blocked_reason`, while smaller candidates whose probe passes remain eligible; the aggregate plan
notice remains for operator visibility. The UI preserves fail-closed behavior for legacy reports
that have only the aggregate notice. This prevents one oversized archive from disabling every
otherwise admissible copy and does not grant copy, cloud-write, attestation, or source-eviction
authority.

The replaceable Goal contract now names `provider-sync-incomplete` as an explicit runtime state and
`destination-headroom-bound` as a completion gate. These terms keep the Finder/File Provider
diagnosis and candidate-scoped local staging evidence visible to projections instead of collapsing
both into a generic pending state.

## Amendment: classify native iCloud pending scans (2026-08-24 21:00 +0900)

The bounded `brctl status` parser now counts path-free `apply{[ pending-scan ... ]}` entries as
`pending_scan_count` and emits `icloud-native-status-pending-scan`. This is an aggregate native
provider observation, not a per-item cloud receipt: a Finder “복사 준비 중” dialog remains
unverified, `provider-sync-incomplete`, and blocked for copy, attestation, cloud write, and source
eviction until the scan backlog is gone and item-level provider evidence is present. The Naruon
readiness export and CloudArchive UI carry the same blocker and show the bounded next action.

## Amendment: persist native health blockers in runtime projections (2026-08-25 00:00 +0900)

When an iCloud admission probe is persisted, DiskSage now selects the native pending-scan blocker
when present and applies it to every bounded, valid iCloud receipt projection. The replaceable Goal
is written as `blocked` with `provider-sync-state-complete=false` and
`explicit-eviction-permit=false`; the paired ADR records `provider-state-blocked:<blocker>`. This
is a projection update only: immutable receipts remain authoritative, and no provider, Finder,
source, cloud, attestation, or eviction mutation is performed. A missing, malformed, oversized,
or incomplete receipt set emits a stable projection warning instead of claiming that all Goals were
updated.

## Amendment: current Google Drive preparation diagnosis (2026-08-25 09:23 +0900)

A fresh bounded read-only `fileproviderctl dump com.google.drivefs.fpext -l` identified the provider
shown behind the `real_datasets` Finder dialog as temporarily disconnected. The dump reported File
Provider `-1004` server-unreachable failures for the root metadata fetch, active upload and download
progress markers, a 2,000-entry reconciliation section, and a provider error generation above zero.
The local Google Drive mount exposed no materialized `real_datasets` destination at observation time;
the 7.2 GiB source remained local and unchanged. The root volume had about 2.1 GiB available, so a
retry that might stage the source locally is unsafe even though this particular dump did not emit an
explicit disk-full marker.

System Events did not enumerate an active Finder copy-progress window during the later read-only
check. That absence is not evidence of a completed copy: no destination receipt or remote content
proof exists. The default route through `utun4` is recorded as network context only, not as a proven
root cause. DiskSage therefore keeps `provider-global-sync-temporarily-disconnected`,
`provider-global-sync-server-unreachable`, `provider-global-sync-transfer-active`, and
`provider-global-sync-reconciliation-pending` fail-closed for copy, attestation, and source
eviction. No Finder cancellation, provider restart, cloud write, source mutation, or eviction was
performed. The observation was made against DiskSage PR #247 exact head
`9fdf2922da2939d96d3c2393539f2b2d42009929`; the PR remains draft, review-required, and blocked while
hosted checks are pending.

## Amendment: project third-party provider blockers into runtime Goal/ADR (2026-08-25 09:30 +0900)

The provider-global sync persistence path now reuses the monotonic projection helper for OneDrive
and Google Drive as well as iCloud. A fresh `temporarily disconnected`, server-unreachable,
transfer-active, or reconciliation-pending observation therefore writes the matching receipt-linked
Goal as `blocked`, closes `provider-sync-state-complete` and `explicit-eviction-permit`, and records
`provider-state-blocked:<blocker>` in its paired ADR. A clear report never rewrites projections, and
missing or malformed receipts remain an explicit bounded warning. This is local evidence/projection
state only; no Finder cancellation, provider restart, cloud write, source mutation, attestation, or
eviction was executed.

After reclaiming only DiskSage's disposable Rust build artifacts, the root volume had about 3.5 GiB
free, while the same Google Drive dump still reported `temporarily disconnected`, `-1004`, active
transfer markers, and a 2,000-entry reconciliation section. The persisted diagnosis is therefore
not reduced to local disk pressure. The implementation was verified at PR #247 exact head
`87c9089bcd4af49f8f8751c54ebcc45b519d1f0c`; the draft PR remains review-required and blocked while
hosted checks are pending.

## Amendment: bind headroom evidence to the actual data volume (2026-08-25 10:14 +0900)

The live host recheck distinguished the system volume from the `/Users` data volume used by the
source and File Provider staging. `/Users` had about 594 MiB available before disposable build
artifacts were cleaned, while `real_datasets` was about 7.2 GiB; after cleanup the same data volume
had about 2.7 GiB available. The iCloud dump simultaneously reported `pending-indexable-count:
490195`, upload/download progress entries stuck at `0.0000`, and a 482,470-entry reconciliation
section. DiskSage
therefore treats destination-volume headroom and provider-global state as independent blockers:
`local-volume-headroom-insufficient` remains candidate-specific, while provider indexing/transfer
blockers remain global. A system-root `df` result cannot authorize a copy staged on `/Users`.

The preview adapter releases only unverified destination-ancestor diagnostics so a later mutation
probe remains authoritative; insufficient headroom stays blocked. This preserves the existing
legacy aggregate fallback and gives the UI candidate-scoped evidence without granting cloud-write,
attestation, or source-eviction authority. Observation is read-only; no Finder, provider, source,
or cloud mutation was performed. Exact implementation head: `5c3b87359103b82df3efb4099668b1b17f532259`.

## Amendment: repeated zero-progress iCloud receipt (2026-08-25 10:18 +0900)

Two read-only probes 19 seconds apart observed `pending-indexable-count` rising from `492224` to
`492507` and reconciliation from `484500` to `484783`; both retained upload and download markers
at `Fraction completed: 0.0000`. No standalone `cp`, `ditto`, or `rsync` process was present; the
visible preparation window is therefore attributed to Finder/File Provider coordination, not a
DiskSage copy worker. This is a repeated provider-stall receipt: Goal remains
`provider-sync-incomplete`, and copy, attestation, cloud-write, and source-eviction gates remain
closed. No cancellation or provider/source/cloud mutation was performed.

## Amendment: deterministic preview-headroom regression fixture (2026-08-25 11:00 +0900)

The candidate-scoped preview normalization behavior is unchanged. Its regression fixture now uses
an intentionally unfit candidate size so the test cannot inherit the host runner's root-volume
capacity when the synthetic destination has no existing ancestor. Pinned Rust 1.97.1 verification
passed all 745 library tests plus one ignored live-provider test at PR #247 exact head
`dc57a1539b82514f4ceb17ec0fca42ed23ae7988`; no cloud, provider, Finder, source, or eviction
mutation was performed.

## Amendment: current Finder copy-preparation provider receipt (2026-08-25 11:06 +0900)

A fresh read-only File Provider dump identifies the visible Finder preparation as a provider
coordination stall, not a DiskSage copy worker. The Google Drive domain is `temporarily
disconnected`; its root metadata fetch reports File Provider `-1004` (server unreachable), the
reconciliation queue is capped at 2,000 entries, and the latest user-initiated root retry is about
57 minutes old. Upload/download progress markers exist without a completed item receipt. The
iCloud domain independently reports `pending-indexable-count: 505103`, upload/download progress
at `0.0000`, `disk import: yes`, and 497,379 reconciliation entries. The data volume currently has
about 20 GiB free, so the observed Finder wait is not itself proof of local disk exhaustion.

DiskSage therefore keeps the operation at `provider-sync-incomplete`: the Finder “복사 준비 중”
window is not a cloud-write receipt, and copy, attestation, source eviction, provider restart, and
Finder cancellation remain fail-closed. No Finder, provider, source, cloud, or eviction mutation
was performed. The evidence is read-only and path-free; filename dates remain secondary to embedded
metadata and context.

## Amendment: persistent provider stall recheck (2026-08-25 11:09 +0900)

The next bounded read-only recheck still reports the same two provider blockers. Google Drive is
temporarily disconnected with File Provider `-1004`, a 2,000-entry reconciliation cap, and active
upload/download markers. iCloud has grown to `pending-indexable-count: 506044` and 498,320
reconciliation entries while upload/download remain at `0.0000`; disk import and stream reset are
still active. The data volume remains at approximately 20 GiB free. This confirms persistence of
the provider coordination stall rather than a transient Finder rendering issue. The runtime Goal
remains `provider-sync-incomplete`; no copy, attestation, eviction, Finder cancellation, provider
restart, or cloud/source mutation was performed.

## Amendment: third-party provider indexing can expose bounded Finder cancellation (2026-08-25 11:24 +0900)

The provider-global UI now treats `provider-global-sync-indexing-pending` as a Finder-copy blocker,
alongside active transfer and reconciliation blockers. This closes the case where OneDrive or Google
Drive reports only an indexing backlog: the user can request the existing bounded Finder Escape
action, while cloud/provider/source mutation remains unchanged. The contract test and Svelte type
check pass at exact head `dda0f1d5`; no automatic cancellation was performed.

## Amendment: retain unverified destination blockers until proof exists (2026-08-25)

Cloud-plan presentation now checks for at least one previously unblocked candidate with verified
destination/staging headroom before clearing any `local-volume-headroom-destination-*` diagnostic.
When no candidate proves the staging filesystem, the candidate blocker and plan-wide fail-closed
notice remain, so the serialized backend view cannot advertise a copy-only approval phrase for an
unverified destination. The focused unit and runtime regressions pass at exact head
`b3e00c6a9bf13152562ccc50f2ed742b03f0bffa`; mutation-time re-probing remains authoritative.
## Amendment: current iCloud Finder preparation evidence (2026-08-25)

The current bounded read-only observation retained three `fetchContentsForItemWithID` requests
with no progress, roughly 525,000 pending indexable entries, upload progress at zero, download
progress at about 21.45%, and an active reconciliation state. Finder and the provider daemons were
alive, but no DiskSage, ZIP, or `real_datasets` process was running; local headroom was about 12 GiB.
These markers classify the user-visible “복사 준비 중” dialog as provider preflight/backlog evidence,
not a completed copy or a local ZIP stall.

PR #259 (`06c5b59ea6da6846f31a2d235698e38cfe041ece`) exposes the aggregate no-progress label,
bounded transfer percentages, and same-blocker duration. This is diagnostic/operator guidance only:
the UI may request Finder Escape cancellation, but DiskSage does not cancel automatically, kill
`bird`/`fileproviderd`, touch CloudDocs databases, or grant copy, attestation, or eviction authority.
New work remains fail-closed until a fresh complete and quiet provider observation plus independent
per-item receipt evidence exists.

The follow-up bounded observation at `2026-08-25 12:49:51 +0900` still retained seven
`fetchContentsForItemWithID` requests with no progress and an active reconciliation backlog of
`523,158` entries. Transfer state had advanced to upload fraction `0.9999` (11,812,609 of
11,813,276 bytes) and download fraction `0.4273` (254,713,831 of 596,099,680 bytes), so the
Finder dialog remains a provider preflight/reconciliation wait rather than evidence of a local
ZIP worker. The data volume was still at 99% capacity with about 9 GiB available, so the local
headroom gate also remains active for any candidate whose size plus the staging reserve exceeds
that budget. This progress does not clear the no-progress or reconciliation blockers: DiskSage
continues to admit no new copy, attestation, or source eviction until a fresh complete and quiet
observation plus an independent per-item receipt exists.

The bounded follow-up at `2026-08-25 13:04:12 +0900` timed out after 20 seconds while retaining
two no-progress fetch requests, upload fraction `0.9999`, download fraction `0.4862`, and a
`526,878`-entry reconciliation backlog with scheduling still `running`. The data volume had
recovered to about 50 GiB available after removing only this session's generated Rust build
artifacts; headroom is therefore no longer the immediate blocker, but the provider timeout and
backlog still keep copy, attestation, and eviction fail-closed. The timeout itself is incomplete
evidence and cannot be treated as a clear or per-item receipt.

The operator's current Finder dialog still shows “real_datasets” copy preparation after several
hours. A fresh bounded probe at `2026-08-25` found no Archive Utility, `ditto`, or `zip` worker and
the Finder process was idle, while `cloudd` remained busy. `fileproviderctl dump` timed out with
three no-progress fetch requests and `531,061` reconciliation entries. DiskSage therefore classifies
the dialog as an iCloud File Provider materialization/reconciliation wait. The operator-visible
Finder cancel control remains the only recovery action exposed here; it cancels the UI operation
without deleting sources, mutating provider state, or granting copy, attestation, or eviction
authority.

The provider parser also records the redacted `itemIsFlockedCanNotPropagate` condition as
`icloud-file-provider-item-locked-observed` and adds the corresponding
`icloud-file-provider-item-locked` admission blocker. The provider-internal token, item identifier,
and path are never retained. This makes a Finder “copy preparation” wait explainable without
mistaking a lock/materialization failure for a completed copy.

The bounded read-only probe at `2026-08-25 14:08:26 +0900` retained three no-progress fetch
requests, a `541,234`-entry reconciliation backlog, upload fraction `0.9999`, and download
fraction `0.5864`. It also retained provider error count `22`, including the already-classified
`itemIsFlockedCanNotPropagate` condition observed about three hours earlier and repeated
`noContentToFetch` failures. The Data volume had about `42 GiB` available. These are aggregate,
path-free observations: provider activity is not a per-item receipt, the flock/error markers keep
copy and eviction fail-closed, and no provider database or daemon mutation is authorized.

The subsequent read-only probe at `2026-08-25 14:45:47 +0900` still retained one
`fetchContentsForItemWithID` request, one `itemIsFlockedCanNotPropagate` marker, `22`
`noContentToFetch` failures, and a maximum observed reconciliation backlog of `544,098` entries.
The corresponding process snapshot showed `fileproviderd` at `52.5%` CPU, `bird` at `27.6%`, and
the user `cloudd` at `6.1%`. This is current provider-pressure evidence, not proof of per-item
completion or causal ownership by DiskSage; the copy and eviction gates therefore remain closed.

The provider parser now also treats a redacted fetch/create operation age of at least 15 minutes
as `icloud-file-provider-stalled`. This captures a multi-hour Finder “preparing” wait even after
DiskSage restarts; it remains an observational blocker, not evidence of causal ownership by
DiskSage or permission to mutate Finder, provider state, or source data.

The exact-head follow-up `f184492a` also accepts a provider operation marker and its redacted age
token on adjacent dump rows, because File Provider diagnostics may wrap one operation across lines.
The focused Rust parser suite passed 12/12, including rejection of unrelated parenthesized durations,
cross-record adjacent age pairing, old healthy operation timestamps without an error marker, and a
fresh `last:` value paired with an older `expired:` value.
A bounded read-only probe at `2026-08-25 15:26 +0900`
timed out after 30 seconds while the earlier bounded output still showed `fetch-content` errors
aged about six hours; that timeout is incomplete provider evidence and keeps copy, attestation,
and eviction fail-closed. No Finder, provider daemon, CloudDocs database, source, or cloud object
was mutated.
