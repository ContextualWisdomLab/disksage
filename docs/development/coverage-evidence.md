# Exact-head coverage evidence

DiskSage treats code coverage as durable CI evidence rather than a locally asserted percentage. The `Test` workflow measures Rust production coverage on the exact pull-request head and emits a machine-readable `coverage-evidence` artifact only when all required metrics are exactly 100%.

## Why the workflow checks out the exact head

GitHub pull-request workflows may otherwise execute against a synthetic merge commit. That is useful for integration testing, but it is not sufficient for a review gate that claims a result about one immutable pull-request head. The coverage job therefore checks out `${{ github.event.pull_request.head.sha || github.sha }}` explicitly and copies the same value into `HEAD_SHA`.

The evidence builder rejects a missing or malformed SHA. Both `head_sha` and `commit_sha` in `coverage-evidence.json` must equal that exact 40-character commit identifier. The repository identity is taken from `GITHUB_REPOSITORY`, not from user-controlled test output.

## What is measured

The workflow uses `cargo llvm-cov` with LLVM source-based instrumentation. Branch coverage is requested explicitly with `--branch`; because cargo-llvm-cov documents branch coverage as unstable, the workflow uses an immutable dated Rust nightly with `llvm-tools-preview` instead of silently falling back to a toolchain that cannot measure the required metric.

The JSON summary is the only source for the emitted percentages. The evidence builder reads LLVM's aggregate totals and requires all of the following to be present, finite, non-empty, fully covered, and exactly 100%:

- statement coverage: LLVM region coverage, used as the statement-equivalent source-based metric;
- branch coverage: LLVM branch totals;
- function coverage: LLVM function totals; and
- line coverage: LLVM line totals.

The workflow never manufactures a percentage from a successful test exit status. Missing totals, zero denominators, partial coverage, malformed JSON, or identity drift stop the job before the artifact can be uploaded.

## Evidence contract

A valid `coverage-evidence.json` has schema version `1` and records the immutable head, repository, CI trust tier, server, workflow name, coverage command, four exact percentages, and `passed: true`. The organization review workflow independently downloads this artifact from the successful `Test` run for the same head and revalidates the contract.

The artifact is uploaded with `if-no-files-found: error`. GitHub Actions artifacts persist workflow outputs such as test and coverage results after the producing job completes, which lets the organization-level reviewer consume evidence without granting the coverage job repository-write permission.

## Fail-closed operating rule

A missing `coverage-evidence` artifact is not equivalent to passing coverage. A queued, cancelled, failed, stale-head, malformed, or less-than-100% measurement is also not passing. Engineers must add realistic tests or remove genuinely unreachable production code; they must not lower thresholds, hard-code percentages, exclude reachable production arithmetic merely to satisfy the gate, or reuse an artifact from an older head.

## References

GitHub. (2026). *Store and share data with workflow artifacts*. GitHub Docs. https://docs.github.com/en/actions/tutorials/store-and-share-data

GitHub. (2026). *Workflow artifacts*. GitHub Docs. https://docs.github.com/en/actions/concepts/workflows-and-actions/workflow-artifacts

Taiki Endo. (2026). *cargo-llvm-cov: Cargo subcommand to easily use LLVM source-based code coverage* [Computer software]. GitHub. https://github.com/taiki-e/cargo-llvm-cov
