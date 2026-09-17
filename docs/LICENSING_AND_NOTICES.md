# DiskSage Licensing, IP, and NOTICE Evidence

## Document status

**Status:** Proposed acquisition/release governance baseline. This document records the evidence DiskSage must preserve; it is not legal advice and must not invent rights that the repository, dependency, model, contributor, or contract evidence does not actually grant.

## Current repository license

Protected `main` contains a root `LICENSE` file with the MIT License and `Copyright (c) 2026 ContextualWisdomLab`. That source-controlled file is the repository's current outbound license evidence. Package metadata and release artifacts must remain consistent with the actual root license; metadata cannot silently redefine rights.

A future outbound-license change requires explicit owner/legal authority, compatibility analysis, migration of package/release metadata and notices, and a reviewed decision. An autonomous development loop must not choose a new outbound license merely to close a diligence checkbox.

## Rights evidence model

For every releasable software or model component, distinguish these evidence classes:

1. **Repository outbound rights** — root license/custom rights declaration and copyright ownership evidence.
2. **Contributor/IP provenance** — evidence that contributions may be distributed under the repository terms, including organizational ownership/assignment policy where applicable.
3. **Dependency license evidence** — exact dependency/version/license/notice obligations for the release candidate.
4. **Bundled asset rights** — fonts, icons, fixtures, media, datasets, examples, native binaries, and other shipped assets.
5. **Model artifact rights** — model code/license, weight/artifact terms, upstream repository/revision, required attribution/use restrictions, and whether redistribution is permitted.
6. **Build/action/tool rights** — tools used to create artifacts versus components redistributed inside artifacts.
7. **Release NOTICE evidence** — all attribution, copyright, license-text, or other notice obligations required by the exact shipped set.
8. **SBOM identity** — the exact component/version/artifact inventory used to connect license findings to a release.

One class does not substitute for another. A dependency scanner cannot prove contributor ownership, and a root MIT license cannot grant rights to third-party material that the repository does not own.

## Dependency inventory

Every release candidate must produce or verify a machine-readable dependency inventory/SBOM for the exact integrated source and shipped artifacts. The release process must be able to answer:

- exact package/crate/native component and version;
- source/registry origin where material;
- declared and detected license expression/files;
- whether the component is shipped, build-only, development-only, optional, or platform-specific;
- required attribution/NOTICE/license-text obligations;
- unresolved, custom, non-standard, or conflicting license evidence;
- relationship to the exact release artifact.

Unknown or contradictory license evidence is fail-closed for a strong acquisition/release-rights claim. It must not be normalized to “permissive” by guesswork.

## NOTICE contract

`NOTICE` here means the release obligation set, not necessarily a single file with that exact name. The release process may generate one or more notice/license bundles as appropriate, but the result must be deterministic and traceable to the exact SBOM.

A releasable NOTICE set must:

- include all third-party notices/license text required for redistribution;
- preserve copyright/attribution requirements;
- identify platform-specific differences when artifacts differ;
- avoid claiming ownership of third-party works;
- bind to the same dependency/artifact set that was attested and published;
- fail closed on an unresolved obligation rather than silently omit it.

## Model and AI artifacts

The reviewed GGUF integrity contract proves artifact identity, not redistribution rights. For every model bundled or downloaded by DiskSage, retain separately:

- upstream project/model identity;
- immutable reviewed revision/version;
- model/license or terms evidence;
- weight/artifact redistribution/use restrictions where applicable;
- required attribution/NOTICE material;
- expected artifact size/digest for security identity;
- whether DiskSage downloads at runtime or redistributes the bytes;
- any buyer-relevant restrictions that affect offline, enterprise, geographic, or commercial use.

If rights evidence is incomplete, the model may be technically verifiable while still being legally unsuitable for a given release/distribution. Do not convert integrity evidence into a license conclusion.

## Assets, fixtures, and benchmark data

Test fixtures, screenshots, media, datasets, known-stem audio, example documents, and generated artifacts need provenance appropriate to their use. A test-only asset may still have redistribution restrictions in public source or CI artifacts.

Before adding a third-party asset, record source, creator/rightsholder where known, license/permission, permitted purpose, transformation/attribution requirements, and whether it is shipped, test-only, or documentation-only. Prefer self-created, synthetic, public-domain, or clearly licensed fixtures where they provide equivalent validation.

## Contributor and acquisition IP evidence

A buyer may require evidence beyond source licensing, including organizational ownership, contributor terms, employment/assignment provenance, or third-party development agreements. These are external/legal evidence classes unless they are explicitly source-controlled.

The repository should not fabricate missing contracts. `docs/ACQUISITION_DILIGENCE.md` must label missing ownership evidence as an external diligence gap rather than a technical success.

## Package metadata consistency

For each package ecosystem and desktop bundle, license, repository, product name, version, publication policy, and notice references must agree with the root source authority. A package manifest cannot silently grant broader or narrower rights than the repository's reviewed license decision.

Where package publication is intentionally disabled, that policy should be machine-tested and documented separately from end-user desktop release publication.

## SBOM and provenance relationship

SBOM answers **what is in the artifact**. Provenance/attestation answers **how and from which source/build identity it was produced**. License/NOTICE evidence answers **what redistribution/use obligations apply**. A commercially defensible release requires these to refer to the same exact artifact set.

Do not publish a notice bundle from one dependency graph beside an artifact built from another. Rebuild and regenerate when the exact integrated source or shipped artifact set changes.

## Security and privacy

License inventories and NOTICE files must not contain secrets, local filesystem paths, private provider identifiers, internal credentials, or arbitrary build-host state. Use normalized component/source identifiers and repository-relative evidence.

## Change and release acceptance

A dependency/model/asset change is incomplete when it changes shipped content without updating applicable rights evidence. Before release:

- root license and package metadata are mutually consistent;
- exact SBOM exists;
- dependency/model/asset license obligations are reviewed;
- required NOTICE/license material is present;
- unresolved rights are explicitly blocking or accepted by an authorized legal/owner decision;
- provenance and notice inventory bind the same artifacts;
- CHANGELOG/release notes do not make unsupported license or ownership claims.

## Must-not-invent rule

The repository and automation **must not invent** missing permission, ownership, license compatibility, contributor assignment, certification, or rightsholder consent. When evidence is absent, record the gap and the minimum external decision/evidence required.

See `LICENSE`, `docs/ACQUISITION_DILIGENCE.md`, `docs/RELEASE_AND_ROLLBACK.md`, and `docs/TRACEABILITY.md`.