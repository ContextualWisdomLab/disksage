# DiskSage Release, Migration, and Rollback Contract

## Release source of truth

A release originates only from the **exact integrated protected head** selected under current repository policy. A branch head, synthetic merge, predecessor result, local build, PR description, model verdict, or copied status is not release authority.

## Required release gates

As applicable to the declared release scope, the unchanged integrated head must pass required CI, exact owned production coverage, security/SAST/dependency/secret gates, zero valid unresolved findings, required review/governance, reproducible dependency setup, supported-platform packaging, package metadata/version validation, affected accessibility evidence, migration/format compatibility and rollback/recovery tests, exact artifact-set admission, SBOM, provenance/attestation where configured, release acceptance, and post-publication verification.

Pending, skipped-required, neutral-required, cancelled, absent, failed, stale-head, predecessor-head, or synthetic-only evidence does not satisfy a release gate.

## Authority separation

Release design separates:

1. **build authority** — read source and produce candidate artifacts;
2. **attestation/provenance authority** — bind candidate artifacts to exact source/build identity;
3. **publication authority** — publish only accepted and verified artifacts.

A model or development agent receives no publication authority merely because it generated or reviewed code.

## Version and CHANGELOG

Before publication, select the version according to repository policy, update every public version manifest, move/render relevant `Unreleased` entries, validate support metadata, and ensure release notes describe user-visible, security, and migration changes without unproven compliance claims.

A version bump is not performed because one feature or documentation PR is green.

## Artifact-set admission

The release path defines the exact expected artifact families per supported platform and fails closed on missing, duplicate, unexpected, redirected, non-regular, or digest-mismatched artifacts. Artifact collection must preserve namespaces so duplicate basenames cannot silently overwrite one another before verification.

## Integrity, SBOM, and provenance

For each published artifact record the filename/type, size, cryptographic digest, exact integrated source revision, workflow/run attempt and immutable workflow source, dependency/build metadata, SBOM identity, provenance/attestation identity, and signer/publication identity where used.

Buyer verification guidance must make it possible to relate the published artifact to the release record.

## Migration acceptance

A release that changes a persisted schema, evidence format, receipt, provider-token record, or any durable state includes old-version fixtures, forward migration, preservation/collision tests, mixed-version compatibility stance, rollback or explicit irreversible boundary, and retention/security implications.

No central application database is assumed today; this rule applies to every actual durable format.

## Rollback

Rollback is a reviewed controlled release operation, not a security bypass. The rollback record identifies the bad and target artifact digests/versions, triggering incident/root cause, durable-format compatibility, migration or compensating action, security fixes that must remain, operator impact, and validation after rollback.

Do not roll back to an artifact that reintroduces a known critical defect solely because it is available.

## Partial boundaries and recovery

Runtime mutation and release publication can have partial durable boundaries. Recovery states what is already durable, what can safely be removed or retried, and what requires fresh user or repository authorization. Existence of an output path is not proof of completion.

## Release rehearsal

Before a stable release line, rehearse clean source checkout, dependency setup, test/coverage/security gates, supported-platform build, artifact enumeration/digest, SBOM/provenance, installation/smoke, migration/rollback where relevant, and publication-permission/environment protection without publishing where practical.

## Post-publication verification

After publication, independently inspect or fetch the released artifact and verify version, digest, expected contents, install/startup behavior, SBOM/provenance linkage, and release metadata. A successful upload response alone is not release completion.

## Incident reopening

If protected-main or published-artifact evidence contradicts the accepted release claim, reopen release acceptance and treat the new evidence as an incident input rather than preserving a green label by reclassification.

## Documentation

Update CHANGELOG, README/support matrix, security/support docs, operability guidance, and this contract when release behavior changes.