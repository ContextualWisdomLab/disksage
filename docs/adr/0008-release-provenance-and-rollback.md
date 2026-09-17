# ADR-0008 — Separate build, provenance, publication, and rollback authority

**Status:** Proposed canonical release decision; stronger implementation may continue to evolve.

## Context

A green build does not prove that the artifact later published is the same artifact, that it came from the accepted source, or that rollback is safe for durable evidence formats.

## Drivers

- buyer-verifiable artifact identity;
- least-privilege publication;
- exact source/workflow binding;
- SBOM/provenance evidence;
- safe upgrade/rollback;
- refusal of duplicate/unexpected artifacts.

## Alternatives considered

1. one write-capable release job builds and publishes directly — rejected for broad authority and weak separation;
2. manual upload with no provenance — rejected for poor reproducibility;
3. read/build -> verify/attest -> publish separation with exact artifact admission and rollback contract — selected.

## Decision

Release originates only from an exact integrated protected head. Build authority produces candidate artifacts without ambient publication authority. Provenance/attestation binds accepted artifacts to source/workflow identity. Publication authority can publish only the verified admitted artifact set. Release acceptance includes artifact digest, SBOM/provenance, compatibility, security, exact coverage, governance, migration/recovery, and post-publication verification.

Rollback is a separate reviewed operation that records bad/target artifact identities, durable-format compatibility, compensating migration, retained security fixes, and verification after rollback.

## Consequences

Release workflow is more structured and may require additional artifacts/jobs. That complexity purchases auditable separation and safer acquisition evidence.

## Failure and recovery

Missing/duplicate/unexpected/non-regular/digest-mismatched artifacts fail closed. A partial publication triggers incident handling and does not retroactively make the run successful. Rollback cannot intentionally reintroduce a known critical defect.

## Security/governance impact

Model/development agents do not receive publication credentials. Untrusted PR content cannot expand publication authority. Environment protections and least-privilege tokens remain part of final workflow design.

## Verification/acceptance

Release tests enumerate the expected artifact set, digests, package metadata, SBOM/provenance, source/workflow identity, publication permission, installation/smoke behavior, and post-publish artifact verification.

## Migration/rollback

Every durable format change documents forward migration, old fixtures, compatibility, and rollback or explicit irreversible boundary before release.

## Supersession

Supersede only with a release design that preserves exact-source identity, artifact admission, provenance/SBOM, least-privilege publication, and controlled rollback.