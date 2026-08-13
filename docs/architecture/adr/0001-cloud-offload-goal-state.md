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

## Consequences

- `is_local_current=true` and `is_uploaded=false` produces `pending-upload` and no eviction permit.
- Goal completion gates remain false until their corresponding evidence exists.
- `eviction-ready` permits only the separately approved, reversible OS-Trash operation.
- A stale projection is replaceable state and must be reconciled against immutable evidence.
