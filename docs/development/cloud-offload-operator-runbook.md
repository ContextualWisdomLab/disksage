# Cloud offload runtime evidence

DiskSage plans cloud copies from embedded metadata first, then filename date, filesystem creation
time, and modification time. Filename tokens such as `2026-04-28` or `251210` are secondary
evidence and never establish production time by themselves.

The runtime sequence is:

1. A verified copy writes `cloud-goals/<receipt-id>-latest.json` with provider and evidence gates
   explicitly incomplete.
2. Provider attestation writes an immutable evidence record, then updates the Goal and ADR
   projections atomically.
3. `is_local_current=true` with `is_uploaded=false` is `pending-upload`; the source remains and
   no eviction permit is issued.
4. Only a fresh attestation plus the separate receipt-bound human approval may move the source to
   the OS Trash. The destination and Trash are never emptied by DiskSage.

The Goal and ADR files are replaceable projections. Agents or operators must compare them with the
immutable receipt/evidence record before any mutation. Naruon receives lineage/provider evidence,
not a second independent deletion authority.
