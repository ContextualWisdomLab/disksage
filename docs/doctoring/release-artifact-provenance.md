# Release artifact provenance

## Decision

DiskSage treats provenance as a release gate rather than optional release metadata. A tagged release may be published only after all operating-system build jobs finish, the exact uploaded artifact set is downloaded into a clean tag-only job, every shipped operational CLI checksum is verified, and GitHub creates signed build-provenance attestations for the files that will be published.

This design keeps three authorities separate:

1. `build` compiles and tests release candidates with read-only repository access, then uploads ephemeral workflow artifacts.
2. `attest-release` receives only `contents: read`, `id-token: write`, and `attestations: write`. It verifies the expected platform and CLI set before generating provenance.
3. `publish-release` receives `contents: write` only after `attest-release` succeeds. It cannot publish an unattested build because it has a durable `needs: attest-release` dependency.

Pull requests and manual non-tag builds still produce inspectable artifacts, but they cannot request an OpenID Connect identity, create durable attestations, or publish a GitHub Release through these jobs.

## Evidence contract

The authoritative implementation is `.github/workflows/release.yml`.

The release contract requires all of the following:

- checkout binds every platform build to `github.event.pull_request.head.sha` for pull requests and `github.sha` for tags or manual runs, rather than silently treating a generated pull-request merge ref as exact-head evidence;
- release concurrency uses `github.event_name == 'pull_request'`: a newer run for the same repository PR cancels its superseded pull-request build, while tag and manual release runs use unique run IDs and are never cancelled by PR activity;
- the three platform builds upload the exact bundle and operational CLI paths that later jobs consume;
- release workflow artifacts use the `release-disksage-*` namespace, which excludes concurrently uploaded `disksage-gpu-*` diagnostic bundles;
- attestation and publication downloads preserve each workflow artifact in its own directory instead of flattening archives, so duplicate basenames remain observable and last-writer-wins extraction cannot erase evidence before admission;
- release publication is absent from the matrix build job, preventing any matrix member from publishing before the complete set exists;
- the attestation and publication jobs run only for `refs/tags/`;
- the attestation job depends on the complete build matrix;
- Linux `.deb` and `.AppImage`, Windows `.msi` and NSIS `.exe`, and macOS `.dmg` bundles are present exactly once in their expected bundle paths;
- all six platform-specific operational CLIs and all six corresponding `.sha256` files are each present exactly once;
- the preserved release tree contains exactly 18 regular files (the five desktop bundles, six operational CLIs, six adjacent checksums, and one source-bound SPDX SBOM) and no symlink, device, socket, FIFO, or other non-regular entry, so unreviewed debug output, logs, dumps, or unrelated executables cannot become attested release subjects;
- every checksum file contains exactly one SHA-256 record naming its adjacent expected CLI basename, so alternate, absolute, traversing, or decoy filenames are rejected before digest verification;
- each checksum is verified before provenance generation;
- the attestation job checks out the exact tag source, validates the locked Cargo and npm dependency manifests, generates a deterministic SPDX 2.3 SBOM, and rejects a namespace that is not bound to `github.sha`;
- `actions/download-artifact` is immutably pinned to commit `37930b1c2abaa49bbe596cd826c3c89aef350131`, the upstream `v7.0.0` tag commit;
- `actions/attest` is immutably pinned to commit `59d89421af93a897026c735860bf21b6eb4f7b26`, the upstream `v4.1.0` tag commit;
- every published file is a subject of the generated attestation; and
- publication depends on successful attestation rather than merely running in parallel with it.

GitHub's action emits an in-toto Statement v1 containing a SLSA Provenance v1 predicate. SLSA specification version 1.2 is the current approved framework version, while the stable build-provenance predicate URI remains `https://slsa.dev/provenance/v1`.

## Buyer and operator verification

Download one release artifact without renaming or modifying it, install a current GitHub CLI, authenticate if the repository visibility requires it, and run:

```bash
gh attestation verify PATH/TO/ARTIFACT -R ContextualWisdomLab/disksage
```

The verifier must bind the artifact digest to `ContextualWisdomLab/disksage`. A successful result demonstrates that GitHub Actions produced an attestation for those exact bytes; it does not independently prove that the software is defect-free, that every dependency is trustworthy, or that the build platform satisfies a claimed SLSA level. Those are separate review and assurance questions.

For offline evidence collection, download the attestation bundle while network access is available:

```bash
gh attestation download PATH/TO/ARTIFACT -R ContextualWisdomLab/disksage
```

Retain the artifact, the downloaded bundle, the release tag, the source commit SHA, and the successful release workflow URL together. Do not substitute an attestation for a differently named or older artifact, even when the version string appears identical.

## Failure and stale-evidence behavior

