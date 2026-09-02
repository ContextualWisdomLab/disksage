# Windows release artifact namespace RCA

**Recorded:** 2026-09-02
**Owning repository:** `ContextualWisdomLab/disksage`
**Canonical repair:** PR #264, `fix/release-artifact-windows-namespace-v1`
**Affected consumer observed in this RCA:** PR #319 exact head `16ea2dbddfd988245070640ae49a3f29aef6549c`

## Failure evidence

PR #319 did not change the release workflow or release verifier. Its Release run `33284644937` built all three platform artifacts successfully. The Windows producer uploaded `release-disksage-windows-2022-1`; the Linux and macOS producers also completed successfully. The downstream `download-artifact-pr-compat` job `99187587649` successfully downloaded the exact three-artifact set and then failed deterministically in `Verify downloaded release artifact contract` with:

`Expected release artifact directory is missing or unsafe: release-disksage-windows-latest-1`

The failure occurred after checkout and artifact download, so it is not a runner-allocation, provider/network, checkout-permission, or product-feature failure in PR #319.

## Root cause and causal owner

The release matrix uses `os: windows-2022` and names uploaded workflow artifacts `release-disksage-${{ matrix.os }}-${{ github.run_attempt }}`. The source-controlled verifier expected `release-disksage-windows-latest-${run_attempt}`. Those names cannot match for a correct Windows build.

The mismatch was introduced when commit `d928e3b7e47d55f6a40850f1198e03e7adc38476` added `.github/scripts/verify-release-artifacts.sh` while the existing release matrix continued to use the concrete `windows-2022` key. Therefore the causal owner is DiskSage release infrastructure, not the feature code in downstream PR #319 and not a shared ContextualWisdomLab library.

## Test-first repair

PR #264 is the existing canonical repair. It adds `src/lib/releaseArtifactVerifierDirectoryContract.test.ts`, materializes the complete platform artifact fixture under the real matrix namespaces (`ubuntu-22.04`, `windows-2022`, `macos-latest`), executes the source-controlled verifier, and proves mislocated Windows artifacts fail closed. The production change is limited to aligning the verifier's Windows expected directory with `windows-2022`; exact-set cardinality, checksum, bundle, CLI, non-regular-entry, attestation, and publication gates remain unchanged.

Before this documentation-only update, PR #264 exact head `c81da762e1480be481e772806b61a4786522458a` had terminal-success Release evidence for `download-artifact-pr-compat` job `99272477332`, including successful Windows `windows-2022`, Linux, and macOS producer jobs. This is the required real-artifact verification that the predecessor base lacked.

## Review and governance blocker

The same exact head still had required `opencode-review` job `99700012078` terminal-failure. OIDC acquisition, repository-scoped GitHub App token exchange, and dispatch to the central `.github` scheduler succeeded. The job then polled for an authenticated `opencode-agent` `APPROVED` or `CHANGES_REQUESTED` review bound to exact head `c81da762e1480be481e772806b61a4786522458a` and timed out without one. This is a fail-closed review-delivery/governance blocker, not evidence that the Windows namespace repair is incorrect.

Any LLM/agent retry must use the existing `ContextualWisdomLab/contextual-orchestrator` integration with the `orchestrator/free` pool. It must not use predecessor reviews, manufacture status, self-approve, update the branch, merge, or weaken required gates.

## Verification and remaining acceptance

This documentation commit intentionally advances PR #264's exact head, so predecessor terminal-success evidence is historical only. The new exact head must rerun the repository's normal Test/Release/Security/SAST and central required workflows. In particular, the new `download-artifact-pr-compat` must again download the real Linux/Windows/macOS artifacts and pass the verifier, and OpenCode must publish an authenticated formal verdict for the new exact head before protected integration.

PR #319 should not receive a feature-level workaround. After PR #264 is integrated into the applicable base lineage, an otherwise unchanged downstream consumer should be rerun and must pass `Verify downloaded release artifact contract` against the real artifact set. Queued, skipped, stale, predecessor, synthetic, or status-only evidence remains non-passing.
