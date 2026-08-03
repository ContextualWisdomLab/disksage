# Metadata Probe Runtime Safety Design

## Problem

A real-world Downloads dry-run showed `ffprobe` spending more than 30 seconds on one large WAV file.
The existing ExifTool, ffprobe, pdfinfo, and unzip adapters used blocking `Command::output` calls
with no runtime or output bound. One malformed or unusually large file could therefore stall the
complete cloud plan or consume unbounded memory.

## Contract

- Every local metadata subprocess is started by Rust with piped stdout and discarded stderr.
- Stdout is drained concurrently so a full pipe cannot deadlock the child.
- The retained output is capped at 1 MiB while excess bytes are still drained.
- A probe gets five seconds. On timeout DiskSage kills and reaps the child.
- Spawn, wait, timeout, non-zero exit, output-read, output-limit, and invalid-output failures become
  stable `metadata-probe-warning` evidence. Paths, stderr, and file contents are not placed in the
  warning.
- A failed probe contributes no production-time claim. Planning continues, but any probe warning
  adds `embedded-metadata-probe-incomplete` and forces human review even when another probe found an
  embedded production date.

## Scope decision

This is deterministic process supervision, so noema, contextual-orchestrator, an external LLM,
fast-mlsirm, semantic-data-portal, and pg-erd-cloud are not involved. The change is confined to the
Rust computation and evidence layer used by DiskSage.
