# Naruon file-lineage export

## Context

Naruon PR #1089 introduces a strict `source_lineage_json` v1 envelope for RFC 822 imports. It
correctly preserves embedded `Date` evidence, exact source SHA-256, message identity, and the
metadata precedence `embedded metadata -> explicit filename date -> filesystem creation time ->
filesystem modification time`. Its schema is intentionally email-specific and rejects unknown
fields.

DiskSage copy receipts already bind general-file content hashes, source-relative path, production
time evidence, metadata review, destination account scope, and cloud-copy state. Reusing Naruon's
email schema would either discard these facts or weaken its fail-closed validator.

## Decision

DiskSage exports a distinct `disksage.file-lineage` schema version 1 from an immutable v3 copy
receipt. The optional provider evidence record must also pass its immutable filename and digest
checks and bind the same receipt, provider, destination, byte count, and BLAKE3 hash.

The export includes:

- source filename and source-relative path, without exporting the absolute source root;
- exact source SHA-256, BLAKE3, and byte count;
- selected production-time value, evidence source, confidence, and the shared four-level evidence
  precedence;
- filesystem creation/modification fallback timestamps and all bounded metadata evidence;
- review fingerprints, reason codes, attributed decision, and rationale;
- copy receipt, destination, provider/account scope, optional provider-sync evidence IDs, and the
  evidence-bound `complete`, `pending`, or `overdue` diagnostic with its fixed 24-hour threshold.

`provider_write_executed` is always `false`. A verified local copy into a File Provider directory
and a later provider status observation do not prove that DiskSage executed a provider API write.
`local_copy_verified` and `provider_sync_confirmed` remain separate facts.
An overdue diagnostic never changes `provider_sync_confirmed` or authorizes source eviction.

The CLI action is read-only:

```text
disksage-cloud-plan --export-naruon-lineage /absolute/receipt.json \
  [--naruon-sync-evidence /absolute/evidence.json]
```

It prints the JSON envelope to standard output and does not create, overwrite, upload, hydrate,
evict, move, or delete any file.

## Integration boundary

This change adds no Naruon database column. Naruon's RFC 822 lineage contract remains untouched;
the general-file validation endpoint checks `schema_kind`, `schema_version`, and internal claim
consistency without persisting or reflecting the envelope. If Naruon later needs multiple
acquisitions or chain-of-custody relations, promote the JSON contract to child records and verify
the migration privately with pg-erd-cloud, as PR #1089 already prescribes.

The export is deterministic validation and serialization in Rust. No Noema agent, local or
external LLM, LLM-as-a-Judge, fast-mlsirm, semantic-data-portal, ontology, or Figma asset is needed.