The pipeline fails closed when an expected platform bundle, operational CLI, checksum file, or source-bound SBOM is absent or duplicated. Artifact namespaces remain separate during download, so the same required filename contributed by two platform archives remains two filesystem entries and is rejected rather than silently overwritten. It validates checksum-record semantics and digests first so an invalid or redirected record receives the specific actionable diagnostic, then rejects any nineteenth regular file and every non-regular filesystem entry before attestation or publication. This exact-set rule prevents a build step from silently adding an unreviewed diagnostic archive, crash dump, log, secret-bearing output, or unrelated executable to the release. Path-scoped checks distinguish the Windows NSIS installer from the two separately shipped Windows operational CLI executables. The pipeline also fails when a checksum record names a file other than its adjacent operational CLI, contains additional fields or records, or presents a malformed digest. A checksum mismatch or malformed/private-path SBOM stops the attestation job. A failed, cancelled, skipped, neutral, missing, or stale-head attestation job cannot satisfy the publication dependency.

Release concurrency cancels an existing run only when the incoming event is a first-attempt pull request (`github.event_name == 'pull_request' && github.run_attempt == 1`) in the same workflow, repository, and PR-number group. Tag and manual runs use `github.run_id`, so they cannot cancel each other or be cancelled by PR activity. Partial reruns are unsupported because platform artifacts are attempt-scoped; use **Re-run all jobs** so every required platform artifact is rebuilt under one attempt. Each attempt remains non-authoritative until every required exact-head job for that run completes successfully.

Attestations bind artifact digests, not mutable filenames. Rebuilding the same version produces different bytes and therefore requires new exact-build attestations. Evidence from an earlier workflow run or commit must never authorize publication of a later head.

## Privacy and security boundaries

The attestation describes build provenance, artifact digests, and the source-bound dependency inventory. It must not include API keys, user data, local disk inventory, file paths from an operator workstation, model prompts, cleanup plans, or dynamic command output containing private host information. GitHub Secrets remain unavailable to pull-request-controlled release tests unless a separately reviewed workflow explicitly requires them. The exact 18-file allowlist is also a privacy boundary: unexpected diagnostics and transient build outputs are rejected rather than made durable through an attestation or GitHub Release. The SPDX SBOM namespace binds the package inventory to the exact source revision and the workflow validates it before attestation.

All third-party actions in the release path use immutable 40-character commit SHAs. The attestation job receives no `contents: write` permission, and the publication job receives neither `id-token: write` nor `attestations: write`. This separation limits the impact of a compromised publication or attestation step.

## Rollback and migration

Rollback is a workflow-source revert, not deletion or reuse of old attestations:

1. revert the provenance workflow commit through an independently reviewed pull request;
2. rerun all exact-current-head test, security, packaging, and release-acceptance checks;
3. do not publish a replacement tag until the approved workflow state is on the protected branch; and
4. document why provenance was removed or changed in `CHANGELOG.md` and the release notes.

Already published attestations remain historical evidence for their original artifact digests. They must not be presented as evidence for replacement binaries. If a release artifact is withdrawn, mark the GitHub Release accordingly and publish a new version with new provenance rather than silently replacing assets under the same tag.

## MSA compatibility

Provenance is attached at the DiskSage release boundary and does not require `naruon`, `contextual-orchestrator`, or organization-central services at runtime. CWL services may consume the same verification contract as a module integration gate: verify the artifact against `ContextualWisdomLab/disksage`, bind the verified digest in deployment metadata, and preserve that digest across promotion and rollback.

## APA 7th references

GitHub. (n.d.). *Using artifact attestations to establish provenance for builds*. GitHub Docs. Retrieved August 6, 2026, from https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/use-artifact-attestations

GitHub. (n.d.). *Verifying attestations offline*. GitHub Docs. Retrieved August 6, 2026, from https://docs.github.com/en/actions/how-tos/secure-your-work/use-artifact-attestations/verify-attestations-offline

in-toto Project. (n.d.). *in-toto attestation framework specification (Version 1.2)*. GitHub. Retrieved August 6, 2026, from https://github.com/in-toto/attestation/blob/v1.2.0/spec/README.md

Supply-chain Levels for Software Artifacts. (n.d.). *SLSA specification (Version 1.2)*. The Linux Foundation. Retrieved August 6, 2026, from https://slsa.dev/spec/v1.2/

Supply-chain Levels for Software Artifacts. (n.d.). *Build: Verifying artifacts (Version 1.2)*. The Linux Foundation. Retrieved August 6, 2026, from https://slsa.dev/spec/v1.2/verifying-artifacts

SPDX Workgroup. (2022). *SPDX specification (Version 2.3)*. Linux Foundation. https://spdx.github.io/spdx-spec/v2.3/

## Reference verification note

The sources above were rechecked against their authoritative upstream locations on August 6, 2026. GitHub documentation was used for the supported action permissions and verification commands; upstream tag comparisons established the immutable action commits; the SLSA and in-toto specifications were used for provenance semantics and attestation structure. This document makes no unsupported claim that the workflow alone certifies a particular SLSA level.
