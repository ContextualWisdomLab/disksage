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
- Probe `.zip` central-directory readability with a bounded local `zipinfo -h` invocation. A probe
  failure blocks copy as `archive-index-unreadable`; approval cannot override the planner block.
- On macOS, retain only the host portion of `com.apple.metadata:kMDItemWhereFroms`. Full URLs,
  query strings, signed tokens, and opaque quarantine identifiers are not persisted.
- Record Edge/Chrome quarantine time as `download-acquired-date`, never as production time.
- Keep all probes local, read-only, bounded by the existing five-second and one-MiB limits.

## Observed case

- One four-volume set exposed parts `000,001,003,004`, proving an internal gap at `002`.
- Another set exposed only `000,004`, proving gaps at `001,002,003`.
- A `.zip` and `.zip.part004` pair had identical SHA-256 values, and `zipinfo` reported that the
  leading bytes required by the central directory were missing. Neither is a safe standalone
  cloud candidate.
- A corporate dataset ZIP carried a groupware origin host and consistent CSV packaging timestamps.
  The origin is destination-policy context; it is not a production date and must be reviewed before
  any personal-cloud copy.

## Non-goals

- No source deletion or Trash operation.
- No automatic renaming of `.crdownload` files based on file signatures.
- No multipart concatenation, extraction, repair, or payload inspection.
- No LLM, agent, ontology, or external catalog dependency; deterministic Rust and OS metadata are
  sufficient for this boundary.
