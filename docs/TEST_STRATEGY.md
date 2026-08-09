# DiskSage Test Strategy

## Objective

DiskSage tests prove safety, correctness, recoverability, privacy, and exact release evidence at the real production boundaries. A passing happy-path unit test is not enough for a feature that interprets untrusted storage or can mutate local state.

## Development discipline

Use strict red-green-refactor for defects and new behavior:

1. reproduce the production-relevant failure with the smallest deterministic RED;
2. confirm the RED fails for the intended reason;
3. implement the narrowest root-cause fix;
4. confirm focused GREEN;
5. run affected integration/security/concurrency/coverage/package gates;
6. re-fetch exact current repository evidence before completion or merge.

Do not weaken or delete a realistic regression merely to satisfy coverage or CI.

## Coverage contract

Owned production code requires 100% statement and branch coverage and, when supported by the selected toolchain, 100% function and line coverage. Public Rust/TypeScript APIs require beginner-readable documentation. Production authority code must not be hidden behind coverage exclusions.

Coverage numbers from an older commit, generated merge tree, different configuration, or partial path do not authorize the current head.

## Test layers

### Deterministic unit tests

Cover parsers, normalization, reason codes, schema/version validation, fingerprint stability/sensitivity, time math, exact confirmation phrases, data minimization, and fail-closed defaults.

### Filesystem integration tests

Use realistic temporary filesystems to cover:

- files/directories/symlinks/non-regular entries;
- hard links and sparse/allocation semantics where relevant;
- protected/root/path traversal boundaries;
- create-new/no-clobber behavior;
- copy/hash/rename/trash/recovery flows;
- unreadable/missing/changing source and destination states;
- journal/receipt creation and rollback.

### Concurrency and race tests

Use deterministic seams/barriers instead of timing sleeps when possible. Exercise source replacement, staging replacement, destination creation/replacement, stale metadata, plan drift, overlapping requests, cancellation, and late asynchronous responses.

### Provider contract tests

For OneDrive, Google Drive, iCloud/File Provider, and local provider clients:

- valid provider-specific evidence;
- wrong provider/account/path/object scope;
- missing/duplicated/malformed fields;
- impossible/inconsistent quota numbers;
- remote/local drift;
- time reversal/staleness;
- unavailable runtime/API/native tooling;
- privacy-safe error mapping.

Network integration tests must use bounded fixtures or explicit scheduled live-smoke authority; deterministic PR acceptance cannot depend on an uncontrolled live provider.

### Cloud-copy and recovery tests

Prove that copy evidence, sync evidence, and eviction permission remain distinct. Cover exact human approval, expiry, scope mismatch, source/destination drift, collision, receipt failure, adoption of an existing identical copy, incomplete-download recovery bounds, and rollback of only invocation-owned output.

### Model tests

For the active model-integrity slices:

- known SHA-256 vectors;
- immutable revision/size/digest specification validation;
- short, long, digest-mismatched and unreadable inputs;
- symlink/non-regular load refusal;
- staging/destination races and identity-aware cleanup;
- exact model load verification before llama initialization;
- no path/model-byte leakage in public error codes.

Model-backed quality tests may use `NVIDIA_NIM_API_KEY` through GitHub Secrets when needed. Deterministic safety tests do not require a model service.

### Frontend tests

Cover input validation, inaccessible/stale UI states, plan/phrase propagation, degraded/error presentation, keyboard/programmatic semantics, asynchronous race handling, and 100% production TypeScript coverage. CSP tests accompany changes to webview resource/navigation authority.

### Security tests

Include hostile Unicode/path/JSON/archive input, link/race attacks, secret redaction, prompt-injection-as-data, dependency review, SAST/CodeQL/Semgrep where configured, secret scanning, and supply-chain action/dependency pin checks.

### Performance and resource tests

Measure or deterministically bound the behavior that matters to safety: scan cardinality, parser output, decompression, response body, subprocess output, timeout, memory, cancellation, and concurrency. Performance thresholds should be based on measured buyer workloads; do not invent an SLA in tests before baseline evidence exists.

### Migration and compatibility tests

Any persistent/schema change requires old/new fixture compatibility, forward migration, rollback or explicit irreversible boundary, collision/data-preservation tests, and standalone/MSA compatibility. Database objects, if introduced, follow descriptive two-or-more-word `snake_case` naming.

### Packaging and release tests

Release acceptance verifies source version consistency, clean build/install, supported platform/runtime metadata, exact artifact set, checksums, SBOM/provenance, signature/attestation when configured, changelog/version consistency, rollback inputs, and post-package smoke behavior. PR #138 strengthens provenance but remains active work.

### Documentation contract tests

Authoritative docs are executable product evidence. Tests must keep PRD, TRD, Architecture, ADR index, UML, data model/ERD, threat model, test strategy, operability, traceability, agent guidance, security policy, and changelog discoverable and prevent critical authority language from silently disappearing.

## Realism rules

- Prefer production entry points over testing only private helpers.
- Use actual filesystem/provider data shapes rather than mocks that omit failure semantics.
- Use deterministic failure injection for permission/read/open/clock/race cases that differ across CI privilege levels.
- A skipped or ignored test is not passing evidence unless the test is explicitly non-required and that status is documented.
- For scientifically/numerically material future components, require true-parameter/property recovery and CPU/GPU parity rather than only snapshot tests.

## Exact-head CI evidence

At merge time, re-fetch the unchanged PR head and independently resolved live base tip. Required current-head tests/checks/security/review evidence must pass under actual repository policy. Queued, pending, skipped-required, cancelled, neutral-required, absent, failed, stale-head, predecessor-head, synthetic-only, or status-only evidence is not success.

## Failure triage

Every failing gate receives RCA before remediation:

1. first failing boundary and exact input/head/run;
2. reproduction or isolation;
3. recent relevant changes;
4. one falsifiable root-cause hypothesis;
5. distinct candidate remedies;
6. empirical feasibility check against permissions, workflow semantics, leases, blast radius, and rollback;
7. smallest test-first remedy;
8. exact failed-gate rerun and final state re-fetch.

After three materially distinct failed hypotheses, reassess the architecture or governing contract rather than stacking patches.

## Release-quality exit

A feature/release is not complete until realistic functional, refusal, security, concurrency, privacy, recovery, coverage, documentation, packaging, and exact repository-evidence gates applicable to that change have passed.