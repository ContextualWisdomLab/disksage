# Headless Cloud Review Design

## Problem

The headless planner can display candidates and copy automatically eligible candidates, but it
cannot record or consume the exact review decision required for metadata evidence that needs human
confirmation. Reusing a decision made for another provider is intentionally invalid because the
review fingerprint includes the destination and current evidence.

## Flow

1. Plan again from the source root and selected provider root.
2. Require both the candidate metadata fingerprint and the current review fingerprint.
3. Record either `approved` or `held` through the existing immutable, hash-bound review decision
   writer in an explicit absolute review directory. The attributed reviewer must use the
   `human:ID` namespace; an agent must use a separately integrated provenance path rather than
   impersonating a human reviewer.
4. For a later copy action, load the latest immutable decision for that candidate from the same
   directory.
5. Pass it to the existing `prepare_cloud_copy_with_review` gate, which rejects missing, held,
   stale, tampered, or destination-mismatched decisions.

The review action does not copy or modify source or destination files. The copy action still keeps
the source and refuses an existing destination.

## Interface

- Review: `--review-candidate-fingerprint`, `--review-fingerprint`,
  `--review-disposition approved|held`, `--reviewed-by human:ID`,
  `--review-rationale TEXT`, and `--review-dir`.
- Copy: existing `--copy-fingerprint` and `--receipt-dir`, plus `--review-dir` when the candidate
  requires review.
- Review, copy, attestation, and root-list actions are mutually exclusive.
- Attribution is validated before the source tree is planned so malformed or non-human identities
  fail without an expensive metadata scan.
