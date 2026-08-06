# Rust package metadata and publication boundary

## Decision

DiskSage's Rust manifest is part of buyer-visible build, SBOM, provenance, incident-response, and acquisition-diligence evidence even though the desktop application is not intended for publication as a crates.io library. The `[package]` metadata therefore identifies the product and its source repository without generator placeholders, records the repository's MIT license identifier, and sets `publish = false` so an ordinary Cargo publication command cannot publish this application package to a registry.

The authoritative metadata is:

- package name: `disksage`;
- description: `Privacy-first desktop storage analysis and reclaim decision-support application.`;
- author/maintainer identity: `Contextual Wisdom Lab`;
- license expression: `MIT`;
- repository: `https://github.com/ContextualWisdomLab/disksage`;
- registry publication policy: `publish = false`.

No homepage field is added merely to duplicate the source repository. Cargo's manifest guidance distinguishes a package homepage from the repository URL and recommends against redundant values.

## Security and acquisition rationale

Cargo exposes package fields such as description, authors, repository, license, and Rust version to build scripts and compiled crates through `CARGO_PKG_*` environment variables. Placeholder values therefore propagate beyond `Cargo.toml` into downstream build evidence and support tooling. Replacing them makes generated provenance and support context attributable to the actual product rather than to a scaffolding template.

`publish = false` is a release-authority boundary, not a substitute for repository protections. It prevents Cargo registry publication for this package, while GitHub release publication, artifact signing, provenance, versioning, and repository authorization remain governed by their independent exact-head workflows and branch-protection rules. Removing or changing this field requires an explicit packaging decision and review; it must never be treated as implied authorization to publish anywhere else.

## Verification contract

`src-tauri/tests/package_metadata_contract.rs` reads the manifest from `CARGO_MANIFEST_DIR`, so the regression test is independent of the runner's current working directory. The test fails if required acquisition metadata disappears, if `publish = false` is removed, or if the original `A Tauri App` / `you` generator placeholders return.

The contract intentionally does not assert a package version bump. Version changes belong to the release-version contract and are permitted only when the integrated exact head passes the repository's release gates.

## Rollback

If a downstream tool is incompatible with one of these metadata fields, revert only the incompatible field after reproducing the tool failure and documenting the replacement contract. Do not restore generator placeholders. Do not remove `publish = false` merely to work around a packaging tool; instead, use the product's release workflow or a dedicated packaging artifact that preserves the repository's publication authorization boundary.

## Standalone and CWL integration boundary

These fields are ordinary Cargo package metadata and do not create a runtime dependency on `ContextualWisdomLab/.github`, Naruon, contextual-orchestrator, or any other CWL service. Central CI may validate the fields, and downstream services may consume resulting SBOM/provenance metadata, but DiskSage remains independently buildable and runnable.

## Reference verification note

Primary Cargo documentation was rechecked on August 7, 2026 (Asia/Seoul). Cargo documents description, authors, repository, license, homepage, and publication metadata in the manifest format; it states that `license` is an SPDX expression, that `repository` identifies the source repository, that a homepage should not redundantly duplicate repository/documentation URLs, and that `cargo package --no-metadata` merely suppresses warnings about missing human-usable metadata rather than making such metadata unimportant. Cargo also documents the corresponding `CARGO_PKG_*` build environment variables.

## References

The Rust Project Developers. (n.d.). *The manifest format*. The Cargo Book. Retrieved August 7, 2026, from https://doc.rust-lang.org/cargo/reference/manifest.html

The Rust Project Developers. (n.d.). *Environment variables*. The Cargo Book. Retrieved August 7, 2026, from https://doc.rust-lang.org/cargo/reference/environment-variables.html

The Rust Project Developers. (n.d.). *cargo package*. The Cargo Book. Retrieved August 7, 2026, from https://doc.rust-lang.org/cargo/commands/cargo-package.html
