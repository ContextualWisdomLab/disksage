# Release version contract

## Decision

DiskSage fails closed before packaging when its buyer-visible release versions are not identical. `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json` must each expose one identical Semantic Versioning value. A tag-triggered release must additionally use the exact tag `v<manifest version>`.

The authoritative executable policy is `scripts/ci/release-version.mjs`. The package `build` command runs that policy before coverage and Vite compilation. Tauri executes `npm run build` through `beforeBuildCommand`, so the same check precedes Linux, Windows, and macOS bundle creation without relying on one operating system's shell syntax.

## Evidence contract

The verifier:

- reads each JSON manifest as UTF-8 and requires one non-empty string `version`;
- reads exactly one literal `version = "..."` from Cargo's `[package]` section and refuses absent, duplicated, or workspace-inherited ambiguity;
- requires all three values to be identical;
- requires the shared value to satisfy Semantic Versioning 2.0.0;
- treats branch and pull-request builds as version-consistency checks without inventing a release tag;
- when `GITHUB_REF` is a tag reference, requires `GITHUB_REF_NAME` to equal `v<manifest version>` exactly;
- emits stable privacy-safe diagnostics containing only repository-controlled version values; and
- runs under the ordinary read-only build authority before compilation, attestation, or publication authority exists.

`src/lib/releaseVersionContract.test.ts` verifies valid releases, prerelease/build metadata, Cargo section parsing, invalid JSON, missing and empty versions, duplicate Cargo versions, each manifest-disagreement path, malformed Semantic Versioning, tag drift, repository-root loading, and stable CLI success and failure behavior. `vitest.config.ts` includes the production verifier in the 100% statement, branch, function, and line coverage gate.

## Failure and stale-evidence behavior

A mismatch terminates `npm run build`; therefore Tauri cannot create a bundle and downstream provenance or publication jobs cannot receive release artifacts. A successful check from another commit, branch, tag, or workflow attempt is not reusable. Any manifest edit changes the exact current head and requires the complete Test, Release, security, review, approval, packaging, provenance, and release-acceptance gates to run again.

The contract does not bump versions automatically. Version changes remain explicit reviewed source changes across all three manifests and `CHANGELOG.md`. Release automation must never rewrite a tag or manifest to make a mismatch pass.

## Rollback and migration

Rollback requires an independently reviewed source revert. After a revert, run the exact-current-head coverage and packaging gates and confirm that all three manifests still agree. Do not reuse or replace assets under an existing tag; publish a new version with new provenance when replacement binaries are necessary.

## MSA compatibility

The verifier is standalone and requires no Naruon, contextual-orchestrator, model API, user data, or network access. CWL services that embed DiskSage may invoke the same package build contract or independently compare the three version sources and the deployment artifact digest before promotion.

## APA 7th references

npm, Inc. (n.d.). *Creating a package.json file*. npm Docs. Retrieved August 6, 2026, from https://docs.npmjs.com/creating-a-package-json-file/

Rust Project. (n.d.). *The manifest format*. The Cargo Book. Retrieved August 6, 2026, from https://doc.rust-lang.org/cargo/reference/manifest.html

Semantic Versioning. (n.d.). *Semantic Versioning 2.0.0*. Retrieved August 6, 2026, from https://semver.org/spec/v2.0.0.html

Tauri Programme within The Commons Conservancy. (n.d.). *Distribute*. Tauri. Retrieved August 6, 2026, from https://v2.tauri.app/distribute/

## Reference verification note

The authoritative publisher sources above were rechecked on August 6, 2026. They support the manifest locations, package version semantics, and distribution boundary used by this contract; they do not imply external certification of DiskSage.
