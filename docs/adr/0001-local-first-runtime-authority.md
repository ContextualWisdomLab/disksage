# ADR-0001: Keep filesystem mutation authority local and Rust-owned

## Status

Proposed in PR #137. The decision reflects the current product direction but becomes the canonical protected-source ADR only after protected integration.

## Context

DiskSage can optionally consume cloud-provider evidence, Naruon contracts, or contextual-orchestrator model output. Those dependencies are useful for explanation and interoperability, but allowing a remote service, model response, or browser/UI state to become filesystem authority would weaken standalone operation, create confused-deputy risk, and make local safety depend on external availability.

## Decision drivers

- Standalone operation must remain useful without CWL services or a network connection.
- Filesystem state can change after any remote observation.
- Raw local paths and provider-local identifiers are privacy-sensitive.
- Model/provider output is untrusted data.
- Mutation requires current local revalidation, exact operator intent, and recoverable evidence.

## Alternatives considered

### Remote orchestration owns mutation

Rejected. It couples local safety to network identity and stale remote state, increases secret/path exposure, and makes another service a confused deputy for local filesystem operations.

### UI directly issues filesystem commands

Rejected. Presentation state is not durable authorization and cannot safely own path canonicalization, race handling, provider semantics, or rollback.

### Rust local authority with optional advisory integrations

Selected.

## Decision

Rust inside DiskSage owns final local validation, approval verification, mutation, rollback/recovery, and receipt generation. Svelte collects operator choices and renders evidence. Tauri exposes an allow-listed typed IPC surface. Naruon, contextual-orchestrator, provider APIs, or models may contribute bounded evidence or advice but cannot independently authorize a mutation.

## Consequences

### Positive

- Core behavior survives external outages.
- Secrets and exact filesystem coordinates remain local by default.
- Every mutation can re-check current local state at the last responsible moment.
- CWL integration stays modular rather than becoming hidden application coupling.

### Negative

- Some validation logic is duplicated locally even when an external platform has related knowledge.
- Rich integrations require explicit versioned adapters instead of shared database access.
- The desktop runtime carries more responsibility for security, recovery, and testing.

## Failure and recovery

If an optional remote dependency is unavailable, DiskSage reports that evidence as unavailable/unknown and continues only with operations whose local prerequisites remain complete. It does not broaden authority to compensate for missing remote evidence.

## Security and governance impact

External content, model output, provider responses, and CWL messages are treated as untrusted inputs. Cross-service payloads are bounded and versioned. Raw local secrets and mutation credentials are not exported as reusable integration tokens.

## Verification and acceptance

- Standalone tests must not require Naruon or contextual-orchestrator for deterministic safety behavior.
- Tauri must expose allow-listed command surfaces rather than arbitrary shell execution.
- Mutation tests must prove current Rust-side scope/fingerprint/approval checks.
- Integration failures must fail closed without silently widening local permission.

## Migration and rollback

A future design that transfers mutation authority outside the Rust runtime requires a superseding ADR, new threat model, explicit credential/tenant model, migration and rollback design, and end-to-end security evidence. Rolling back this ADR is not a configuration toggle.

## Supersession conditions

Supersede only if a reviewed architecture can prove equivalent or stronger local-state freshness, privacy, least privilege, offline/degraded safety, auditability, and recovery while moving authority elsewhere.