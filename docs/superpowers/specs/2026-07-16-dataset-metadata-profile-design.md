# Dataset Metadata Profile Design

## Goal

DiskSage must inspect data metadata before treating a dataset as a cloud-offload candidate. A
filename date, filesystem timestamp, or a generic `Dataset` extension is not sufficient evidence.

This slice adds a bounded, local, Rust-native schema profile for CSV, TSV, and JSON Lines files.
It never records cell values. Other dataset formats remain explicitly unprofiled and fail closed.

## Contract

- Read at most 1 MiB and 100 data rows from a dataset.
- Record format, sampled row count, column names, inferred coarse types, missing counts, and
  sensitive-name indicators.
- Never persist or display cell values.
- Flag empty/duplicate columns, inconsistent row widths, parser failures, sample truncation,
  unsupported formats, and likely sensitive columns.
- Keep every dataset in explicit review. A complete schema sample does not by itself authorize a
  cloud copy, and a dataset still needs high-confidence embedded production-time evidence.
- Expose the structured profile in the desktop UI and the existing metadata evidence trail.

## Supported formats

- CSV and TSV: header-driven schema plus bounded row sampling using Rust's `csv` parser.
- JSONL: union of top-level object keys plus bounded row sampling using `serde_json`.
- Parquet, Arrow/Feather, SQLite, SPSS/SAS/Stata/R, and SQL dumps: recognized as datasets but
  reported as `unsupported-dataset-format` until a native reader is deliberately added.

## Catalog boundary

DiskSage remains the local scanner and evidence producer. `semantic-data-portal` is the appropriate
future catalog consumer because it already models datasets, columns, ontology concepts, policy,
and steward review. This slice does not start its Python service or upload local metadata. A later
opt-in connector can send only the structured profile after the user chooses a catalog endpoint and
approves which metadata fields may leave the machine.

The bounded parser and fail-closed policy are deterministic, so this slice does not need `noema`,
`contextual-orchestrator`, an external LLM, or `fast-mlsirm`. Those systems remain escalation paths
only if later semantic mapping or ambiguous stewardship decisions cannot be expressed as explicit
rules and evidence.

## Data-quality interpretation

The profile covers the lowest layer of the data-quality pyramid: schema structure, bounded
completeness signals, and obvious validity risks. It does not claim accuracy, uniqueness, referential
consistency, or full-table completeness from a sample. Those claims require an explicit data
contract and a deeper, separately authorized scan.
