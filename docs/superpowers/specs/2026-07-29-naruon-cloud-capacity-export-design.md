# Naruon cloud-capacity assessment export

## Context

DiskSage capacity schema version 3 binds read-only provider capacity to a
destination provider and, where the provider exposes it, an authoritative
account scope. The ordinary decision summary is redacted but contains other
planning aggregates that Naruon does not need. The per-file
`disksage.file-lineage` envelope is also the wrong boundary: destination
account capacity is plan evidence, not source-file provenance.

## Decision

DiskSage exports a separate `disksage.cloud-capacity-assessment` version 1
envelope. It contains:

- the decision-batch fingerprint version and fingerprint for the exact plan;
- the destination provider and account scope;
- the version 3 capacity snapshot, including its provider-bound evidence
  fingerprint but not the provider account, drive, permission, or cloud-root
  identifier; and
- the requested, largest-candidate, reserve, required, fit, blocker, and notice
  claims.

Before serialization, Rust:

- validates provider-specific snapshot shapes;
- rejects provider or authoritative account-scope switching;
- binds requested bytes to the plan's potentially reclaimable bytes and the
  largest candidate to the plan's largest unblocked candidate;
- recomputes the capacity assessment with checked unsigned 64-bit arithmetic;
  and
- rejects a missing or altered assessment.

No absolute source path, destination path, cloud-root path, root label,
provider account identifier, OneDrive drive ID, or Google permission ID appears
in the envelope.

## CLI

The read-only output mode requires a fresh single-destination capacity
observation:

```text
disksage-cloud-plan --root /absolute/source \
  --provider icloud \
  --verify-capacity \
  --export-naruon-capacity
```

OneDrive and Google Drive additionally use the existing absolute
`--oauth-connections` document. The command prints JSON to standard output. It
does not write a provider object, copy a candidate, persist a receipt, hydrate
or evict a File Provider item, move a source, or delete anything.

## Trust boundary

The decision-batch fingerprint intentionally omits volatile capacity because a
copy always requires a fresh capacity check. Naruon receives neither the
redacted plan inputs needed to recompute that batch fingerprint nor the
provider binding identifier needed to recompute the evidence fingerprint. Its
acceptance therefore means schema and claim consistency only, not independent
provider authentication or freshness.

A positive fit assessment is not copy approval, provider-write proof,
provider-sync proof, physical-reclaimability proof, or local-source eviction
authorization.

## Integration choice

The exporter and arithmetic stay in Rust. The contract is deterministic and
database-free, so it does not require Noema, a local or external LLM,
fast-mlsirm, semantic-data-portal, pg-erd-cloud, or Figma.
