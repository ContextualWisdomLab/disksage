# ADR-0011: Failed copy evidence and placeholder-safe adoption

**Status:** Accepted  
**Date:** 2026-08-25

## Context

The successful cloud-copy receipt is immutable, but a bounded native copy can time out or be
cancelled before that receipt exists. Existing-copy adoption also reads the destination to compare
hashes; on macOS a dataless File Provider item can be hydrated by that read and consume local
headroom. Finally, local recovery needs exact paths while shareable Naruon/lineage exports must not
leak them.

## Decision

DiskSage writes an append-once, mode-restricted private failure record for every bounded native
copy error or cancellation. The record binds the candidate fingerprint, provider, exact local
paths, action, bounded error code, timestamp, and stable failure ID. It is diagnostic only: it
cannot satisfy provider synchronization, approval, or source-eviction gates.

The adoption path first asks the provider adapter to prove that the destination is already
materialized and local-current. Dataless, downloading, stale, unsupported, or changed status
fails closed before any hash read. A later success receipt still rechecks identity around hashing.

Private local receipts and UI retain identity-critical paths for recovery and exact revalidation.
Shareable/public logs and Naruon/semantic-data-portal exports use stable fingerprints and relation
IDs only; they are path-free by contract.

## Consequences

- A failed or cancelled transfer is restart-auditable without implying a successful copy.
- Cancellation is explicit in the UI and stops at a safe helper/chunk boundary.
- Placeholder adoption cannot silently hydrate data merely to test equality.
- The implementation is local-first and does not add OAuth, Noema, or an external LLM dependency.

## Evidence

- `src-tauri/src/cloud_transfer.rs` failure record, cancellation token, and copy gate tests.
- `src-tauri/src/provider_sync.rs` pre-hash materialization gate and status tests.
- `docs/product-technical-gap-baseline.md` P0/P1 gap rows and issue #261.
