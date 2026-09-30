# ADR-0004 — Treat the on-device model as executable supply-chain input

**Status:** Accepted behavior on current protected main; this ADR is its proposed canonical record.

## Context

A downloadable GGUF influences local reasoning and is parsed by llama.cpp. HTTPS success, a familiar filename, or an earlier valid installation does not prove the bytes currently executed are the reviewed artifact.

## Drivers

- immutable dependency identity;
- bounded transfer/memory use;
- race-resistant local installation;
- protection against post-install substitution;
- privacy-safe failure evidence;
- standalone operation.

## Alternatives considered

1. trust upstream `main`/transport — rejected;
2. verify only during download — rejected because the file can later change;
3. pin and verify at installation and immediately before execution while retaining verified identity — selected.

## Decision

The default model specification binds immutable upstream revision, exact byte count, and SHA-256. Installation streams within bounds, verifies reviewed bytes, and uses race-resistant/no-clobber publication. Load-time verification rejects missing, linked, non-regular, size-mismatched, unreadable, identity-raced, or digest-mismatched artifacts and retains a verified identity through llama.cpp loading.

The digest proves artifact-byte identity only. It does not prove model behavioral safety, training provenance, absence of backdoors, quality, or licensing conclusions.

## Consequences

Existing exact-valid artifacts remain usable; invalid/tampered artifacts are refused. Model hashing adds deterministic I/O cost at the execution boundary.

## Failure and recovery

Stable path-free error categories refuse execution. Recovery obtains the reviewed artifact through the approved install path rather than bypassing verification.

## Security and governance impact

Model bytes and model output are untrusted. Model integrity cannot authorize filesystem actions or substitute for human approval.

## Verification and acceptance

Tests cover known digest vectors, immutable specification, bounded install, short/long/wrong-digest data, collision/race behavior, load-time missing/link/type/size/read/digest failures, path substitution, and source ordering before llama initialization.

## Migration and rollback

Changing the default model requires updating immutable revision, exact byte count, digest, license evidence, doctoring, tests, and CHANGELOG together. Do not roll back to mutable refs or existence-only admission.

## Supersession

Supersede only if a new artifact-verification mechanism provides equivalent or stronger immutable identity, bounded transfer, race resistance, execution-boundary revalidation, and privacy-safe errors.