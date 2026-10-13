# ADR-0005 — Keep central software-delivery control separate from runtime authority

**Status:** Proposed canonicalization of existing CWL integration boundaries.

## Context

DiskSage consumes organization-level GitHub workflows and may integrate with Naruon/contextual-orchestrator. Duplicating central logic locally creates drift; letting central automation become product runtime authority creates a confused-deputy boundary.

## Drivers

- standalone product operation;
- modular MSA composition;
- single ownership of organization policy;
- least privilege across repositories;
- no hidden cross-database or filesystem authority.

## Alternatives considered

1. copy central workflows/policy implementation into DiskSage — rejected as duplication/drift;
2. make central service a required runtime dependency — rejected;
3. thin versioned integration with explicit authority separation — selected.

## Decision

`ContextualWisdomLab/.github` owns reusable software-delivery control-plane behavior. DiskSage owns repository-local product behavior, tests, and local runtime authority. Naruon and contextual-orchestrator are optional consumers/providers through bounded versioned contracts and cannot bypass Rust authorization.

## Consequences

A central defect may delay merge/release but should not be patched with unsafe leaf workarounds. DiskSage remains independently operable when external services are unavailable.

## Failure and recovery

When a central dependency fails, RCA identifies the correction owner. The DiskSage lane remains fail closed for the dependent gate while unrelated local work continues.

## Security/governance impact

Cross-repository writes require their own writer lease. No blanket secret inheritance or ambient database access is implied by integration.

## Verification/acceptance

Integration tests and documentation verify versioned schemas, fail-closed unknown versions, no implicit mutation authority, and correct degradation when optional services fail.

## Migration/rollback

A changed central contract requires compatibility/version analysis. Rollback retains a known compatible thin contract; it does not fork central implementation into DiskSage.

## Supersession

Supersede only if CWL changes product/control-plane ownership with equivalent isolation and explicit migration.