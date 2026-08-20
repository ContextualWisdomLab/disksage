# ADR-0009: Export a path-free lineage relation graph

**Status:** Accepted
**Date:** 2026-08-21

## Context

The cloud-offload receipt already binds metadata, review, copy verification, and provider
evidence, but its exported ontology relation list only contained a generic `archivedTo` edge.
That made the connection between a Finder/File Provider incident, its metadata decision, and a
later provider attestation difficult to inspect or catalog. Raw local and provider paths must not
become graph identifiers.

## Decision

The Rust Naruon export emits deterministic, domain-separated digest node IDs for the source,
metadata evidence, production evidence, archive, destination, receipt, review decision, provider
sync state, provider evidence, and remote object. It emits ontology predicates for those edges and
retains the existing `archivedTo` predicate for compatibility. The relation graph is derived from
the validated receipt and optional validated provider evidence; it never grants cloud-write or
source-eviction authority. Missing provider evidence produces an `unknown` sync-state node and no
attestation or remote-object edge.
Legacy evidence that only sets `sync_complete=true` while leaving `sync_state=unknown` is also
exported as unconfirmed; an explicit `complete` provider state is required.

## Consequences

- Naruon, semantic catalogs, and UI consumers can render stable provenance edges without using raw
  paths as identifiers.
- `local-current` with `is_uploaded=false`, missing evidence, or an incomplete provider probe
  remains visibly pending and cannot produce an eviction permit.
- Existing consumers that only understand `archivedTo` continue to work.
- The envelope still carries legacy path fields for compatibility; consumers should use relation
  node IDs for catalog joins.

## Rejected alternatives

- Adding a new ontology service or external LLM was rejected because the existing Rust receipt is
  the authoritative evidence boundary and already has the required fields.
- Hashing only the destination was rejected because it would omit metadata, review, and provider
  evidence relationships.
- Using raw paths or provider object IDs as relation identifiers was rejected because they disclose
  private locations and are not safe catalog keys.

## Evidence

The implementation and focused regression test are in `src-tauri/src/naruon_lineage.rs` at source
head `677042467b3398866757f39b9475bd0b267abc75`; the focused relation tests are kept alongside
the export contract. The design preserves the
metadata-first precedence and the fail-closed provider-sync contract from ADR-0001.
