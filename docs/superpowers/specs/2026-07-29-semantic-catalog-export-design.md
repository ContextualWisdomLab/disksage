# Semantic catalog pre-copy export

## Context

DiskSage has rich metadata before a cloud copy and a content-addressed Naruon
lineage envelope after a verified copy. The standards-based file ontology in
semantic-data-portal requires a content SHA-256 and a verified distribution, so
using its persistent `FileAsset` contract before copy would misstate both file
identity and storage evidence.

The integration therefore uses the non-persisting
`POST /file-assets/preview/disksage` contract introduced by
semantic-data-portal PR #31, stacked on its standards-based file ontology PR
#28.

## Contract

`export_semantic_catalog_candidate_batch` emits
`disksage.file-catalog-candidate-batch` version 1. It is bounded to 200
candidates and a 2 MiB pretty-printed JSON body. The Rust output type has no
field for:

- absolute source or destination path;
- source root, relative path, source context, or filename;
- cloud-root path or label;
- provider account, drive, permission, object, or locator identifier.

It retains the stable candidate and review fingerprints, destination provider
and account scope, archive kind, byte count, filesystem timestamps, selected
production time, review state, embedded content metadata, bounded dataset
profile, and metadata evidence. Content title, authors, context, column names,
and evidence values are private metadata: stdout must be sent only to an
approved portal endpoint and must not be committed or copied into public
diagnostic artifacts.

## Metadata lineage invariant

Rust and semantic-data-portal enforce the same fixed production-time
precedence:

1. `embedded_metadata`
2. `explicit_filename_date`
3. `filesystem_created`
4. `filesystem_modified`

The selected timestamp must have matching calendar-date evidence and the exact
source binding. If evidence from a higher-priority class exists, selecting a
lower-priority value is rejected. Every non-embedded selection remains
low-confidence and review-required. A filename date is therefore auxiliary
evidence, never a trusted replacement for embedded metadata.

## CLI

The mutually exclusive read-only output mode is:

```text
disksage-cloud-plan --root /absolute/source \
  --cloud-root /absolute/detected/cloud/root \
  --min-size-mib 1 \
  --min-age-days 0 \
  --limit 50 \
  --export-semantic-catalog
```

The command creates a fresh single-destination dry-run plan and prints the
bounded batch to stdout. It does not call semantic-data-portal itself, write a
catalog record, call an LLM, copy a file, create a receipt, upload a provider
object, approve a review, hydrate or evict a cloud item, move a source, or
delete anything.

## Portal semantics

The portal validates the structure and returns deterministic proposed
`hasArtifactType` projections. It does not echo private content metadata, call
an LLM, or persist graph/file assets. Its response explicitly keeps
`copy_authorized`, `eviction_authorized`, and `persistable_as_file_asset`
false. A content SHA-256 and verified distribution are still required before
the persistent file-asset contract can be used.

The endpoint records its normal authenticated create-file policy decision as
governance evidence. That evidence is not catalog persistence and not operator
approval.

## Integration choice

The exporter and every selection/size check remain deterministic Rust. Noema,
Gemma, contextual-orchestrator, fast-mlsirm, pg-erd-cloud, and Figma are not
needed for this structural adapter. semantic-data-portal is used because an
ontology/catalog boundary is now real; its PR #28 already carries the
standards and research grounding reused by this stacked integration.
