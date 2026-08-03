# Incomplete download metadata and archive integrity

## Problem

The cloud planner previously selected only known final-file extensions. A real Downloads scan
therefore omitted more than 3 GiB of `.crdownload` files and multipart ZIP members while also
treating one final ZIP segment with a `.zip` name as an ordinary archive candidate.

Filename dates, filesystem creation times, browser quarantine times, and ZIP entry timestamps are
not production-date proof. They may still be useful as acquisition, packaging, and lineage
evidence, but only when their source and confidence remain visible.

## Decision

- Surface `.crdownload` files as `incomplete-download` candidates and always block cloud copy.
- Surface exact `.zip.partNNN` members, inventory present and internally missing part numbers, and
  block per-file copy because multipart archives require an atomic set operation that DiskSage
  does not yet provide.
- Parse complete `.zip` central directories in Rust without extracting entries. The preflight
  requires a true end-of-file EOCD, rejects Zip64 and multi-disk indexes, caps the index at 10,000
  entries and 16 MiB, and records entry timestamps, top-level names, aggregate sizes, unsafe or
  encrypted entry counts, and coarse content classes. An unreadable index blocks copy as
  `archive-index-unreadable`; approval cannot override the planner block.
- Stream `.crdownload` bytes through a one-MiB Rust buffer, only for files no larger than 4 GiB.
  Record at most 64 ZIP EOCD offsets and validate each retained offset against bounded central
  directory records plus the first and last local-file headers. An EOCD is only a ZIP fragment
  signal; a validated span is only a structural archive candidate, not proof that every payload
  entry or CRC is recoverable. The original incomplete download remains copy-blocked either way.
- On macOS, retain only the host portion of `com.apple.metadata:kMDItemWhereFroms`. Full URLs,
  query strings, signed tokens, and opaque quarantine identifiers are not persisted.
- Record Edge/Chrome quarantine time as `download-acquired-date`, never as production time.
- Keep all probes local and read-only. External metadata commands retain the existing five-second
  and one-MiB output limits; Rust archive scans have explicit byte, entry, index, and evidence-count
  bounds.

## Observed case

- One four-volume set exposed parts `000,001,003,004`, proving an internal gap at `002`.
- Another set exposed only `000,004`, proving gaps at `001,002,003`.
- A `.zip` and `.zip.part004` pair had identical SHA-256 values, and `zipinfo` reported that the
  leading bytes required by the central directory were missing. Neither is a safe standalone
  cloud candidate.
- A corporate dataset ZIP carried a groupware origin host and consistent CSV packaging timestamps.
  The origin is destination-policy context; it is not a production date and must be reviewed before
  any personal-cloud copy.
- Of eight stale large incomplete downloads in the observed Downloads directory, four had no ZIP
  EOCD signature, two had ZIP fragments but no structurally complete span, one had a single
  structurally valid 5,706-entry span surrounded by unrelated bytes, and one had four small
  structurally valid spans. This evidence prevents treating every old `.crdownload` as equivalent
  disposable browser residue.

## Non-goals

- No source deletion or Trash operation.
- No automatic renaming of `.crdownload` files based on file signatures.
- No multipart concatenation, extraction, archive carving, repair, decompression, payload-value
  inspection, or CRC-based recovery claim.
- No LLM, agent, ontology, or external catalog dependency; deterministic Rust and OS metadata are
  sufficient for this boundary.
