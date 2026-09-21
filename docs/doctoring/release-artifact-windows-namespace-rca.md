# Windows release artifact namespace and rerun identity RCA

**Recorded:** 2026-09-02; rerun follow-up 2026-09-04
**Owning repository:** `ContextualWisdomLab/disksage`
**Canonical repair:** PR #264, `fix/release-artifact-windows-namespace-v1`
**Affected consumer observed in the namespace RCA:** PR #319 exact head `16ea2dbddfd988245070640ae49a3f29aef6549c`

## Namespace failure evidence

PR #319 did not change the release workflow or release verifier. Its Release run `33284644937` built all three platform artifacts successfully. The Windows producer uploaded `release-disksage-windows-2022-1`; the Linux and macOS producers also completed successfully. The downstream `download-artifact-pr-compat` job `99187587649` successfully downloaded the exact three-artifact set and then failed deterministically in `Verify downloaded release artifact contract` with:

`Expected release artifact directory is missing or unsafe: release-disksage-windows-latest-1`

The failure occurred after checkout and artifact download, so it was not a runner-allocation, provider/network, checkout-permission, or product-feature failure in PR #319.

## Namespace root cause and causal owner

The release matrix uses `os: windows-2022`. The source-controlled verifier expected a `windows-latest` namespace. Those names cannot match for a correct Windows build.

The mismatch was introduced when commit `d928e3b7e47d55f6a40850f1198e03e7adc38476` added `.github/scripts/verify-release-artifacts.sh` while the existing release matrix continued to use the concrete `windows-2022` key. The causal owner is DiskSage release infrastructure, not downstream feature code and not a shared ContextualWisdomLab library.

## Namespace repair

PR #264 adds `src/lib/releaseArtifactVerifierDirectoryContract.test.ts`, materializes the complete platform artifact fixture under the real matrix namespaces (`ubuntu-22.04`, `windows-2022`, `macos-latest`), executes the source-controlled verifier, and proves mislocated Windows artifacts fail closed. The production verifier aligns its Windows expected directory with `windows-2022` and binds bundle and operational-CLI discovery to direct placement under each expected platform artifact directory instead of accepting deeper or cross-platform matches. Exact-set cardinality, checksum, bundle, CLI, non-regular-entry, attestation, and publication gates remain required.

Historical exact head `c81da762e1480be481e772806b61a4786522458a` obtained terminal-success real-artifact Release evidence for `download-artifact-pr-compat` job `99272477332`, including successful Windows, Linux, and macOS producer jobs. That evidence remains historical and does not transfer to later heads.

## Failed-job rerun failure evidence

After the stale attestation test was repaired at `dc3be87dc56db8f6a4cac881f880508f9178df66`, Release run `33856915539` was re-run with failed jobs only. Its three successful platform build jobs from the first attempt were retained. On attempt 2, downstream job `100984961525` checked out exact head `dc3be87dc56db8f6a4cac881f880508f9178df66` and invoked `actions/download-artifact` with pattern `release-disksage-*-2`. GitHub reported three artifacts in the workflow run, but zero matched that attempt-2 pattern. The verifier then failed because `release-artifacts` had not been materialized.

This is a deterministic retry-identity defect, not an artifact-content failure. The producers had named artifacts with `${{ github.run_attempt }}`. GitHub increments `github.run_attempt` for each attempt of one workflow run, while failed-job reruns keep successful jobs from the prior attempt. A downstream rerun therefore cannot assume that every producer artifact was recreated under the new attempt number. GitHub's rerun documentation also keeps the original workflow run and commit/ref identity for reruns.

## Rerun-safe repair

Commit `0e9db4bf80ace4abd2b3297c21e58d410dbbb69c` replaces attempt-scoped release artifact addresses with workflow-run-scoped `${{ github.run_id }}` addresses across build upload, PR compatibility download, tag attestation, SBOM upload, and publication download. The platform suffix remains part of each build artifact name, so matrix jobs still have disjoint artifact identities.

Both build-artifact and attested-SBOM uploads set `overwrite: true`. The pinned `actions/upload-artifact` input contract defines this as deleting a same-name artifact before uploading its replacement. This allows a producer job that itself reruns to replace only its stable per-run artifact, while successful producer artifacts that are not rerun remain consumable by downstream failed-job reruns.

`src/lib/releaseArtifactRerunContract.test.ts` prevents regression by requiring producer and consumer names to use `github.run_id`, requiring overwrite-safe producer/SBOM uploads, and rejecting attempt-scoped build/SBOM addresses. The existing verifier remains fail closed and receives the same stable run identity used in the artifact directory names; no checksum, cardinality, platform-placement, provenance, or publication gate is weakened.

## Review and governance boundary

Earlier exact heads also encountered central review-delivery failures after OIDC acquisition and central scheduler dispatch succeeded but no authenticated exact-head verdict arrived before timeout. Such failures are governance/delivery blockers, not permission to self-approve, manufacture status, weaken required checks, or transfer a predecessor verdict.

Any model-backed retry must remain on the existing ContextualWisdomLab contextual-orchestrator path with `orchestrator/free`; provider/model/paid fallback selection does not belong in this repository workflow.

## Verification and remaining acceptance

Every commit described above that is not the current exact head is historical evidence only. The current PR #264 head must independently prove Test, Release, Security/SAST/OSV/Scorecard and central required workflows. In particular, a normal Release run must build and download the real Linux/Windows/macOS set using the stable workflow-run identity, and a later failed-job rerun must remain compatible without rebuilding already-successful producers.

Downstream PRs must not receive feature-level workarounds for either release-infrastructure defect. Queued, skipped-required, stale, predecessor, synthetic-only, status-only, or model-only evidence remains non-passing.
