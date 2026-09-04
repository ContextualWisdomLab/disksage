# Exact-head coverage evidence

DiskSage treats coverage as CI evidence bound to one immutable source head. A locally reported percentage, a predecessor workflow run, or a successful test exit code is not equivalent evidence.

## Exact source identity

The `Test` workflow checks out `${{ github.event.pull_request.head.sha || github.sha }}` explicitly in every checkout-bearing job. The coverage job copies the same value into `HEAD_SHA`, validates it as a 40-character commit SHA, and records it as both `head_sha` and `commit_sha` in `coverage-evidence.json`.

This deliberately distinguishes the pull-request source head from GitHub's synthetic merge commit and from the PR's historical base snapshot. A coverage artifact is valid only for the exact head that produced it.

## Rust measurement boundary

The Rust evidence job uses a dated nightly toolchain with `llvm-tools-preview` and runs:

```text
cargo llvm-cov --locked --no-cfg-coverage --no-cfg-coverage-nightly --all-features --manifest-path src-tauri/Cargo.toml --branch --json --output-path coverage.json
```

`--branch` is explicit because branch coverage is part of the repository gate. The dated nightly is intentional because cargo-llvm-cov documents branch coverage as unstable/nightly-dependent.

`--no-cfg-coverage` and `--no-cfg-coverage-nightly` are also intentional. Instrumentation must not silently alter DiskSage production `cfg` semantics and thereby change the code graph being claimed by the gate. Ordinary exact-head Rust tests and the feature-specific CLI/library proofs exercise effectful boundaries separately.

The JSON report is the sole source for emitted percentages. The evidence builder requires non-empty, finite, fully covered totals and exactly 100% for:

- statement-equivalent LLVM region coverage;
- branch coverage;
- function coverage; and
- line coverage.

Missing or malformed totals, zero denominators, partial coverage, or identity drift prevent `coverage-evidence.json` from being produced as passing evidence.

## Failure evidence

Coverage failure must remain actionable without leaking runner-local paths or unbounded command output.

If `cargo llvm-cov` produces `coverage.json` but exits because a metric is below the required threshold, `Build exact-head coverage evidence` still runs under `always() && hashFiles('coverage.json') != ''`. It writes `coverage-diagnostic.json` with repository-relative high-gap files and up to 40 sorted uncovered line numbers per file, then the exact-100% validation fails closed before the success artifact can be emitted.

If measurement itself fails before usable JSON exists, `.github/scripts/bound-coverage-command-diagnostic.sh` produces a bounded command diagnostic. The helper caps pathological individual lines, preserves both bounded log edges, prioritizes compiler errors and test panic context, and retains ANSI-colored Rust diagnostics after normalization. The workflow removes raw transient logs after redaction. If diagnostic rendering or redaction fails, the authoritative coverage exit status is preserved and the only replacement text is:

```text
coverage diagnostic rendering failed; raw diagnostic withheld
```

The same bounded diagnostic identity is carried in the artifact name with the exact head SHA. Failure diagnostics are not passing coverage evidence.

## Frontend scope

Vitest coverage includes source-controlled production TypeScript under `src/lib/**/*.ts` and `src/routes/**/*.ts`, excluding tests and generated declaration files. It does not use a hand-maintained production-file allowlist. Statement, branch, function, and line thresholds remain exactly 100%.

A frontend failure produces a bounded diagnostic with repository-relative file identity and uncovered line coordinates. It does not relax the threshold or remove production files from the denominator.

## Hosted-runner resource contract

The ordinary Test job contains several large Rust feature batches. To avoid treating hosted-runner disk/linker exhaustion as a product defect while still preserving the actual proofs:

- duplicate-audit and archive library checks use `--lib` so Cargo does not relink unrelated integration-test targets for a focused library proof;
- their dedicated CLI proofs remain explicit and `--locked`;
- `cargo clean --manifest-path src-tauri/Cargo.toml` reclaims disposable Cargo build artifacts between the duplicate-audit and archive batches; and
- `df -h .` leaves bounded disk-availability evidence in the workflow log.

The cleanup does not delete source, lockfiles, coverage thresholds, or test contracts.

## Concurrency and stale-head runs

Repeated commits to one pull-request branch can otherwise leave obsolete Test runs queued while a newer exact head becomes authoritative. The workflow therefore uses a same-ref concurrency group:

```yaml
concurrency:
  group: test-${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: ${{ github.run_attempt == 1 }}
```

The group intentionally omits source SHA so a newer first-attempt run can supersede older first-attempt work for the same ref. A manual or automated rerun (`run_attempt > 1`) is not made to self-cancel by this condition. Canceled or superseded runs remain non-passing; only the unchanged current head can supply merge evidence.

GitHub documents concurrency groups as the mechanism for limiting simultaneous workflow/job execution and canceling outdated runs. It also documents workflow artifacts as the mechanism for persisting outputs such as test and coverage results after a job completes.

## Latest measured gap and recovery

The latest completed repository-wide Rust measurement that produced usable coverage totals is predecessor head `af39bce9bb6ac3186e3940e2c94dd8381080f619` from Test run `33779617794`. It measured 64,391/80,218 regions (80.270014%), 5,292/8,991 branches (58.858859%), 3,357/4,924 functions (68.176280%), and 42,867/53,111 lines (80.712094%). Those values are RED evidence: none is reusable as passing evidence for a later head.

The bounded diagnostic identified `src-tauri/src/commands.rs` (2,364 uncovered lines), `src-tauri/src/cloud.rs` (687), `src-tauri/src/icloud_sync_health.rs` (474), and `src-tauri/src/provider_oauth.rs` (388) as the largest then-current uncovered production contributors. Recovery is therefore ordered by measured contribution instead of by arbitrary test-file count.

Current coverage-owner lineage has adopted the still-valid command-layer public coverage and HOME-absent environment fixture from historical PR #156, then adopted `src-tauri/tests/cloud_public_coverage.rs` after verifying its public cloud contracts against current `cloud.rs`. These are test-only changes; they do not narrow the denominator or substitute synthetic data for destructive/recovery acceptance. Their value is not considered inherited until an unchanged exact head compiles, runs them, and emits the next real measurement.

The next historical donor, `src-tauri/tests/icloud_sync_health_public_coverage.rs`, is not source-compatible verbatim. Current `IcloudSyncHealthReport` includes `native_status` and `file_provider_activity`; the older fixture predates those fields. Its filesystem/admission semantics remain candidates for recovery, but the donor must first be minimally adapted to the current report shape and then pass exact-head execution. Copying a stale blob merely to increase test count is not acceptable evidence.

## Operating rule

A missing `coverage-evidence` artifact is not passing. Queued, pending, canceled, failed, stale-head, malformed, less-than-100%, predecessor, or synthetic-merge evidence is non-passing. Engineers must add realistic tests or remove genuinely unreachable production code; they must not lower thresholds, hard-code percentages, narrow the production denominator, or reuse an artifact from a different head.

## References

GitHub. (2026). *Concurrency*. GitHub Docs. https://docs.github.com/en/actions/concepts/workflows-and-actions/concurrency

GitHub. (2026). *Workflow artifacts*. GitHub Docs. https://docs.github.com/en/actions/concepts/workflows-and-actions/workflow-artifacts

Endo, T. (2026). *cargo-llvm-cov: Cargo subcommand to easily use LLVM source-based code coverage* [Computer software]. GitHub. https://github.com/taiki-e/cargo-llvm-cov
