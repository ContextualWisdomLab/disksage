# Changelog

All notable changes to DiskSage are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and released versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Unreleased entries describe integrated source changes only; they are not release evidence until the repository's review, CI, security, packaging, provenance, and release-acceptance gates pass on the exact tagged commit.

## [Unreleased]

### Changed

- Add a read-only exact-photo duplicate audit that records byte identity with BLAKE3 and groups
  current materialized PNGs only when dimensions and normalized decoded RGBA16 pixels match;
  choose a displayed keeper only under unique Pareto dominance across losslessness, source bit
  depth, metadata completeness, and lineage, while ties require customer selection and all cleanup
  remains unavailable. Provider paths, Photos libraries, placeholders, symlinks, and replacement
  races remain rejected during audit; active-use evidence is reserved for a fresh execution
  preflight if cleanup is implemented later.
- Keep coverage builds compile-safe by applying the same `not(coverage)` boundary to native-copy
  identity cleanup and dependent eviction helpers; the focused authority contract remains green.
- Add durable private failure records in a separate journal directory and a receipt-bound
  cancellation command for bounded native cloud copies; bind cancellation to the active candidate,
  require provider-native local-current materialization evidence before existing-copy adoption can
  hash a destination, cap the private failure journal at 10,000 records, and bind failed-copy
  cleanup to Unix/Windows file identity while keeping shareable lineage exports path-free. Existing
  copy adoption remains explicitly non-cancellable because it performs verification only.
- Persist bounded, path-free local-volume snapshots from cloud plans with create-only files,
  content fingerprints, Unix `0400`/`0700` permissions, and shape-limited retention; surface a
  warning when incident-comparison evidence cannot be written without changing copy authority.
- Persist path-free provider-client process observations with the same bounded, create-only
  evidence contract so a stalled File Provider incident can be compared across planning loops.
- Persist redacted iCloud queue and File Provider activity summaries as bounded, create-only,
  timestamped evidence records, without retaining raw CloudDocs databases or provider dumps;
  surface persistence failure without changing copy or eviction authority.
- Gate iCloud copy plans on a path-free three-stream evidence cohort with deterministic
  fingerprints and a five-minute observation-skew ceiling; incomplete, malformed, or stale
  observations remain blocked and never become cloud-write or eviction authority.
