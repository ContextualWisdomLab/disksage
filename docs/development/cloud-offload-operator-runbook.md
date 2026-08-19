# Cloud offload runtime evidence

DiskSage plans cloud copies from embedded metadata first, then filename date, filesystem creation
time, and modification time. Filename tokens such as `2026-04-28` or `251210` are secondary
evidence and never establish production time by themselves.

The runtime sequence is:

1. A verified copy writes `cloud-goals/<receipt-id>-latest.json` with provider and evidence gates
   explicitly incomplete. The desktop app uses its app-data directory; the headless CLI derives
   sibling `cloud-goals` and `cloud-adr` directories from its receipt/evidence directory.
2. Provider attestation writes an immutable evidence record, then updates the Goal and ADR
   projections atomically in both entrypoints.
3. After restart, the desktop app automatically runs `reconcile_cloud_receipts` over the bounded
   receipt set; it refreshes provider evidence and the replaceable ADR/Goal projections locally.
   The open desktop view repeats this cloud-write-free reconciliation every 60 seconds and exposes
   a manual re-run; it never evicts a source.
   The same operation is available headlessly with
   `disksage-cloud-plan --reconcile-receipts --receipt-dir ABSOLUTE_PATH --evidence-dir ABSOLUTE_PATH`
   (add the existing OAuth connection flags only when a provider API fallback is required). This
   command performs local evidence/projection writes only; `--audit-receipts` remains a strictly
   read-only integrity report.
   When the local File Provider cannot admit a new OneDrive or Google Drive item, an explicitly
   approved headless upload can use `--provider-api-copy-fingerprint HEX64` together with
   `--oauth-connections ABSOLUTE_PATH`. This path requires the write OAuth scope, fresh capacity,
   the same human-attributed copy phrase and review gates, re-hashes the source after upload, and
   immediately attempts provider-API attestation. It never supports iCloud or local eviction in
   the same action; the returned receipt/object ID is the hand-off for a later attestation.
4. `is_local_current=true` with `is_uploaded=false` is `pending-upload`; the source remains and
   no eviction permit is issued.
   Third-party File Provider dumps also block new copies while upload/download progress,
   non-zero reconciliation backlogs (`provider-global-sync-reconciliation-pending`), provider
   disconnection, or path errors are present; the stable blocker codes are shown in the plan and
   are never a reason to bypass the gate.
5. If the destination is valid but the receipt source is missing, unsafe, or macOS reports it as a
   File Provider `dataless` object, reconciliation writes a blocked Goal/ADR projection and records
   `source-not-present`, `source-content-not-local`, or the precise source-state blocker; it never
   treats that as proof of a completed eviction. A previously advanced projection is not rewound;
   its Goal status becomes `blocked` and the explicit eviction gate is revoked. A terminal
   `source-evicted` projection remains completed because the original path is expected to be gone.
   Files under macOS File Provider's private `File Provider Storage` tree (including
   `DownloadStage`) are also non-overridable `system-managed-file-provider-storage` blockers;
   DiskSage never treats provider staging bytes as user-owned cleanup candidates.
6. Files inside a `.photoslibrary`/`.photolibrary` bundle are non-overridable
   `system-managed-photos-library-data` blockers; individual SQLite members are never copied.
7. Only a fresh attestation plus the separate receipt-bound human approval may move the source to
   the OS Trash. The destination and Trash are never emptied by DiskSage.
8. Keep DiskSage repositories, Git worktrees, and temporary evidence outside macOS-managed
   File Provider roots (for example `~/Documents` when it carries a provider-domain marker).
   A dataless `.git` file or a Git operation that waits on materialization is provider evidence,
   not a stale-worktree deletion signal; stop the audit and relocate the worktree to a local
   volume such as `/private/tmp` before continuing.

The Goal and ADR files are replaceable projections. Agents or operators must compare them with the
immutable receipt/evidence record before any mutation. Naruon receives lineage/provider evidence,
not a second independent deletion authority.
