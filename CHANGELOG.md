# Changelog

All notable changes to DiskSage are documented in this file.

The format follows Keep a Changelog, and releases use Semantic Versioning.

## [Unreleased]

### Fixed

- Hardened iCloud local-copy batch eviction with fresh per-item timestamps, deterministic planner/executor/recorder/clock seams, fail-closed immutable checkpoint handling, bounded manifest admission, symlink-safe control-path validation, and distinct operator diagnostics.

### Security

- Removed obsolete one-shot repair workflows and patch scripts so repository automation no longer retains dormant write-capable recovery paths.
