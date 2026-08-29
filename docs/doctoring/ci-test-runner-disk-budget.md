# CI test runner disk budget

## Decision

The Ubuntu test job removes only three fixed, image-owned SDK roots that DiskSage does not invoke:
Android, .NET, and GHC. It records filesystem availability before and after cleanup and fails if
available capacity decreases. Cargo incremental compilation and test-profile debug information are
disabled because the job consumes test results, not reusable incremental state or debugger symbols.
Cloud and archive feature tests are compiled together once. This preserves the complete feature
test set while avoiding six overlapping archive and link passes in one shared Cargo target.

This is a deterministic capability boundary, not a free-space threshold. The workflow neither
walks customer data nor chooses deletion targets from observed size. Package-manager indexes are
removed only after the job's required Tauri packages have been installed.

## Incident evidence

On 2026-08-29, exact-head test runs for multiple independent DiskSage pull requests completed their
test suites but failed while Rust created `libdisksage_lib.a`. The hosted runner returned operating
system error 28 (`No space left on device`). Re-running without changing the workflow could move the
failure between pull requests but could not remove the shared capacity defect.

## Verification contract

The workflow publishes `runner_available_bytes_before`, `runner_available_bytes_after`, and
`runner_reclaimed_bytes` in the GitHub job summary. The existing full Rust, feature-specific CLI,
frontend, and production-build checks remain unchanged; feature-specific tests run through one
combined Cargo invocation.

## Reference

GitHub. (2026). *Ubuntu 24.04 software inventory*. GitHub Actions runner images.
https://github.com/actions/runner-images/blob/main/images/ubuntu/Ubuntu2404-Readme.md
