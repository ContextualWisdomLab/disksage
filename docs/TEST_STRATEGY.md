# DiskSage Test and Verification Strategy

## Purpose

Tests must prove product behavior and authority boundaries rather than merely execute lines. This strategy covers red-green-refactor development, realistic correctness, exact coverage, security, concurrency, compatibility, accessibility, packaging, release, documentation, and operational evidence.

## Evidence before claims

No test, coverage, compatibility, security, review, or release claim transfers from an older head. Repository verification is bound to the exact current source head and, for integration decisions, the independently resolved live base tip and current policy.

## TDD contract

For every production defect or new authority-bearing behavior:

1. write the smallest realistic regression at the intended production boundary;
2. run it and observe the expected RED reason;
3. implement the narrowest root-cause change;
4. rerun focused GREEN;
5. run the relevant full suite;
6. run exact-head CI/security/coverage gates;
7. update canonical docs/ADR/CHANGELOG if the contract changed.

Setup/import/fixture failure is not a valid production RED.

## Coverage contract

Owned production code targets exact 100% statement and branch coverage and, where tooling exposes them, exact 100% function and line coverage. Production authority behavior cannot be hidden behind coverage exclusions. Generated/vendor/platform code outside DiskSage ownership is classified explicitly rather than silently distorting the denominator.

An unsupported, empty, skipped, diagnostic-only, stale, or predecessor-head report is not passing coverage evidence.

## Public documentation contract

Public Rust and TypeScript surfaces require beginner-readable rustdoc/JSDoc/docstrings explaining purpose, input constraints, result semantics, refusals/errors, authority, and privacy implications where relevant.

`src/lib/architectureDocumentation.test.ts` protects the canonical documentation families and cross-cutting markers.

## Unit tests

Deterministic tests cover schema/version admission, path/scope validation, fingerprints/digests, approval freshness, stable reason codes, limits, archive/metadata parsing, provider normalization, model specifications, evidence redaction, and state transitions.

## Filesystem integration tests

Use isolated real filesystem behavior for existing destinations, symlinks/non-regular entries, hard links/same-file identity, source/destination replacement races, create-new/no-clobber, interrupted writes and recovery, sparse/allocation semantics where relevant, permission failures, missing parents, and source preservation.

Concurrency tests prefer deterministic seams/handoffs over timing sleeps.

## Provider tests

Keep discovery, account scope, runtime presence, capacity, placeholder state, queue state, item sync, remote proof, and eviction authorization separate. Fixtures include missing/extra/future fields, malformed values, contradictory states, duplicates, stale evidence, oversized responses, timeouts, and privacy-sensitive diagnostics.

No test may imply one evidence class proves another.

## Model artifact and AI tests

Cover immutable revision/size/digest, bounded streamed installation, short/long/digest mismatch, collision/race safety, missing/link/non-regular installed paths, load-time size/digest mismatch, pathname/identity substitution resistance, path-free stable errors, and proof that llama.cpp cannot precede required verification.

Live model-backed tests use GitHub Secret `NVIDIA_NIM_API_KEY` only when materially required. They remain separate from deterministic gates and cannot be the sole proof of a deterministic invariant.

## Security tests

Use hostile Unicode/filenames, path traversal/links, archive indexes/metadata, JSON/evidence payloads, provider sizes/types, stale/replayed approvals, malformed plans/receipts, model artifacts/output, and workflow/evidence identity. Fuzz/property tests are appropriate for parsers, bounds, identifiers, and state machines when they add meaningful coverage.

## Frontend and accessibility tests

Affected workflows verify keyboard/focus behavior, programmatic labels/status/errors, no color-only risk meaning, stale state/duplicate submission refusal, strict backend evidence parsing, backend-authored confirmation phrases, degraded/failure views, and exact-value alternatives where visualization alone is insufficient.

## Performance and resource tests

Representative buyer workloads measure scan throughput, peak memory, file-count/depth scaling, hashing, archive bounds, provider response handling, model installation verification, and UI responsiveness where material. Safety bounds are never weakened for benchmark results. Numeric SLOs require repeatable dated measurements.

## Migration and rollback tests

Durable-format changes test old fixtures, forward migration, retry/idempotency where relevant, data preservation, collisions, rollback or explicit irreversibility, mixed-version compatibility, and stale alias/source-of-truth rejection.

## Packaging and release tests

Verify version/CHANGELOG alignment, clean dependency setup, supported-platform builds, package contents, smoke tests, artifact digests, SBOM/provenance, source/workflow identity, publication authorization, post-publication verification, and rollback guidance.

## Repository automation tests

Where automation behavior is source-controlled, test exact source-head/live-base semantics, evidence-class separation, stale-head refusal, branch-local writer leases, immutable workflow sourcing, least privilege, and rejection of self-modifying repair automation.

Queued checks/reviews/provider waits are not success. The maintenance loop defers the exact waiting lane and rotates to other safe work.

## Evidence classification

| Evidence | May prove | Cannot prove alone |
| --- | --- | --- |
| focused test | narrow behavior | full release readiness |
| coverage report | measured graph execution | correctness or security |
| scanner result | that scanner's findings | review/merge authority |
| formal review | reviewer judgment | CI/security success |
| local test | local environment behavior | exact GitHub required check |
| package smoke | package usability | provenance/governance |
| model evaluation | sampled model behavior | filesystem authorization |
| documentation test | docs presence/markers | product implementation |

## Completion rule

A feature is not complete because tests exist, docs exist, or one CI lane is green. Product behavior, refusals/degraded paths, security/privacy, exact coverage, docs, migration/recovery, and required exact-head repository evidence must match the feature risk.