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
4. `is_local_current=true` with `is_uploaded=false` is `pending-upload`; the source remains and
   no eviction permit is issued.
5. If the destination is valid but the receipt source is missing or unsafe, reconciliation writes
   a blocked Goal/ADR projection and records `source-not-present` (or the precise source-state
   blocker); it never treats that as proof of a completed eviction.
6. Files inside a `.photoslibrary`/`.photolibrary` bundle are non-overridable
   `system-managed-photos-library-data` blockers; individual SQLite members are never copied.
7. Only a fresh attestation plus the separate receipt-bound human approval may move the source to
   the OS Trash. The destination and Trash are never emptied by DiskSage.

The Goal and ADR files are replaceable projections. Agents or operators must compare them with the
immutable receipt/evidence record before any mutation. Naruon receives lineage/provider evidence,
not a second independent deletion authority.
