# Security Policy

## Supported Versions

Security fixes are maintained on the default branch and on currently open release
preparation branches. A version or pull request is not treated as security-cleared
merely because one scanner, status, or predecessor-head workflow is green; current
repository policy and exact-source security evidence remain authoritative.

## Product security model

DiskSage is local-first. Security-relevant filesystem validation, runtime authorization,
mutation, rollback/recovery, and receipts remain in the Rust authority boundary. A scan,
model response, provider observation, frontend state, repository status, or Git reference
does not become local mutation authority by implication.

The cross-cutting threat inventory, trust boundaries, abuse cases, and residual risks are
documented in [`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md). System/deployment/authority
boundaries are documented in [`ARCHITECTURE.md`](ARCHITECTURE.md), and verification
requirements in [`docs/TEST_STRATEGY.md`](docs/TEST_STRATEGY.md).

Public/shareable errors and evidence should use bounded non-sensitive codes and contracts.
Exact local paths, provider-local identifiers, OAuth or API secrets, unrestricted command
output, model bytes, and detailed private receipts are not public evidence by default.

## Reporting a Vulnerability

Please report suspected vulnerabilities through
[GitHub Security Advisories](https://github.com/ContextualWisdomLab/disksage/security/advisories)
or
[private vulnerability reporting](https://github.com/ContextualWisdomLab/disksage/security/advisories/new)
for this repository when available. If private reporting is unavailable,
contact the maintainers privately before publishing details in a public issue.

Do not include credentials, access tokens, private filesystem paths, proprietary file
contents, or other unnecessary sensitive data in a public report. A minimal reproduction
and the affected version/commit or released artifact identity are preferred.

Maintainers should acknowledge reports within 7 days, provide a remediation
plan or status update after triage, and coordinate disclosure after a fix is
available.

## Remediation evidence

A security fix requires root-cause analysis, a regression test at the affected boundary,
the narrowest safe remediation, and exact-current-head revalidation. Addressed review
threads may be resolved; stale or unrelated feedback remains classified rather than
silently dismissed. Security gates, branch protection, review policy, and required checks
must not be weakened to make a fix mergeable.

If a provider, review service, central workflow, or scanner is unavailable, that condition
blocks only the dependent action. It does not convert pending/absent evidence into success
and does not justify bypassing the security boundary.