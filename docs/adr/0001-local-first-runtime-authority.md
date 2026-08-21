# ADR-0001 — Local-first Rust runtime authority

**Status:** Proposed canonicalization of the integrated architectural boundary.

## Context

DiskSage can observe local files, provider state, archives, model output, and optional CWL services. Allowing any remote service, UI state, or model output to become ambient filesystem authority would undermine standalone safety and make acquisition diligence dependent on hidden trust.

## Drivers

- standalone desktop usefulness;
- least privilege;
- privacy of local paths and content;
- deterministic failure behavior;
- modular CWL composition without shared-database or hidden authority coupling.

## Alternatives considered

1. central service owns all mutation — rejected because offline operation and local trust become impossible;
2. frontend owns filesystem actions — rejected because presentation state is not a durable security boundary;
3. Rust local authority with optional advisory integrations — selected.

## Decision

Rust owns security-relevant local interpretation, authorization, mutation, rollback/recovery, and receipts. Svelte and Tauri provide presentation/typed dispatch; optional provider/CWL/model components contribute bounded evidence only.

## Consequences

Integration outages degrade only dependent advisory/evidence capabilities. Cross-service contracts must remain bounded and versioned. Some workflow logic remains local even when a remote orchestrator could duplicate it.

## Failure and recovery

If a remote/provider/model dependency is unavailable, the required remote state remains unknown and dependent mutation fails closed. Local deterministic operations continue when their own evidence is sufficient.

## Security and governance impact

No service call, model answer, provider acknowledgement, Git reference, or UI state substitutes for local human authorization and current-state Rust validation.

## Verification and acceptance

Tests must prove optional integration failure does not broaden local authority and that public mutation surfaces reach Rust validation before filesystem changes.

## Migration and rollback

A future move of mutation authority out of Rust requires a superseding ADR, PRD/TRD update, threat model, migration, rollback, tenant/identity design, and equivalent or stronger deterministic tests.

## Supersession

Supersede only when a reviewed architecture introduces a different explicit local/remote authority model and the complete security evidence is integrated.