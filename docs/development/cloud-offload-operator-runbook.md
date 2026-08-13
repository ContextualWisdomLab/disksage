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
   receipt set; it refreshes provider evidence and the replaceable ADR/Goal projections only. The
   UI also exposes a manual re-run, and the reconciliation never writes to cloud or evicts a source.
4. `is_local_current=true` with `is_uploaded=false` is `pending-upload`; the source remains and
   no eviction permit is issued.
5. Files inside a `.photoslibrary`/`.photolibrary` bundle are non-overridable
   `system-managed-photos-library-data` blockers; individual SQLite members are never copied.
6. Only a fresh attestation plus the separate receipt-bound human approval may move the source to
   the OS Trash. The destination and Trash are never emptied by DiskSage.

The Goal and ADR files are replaceable projections. Agents or operators must compare them with the
immutable receipt/evidence record before any mutation. Naruon receives lineage/provider evidence,
not a second independent deletion authority.
