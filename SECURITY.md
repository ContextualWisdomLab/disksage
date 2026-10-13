# Security Policy

## Supported versions

Security fixes are maintained on the default branch and on currently open release-preparation branches. A branch, version, PR, or artifact is not security-cleared merely because one scanner/status/predecessor-head workflow is green; exact-current-source evidence and current repository policy remain authoritative.

## Product security model

DiskSage is local-first. Security-relevant filesystem validation, runtime authorization, mutation, rollback/recovery, and receipts remain in the Rust authority boundary. A scan, model response, provider observation, frontend state, repository status, or Git reference does not become local mutation authority by implication.

Cross-cutting threats and controls are documented in `docs/THREAT_MODEL.md`; system/deployment/authority boundaries in `ARCHITECTURE.md`; verification requirements in `docs/TEST_STRATEGY.md`; incident/recovery posture in `docs/OPERABILITY.md`.

Shareable errors/evidence use bounded non-sensitive contracts. Exact local paths, provider-local identifiers, OAuth/API secrets, unrestricted command output, model bytes, and detailed private receipts are not public evidence by default.

## Reporting a vulnerability

Please report suspected vulnerabilities through GitHub Security Advisories or private vulnerability reporting for this repository when available. If private reporting is unavailable, contact maintainers privately before public disclosure.

Do not include credentials, access tokens, private filesystem paths, proprietary file contents, or other unnecessary sensitive data in a public issue. Prefer a minimal reproduction plus affected version/commit/released-artifact identity.

Maintainers should acknowledge reports within 7 days, provide a remediation plan or status update after triage, and coordinate disclosure after a fix is available.

## Remediation contract

A security fix requires:

1. evidence-backed root-cause analysis;
2. a realistic failing regression at the affected production boundary;
3. the narrowest safe root-cause repair;
4. focused and full relevant verification;
5. exact-current-head security/check evidence;
6. review of privacy, recovery, migration, interoperability, and release impact;
7. affected canonical documentation/ADR updates.

Security gates, branch protection, review governance, and required tests/checks are not weakened to make a fix mergeable.

If a provider, review service, central workflow, or scanner is unavailable, only the dependent action is deferred. Missing/pending evidence never becomes success and the loop continues other safe work.

## Model and AI security

The on-device model artifact is executable supply-chain input and is verified against reviewed immutable identity during installation and again immediately before execution. Byte integrity does not establish behavioral safety, training provenance, absence of backdoors, or licensing conclusions.

Model outputs and external retrieved content are untrusted advisory data. They never bypass deterministic Rust validation or human authorization.

## Software supply chain

Release evidence binds exact integrated source, required checks/reviews, package artifacts, integrity digests, SBOM/provenance, migration/rollback, and post-publication verification. Build, attestation, and publication authority remain distinct where the release flow provides them.

## Responsible disclosure and residual risk

When a required property cannot be proven, DiskSage fails closed rather than redefining unknown as safe. Residual risk and unsupported evidence should be documented precisely; no standard/reference citation is a certification claim.