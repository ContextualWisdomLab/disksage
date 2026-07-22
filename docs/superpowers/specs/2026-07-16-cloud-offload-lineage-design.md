# Cloud offload and lineage dry-run

## Goal

Use DiskSage against a genuinely space-constrained machine to identify files that can move to iCloud Drive, OneDrive, or Google Drive without risking a premature destructive action.

## Scope

- Discover permission-writable local roots for all three providers. Google Drive writable children are separate destinations because the account root is read-only on macOS. Record a bounded readability snapshot and a stable non-sensitive access issue for every surfaced root; an unreadable root remains visible for diagnosis but is disabled in the UI and revalidated immediately before planning or OAuth use. A separate discovery report preserves failures above the destination level, such as a Google account root whose children cannot be enumerated, while the legacy root-list response remains backward compatible.
- On macOS, keep the Tauri GUI (`disksage`) and headless planner (`disksage-cloud-plan`) as distinct binaries and embed purpose strings for Downloads and File Provider access. The headless binary is behind the explicit `cloud-cli` Cargo feature so Tauri's `--bins` build cannot substitute it for the GUI executable; CI tests it separately. Permission remains user-mediated; DiskSage never edits TCC or bypasses a denial.
- Scan locally in Rust while pruning provider roots, symlinks, hidden trees, OS `Library`/`System` trees, and regenerable developer artifacts.
- Consider only a conservative allowlist of document, media, archive, dataset, backup, and creative file extensions.
- Default to files at least 256 MiB and not modified for 90 days.
- Extract content metadata from media containers (`ffprobe`), PDF document info (`pdfinfo`), and OOXML/ODF core properties (`unzip`), when the corresponding local SDK is available.
- Resolve production time in this order: embedded content metadata, an explicit full date token in the filename, filesystem creation time, then filesystem modification time. A filename date is only a low-confidence provisional value: it never outranks embedded metadata and cannot authorize a copy without an evidence-bound review. A Korean publication marker such as `26년 1월호` is not a full production date: preserve it only as `filename-publication-month=2026-01` context and `filename:publication-month` low-confidence evidence, never fabricate a day or use it for destination placement. Within embedded metadata, prefer high-confidence recording/capture/document-creation fields over medium-confidence dates inferred from titles. Preserve every observed value and its source/confidence as lineage evidence rather than discarding conflicts. Embedded does not automatically mean trustworthy: known OOXML template defaults, an embedded date after the filesystem modification date, and metadata-derived personal or confidential context require review.
- Preserve lineage fields: source root, original relative path and parent context, created and modified timestamps, content title/authors/duration, production timestamp source, all metadata evidence, planned provider/destination, and a stable metadata fingerprint.
- Plan the destination as `DiskSage Archive/<production year>/<month>/<kind>/<original relative path>`.
- Match selected provider roots by exact or canonical filesystem identity, with NFC-equivalent UTF-8 as a bounded fallback so macOS File Provider's decomposed names remain selectable; ambiguous matches fail closed.
- Mark datasets and backups for explicit review.
- Surface destination collisions and exclude them from potentially reclaimable bytes.
- Selectively stream SHA-256 and BLAKE3 only for non-blocked candidates that share an exact byte length. Exact-content clusters expose their member count and redundant bytes, bind the hashes into review evidence, and require explicit canonical-lineage selection instead of silently copying every duplicate path.
- Require review when embedded geolocation is present, embedded production-date fields disagree, an embedded production date conflicts with a full filename date, an embedded production month differs from a filename publication month, a known template/default timestamp is detected, embedded title/author/context indicates personal or confidential material, or no embedded production date is available.
- Provide both a Tauri UI and a headless JSON CLI. The CLI's plan-only
  `--decision-summary` view keeps relative paths, metadata/review fingerprints, production-time
  source and confidence, review reasons, blockers, duplicate totals, and capacity status. It
  omits absolute source/destination/cloud-root paths, root labels, content titles/authors, raw
  metadata evidence values, and dataset profiles. The flag is rejected when combined with root
  inspection, copy, adoption, attestation, eviction, review, or Naruon export actions; the full
  `CloudPlanReport` and Naruon lineage contracts remain unchanged.

## Safety boundary

This slice is read-only. It does not create a destination directory, move/delete a file, hydrate cloud placeholders, call a model, or contact a network service. A later execution slice must verify cloud quota, perform copy plus content-hash verification, wait for provider sync completion, write an immutable lineage manifest, and only then offer local eviction or trash-based source removal.

Metadata extractor absence is non-fatal: the Rust planner records the best remaining local evidence and falls back conservatively. The external utilities only parse local files; they do not perform network access.

## Integration decisions

- Noema is not needed for runtime planning; it remains an independent code-review/governance concern.
- No LLM or LLM-as-a-Judge is used in this slice. If unknown-file/context classification becomes a binary or polytomous judgment, evaluate `fast-mlsirm` against its live PR #160 contract before integration.
- The existing local OWL classifier remains sufficient for the dry-run. Persisted cross-device catalog/search/lineage will be designed as a `semantic-data-portal` integration after this local event contract is proved.
- There is no database in this slice, so `pg-erd-cloud` has no schema to model yet. It becomes required when a persistent lineage store is introduced.
