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

Source enumeration is also forbidden inside managed File Provider trees (`Library/Mobile
Documents`, `Library/CloudStorage`, `Library/Application Support/FileProvider`, and
`File Provider Storage`). If one of these trees is supplied as the scan root, the bounded collector
returns an incomplete scan with `source-scan-managed-file-provider-root` and produces no transfer
candidate. This prevents DiskSage diagnostics from competing with, or materializing, provider
state.

DiskSage repositories, Git worktrees, and temporary evidence are operated from a local volume
outside managed File Provider roots. A provider-domain marker on the parent or a dataless `.git`
entry is treated as provider materialization evidence, not as proof of a stale worktree; the
worktree audit stops and must be relocated before it can continue.

Provider-wide File Provider dumps are bounded by both output size and wall-clock time. If a timed-out
dump has already emitted safe aggregate markers, DiskSage may retain only those markers as
incomplete evidence; it records `provider-global-sync-probe-timeout`, marks the provider state
`unavailable`, and continues to block new copies. A partial dump can never become authoritative
clear evidence.

## Consequences

- `is_local_current=true` and `is_uploaded=false` produces `pending-upload` and no eviction permit.
- Goal completion gates remain false until their corresponding evidence exists.
- Filename dates can place a candidate in a provisional archive period, but never authorize automatic transfer or eviction.
- A personal native-client copy may proceed without OAuth quota evidence only while the matching desktop client is observed running; provider sync attestation still gates eviction.
- Managed File Provider roots are never recursively scanned; the explicit incomplete-scan blocker is non-overridable.
- Worktree audits stop on provider-managed parents or dataless Git metadata; stale-worktree removal
  is never inferred from a materialization wait.
- A timed-out provider-wide dump may explain active transfer or reconciliation markers, but its incomplete evidence never admits a new copy.
- An iCloud native `needs-sync-up` or `needs-sync-down` state blocks new-copy admission until the
  bounded native status is quiet; neither direction is treated as completed provider evidence.
- A timeout while collecting the bounded iCloud native status also blocks new-copy admission;
  timeout is not interpreted as a quiet provider.
- The bounded iCloud File Provider activity probe records only the count of redacted `no progress`
  fetch markers. Any such marker, a probe timeout, or unavailable probe evidence blocks new-copy
  admission; no path, filename, item identifier, or content is retained.
- A `source-not-present`, `source-content-not-local`, or unsafe-source observation blocks the Goal
  even when provider sync is complete; DiskSage never infers that an externally removed or
  File-Provider-dataless source was safely evicted.
- `eviction-ready` permits only the separately approved, reversible OS-Trash operation.
- A stale projection is replaceable state and must be reconciled against immutable evidence.
- Ontology-based local organization uses the same lineage precedence as cloud planning (embedded
  metadata, explicit filename date, filesystem creation time, then modification time). Its move
  plan carries a path-free lineage fingerprint plus the source size/mtime snapshot and is rejected
  if the source changes; File Provider dataless sources are not moved.
