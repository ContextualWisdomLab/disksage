# DiskSage Architecture

## Purpose and evidence status

This document is the buyer-facing architecture map for DiskSage. It describes the
product boundary, deployment modes, trust decisions, privacy constraints, modular
integration seams, operational recovery model, and the evidence required before a
release or acquisition claim can be relied upon.

It is an architectural decision record, not a certification. References to NIST,
ISO/IEC, OWASP, SLSA, or WCAG identify design inputs and verification targets. They
do not claim formal conformity, certification, or a particular assurance level.

## Product and system context

DiskSage is a Tauri 2 desktop application with a Rust authority layer and a Svelte 5
presentation layer. Its primary job is to inspect local storage, explain reclaim
opportunities, and stage conservative actions without turning observations into
unreviewed deletion authority.

The authoritative product description and current user-visible capabilities live in
`README.md`. Security reporting and supported-version policy live in `SECURITY.md`.
Integrated source changes are recorded in `CHANGELOG.md`.

The system is divided into four logical planes:

1. **Observation plane.** Read-only scanners collect bounded filesystem, provider,
   archive, process-presence, and capacity evidence.
2. **Decision-support plane.** Rust planners and the optional on-device model explain
   candidates, uncertainty, prerequisites, and blockers. A recommendation remains
   advisory evidence.
3. **Authorization plane.** Exact fingerprints, attributed human approval, freshness,
   and fail-closed policy determine whether a narrowly scoped action is authorized.
4. **Execution and evidence plane.** Mutating commands perform only the approved
   operation, verify results, roll back invocation-owned partial output when possible,
   and emit bounded receipts.

These planes are deliberately separate. A local validator can reject unsafe input,
but it does not become durable authorization. A model response, UI state, successful
scan, local process observation, capacity estimate, or provider acknowledgement also
does not become durable authorization.

## Standalone deployment

The standalone product runs as a local desktop application:

- the Svelte UI renders evidence and collects explicit operator choices;
- Tauri IPC exposes an allow-listed command surface rather than arbitrary shell
  execution;
- Rust owns filesystem interpretation, validation, planning, hashing, approval
  verification, mutation, rollback, and receipt generation;
- local model inference is optional and remains advisory;
- private path-bearing evidence stays local unless the operator explicitly creates a
  restricted private dossier or receipt;
- path-free summaries are versioned and bounded before they can cross a process or
  service boundary.

A standalone build must remain useful without Naruon, contextual-orchestrator, a CWL
control plane, or a network connection. Network-backed provider checks are optional
capabilities with explicit failure states; their absence must not silently broaden
local authority.

## Modular MSA integration

DiskSage is designed to operate separately and as a bounded module in the wider CWL
ecosystem.

### `ContextualWisdomLab/.github`

The organization repository supplies shared review, security, provenance, and release
policy. DiskSage consumes those controls as an external control plane but keeps local
workflows sufficient to identify repository-specific failures. Shared workflow success
is necessary evidence when required by policy; it is not permission to bypass local
checks or branch protection.

### `naruon`

Naruon may consume path-free readiness envelopes, review blockers, action identifiers,
and evidence fingerprints. DiskSage must not export raw local paths, provider account
identifiers, file contents, unrestricted command output, or a reusable mutation token.
Naruon orchestration cannot convert advisory readiness into DiskSage execution
authority; the final action remains bound to DiskSage's current evidence and explicit
human approval.

### `contextual-orchestrator`

contextual-orchestrator may route model-backed explanation or evaluation when a
networked deployment explicitly enables it. DiskSage must remain functional without
that service. Model selection, recursive depth, agent roles, and reasoning effort are
orchestration concerns; filesystem authority, approval validation, and mutation stay
inside the Rust boundary. Model-backed tests use `NVIDIA_NIM_API_KEY` only through
GitHub Secrets and must not use `COPILOT_GITHUB_TOKEN`.

### Other CWL services

Integrations use versioned schemas, stable action identifiers, bounded evidence,
content fingerprints, explicit capability negotiation, and fail-closed parsing. A
consumer can reject an unsupported schema without breaking standalone operation. A
producer must not infer compatibility from a matching service name alone.

## Trust and authority boundaries

### Untrusted inputs

The following are untrusted until validated in the current operation:

- filesystem names, metadata, links, archive indexes, and file contents;
- operating-system and provider-client output;
- OAuth and provider responses;
- imported plans, receipts, and baseline snapshots;
- model output and generated explanations;
- pull-request content, review text, workflow artifacts, and external status reports;
- data received from Naruon, contextual-orchestrator, or another CWL module.

Validation is fail closed. Unknown, missing, stale, malformed, contradictory, or
out-of-range evidence remains unknown or blocking; it is never normalized to zero,
success, or approval.

### Durable authorization

#### Read-only operations

A read-only invocation requires an allow-listed operation identifier, supported schema
version, bounded source or provider scope, explicit resource limits, and a current
observation timestamp and fingerprint for every claim it emits. It never requires or
returns a reusable mutation token. It cannot promote a successful observation into
approval, provider identity, account ownership, synchronization completion, physical
reclaimability, or deletion safety.

Read-only evidence is regenerated for each invocation. An absent observation, an
unsupported schema, a changed scope, a limit breach, a stale fingerprint, or a failed
clock validation returns the stable rejection state `evidence-incomplete`. Persisted
UTC timestamps are evidence labels rather than authorization. When one process measures
elapsed time, Rust also uses a monotonic clock so wall-clock adjustment cannot make an
observation appear fresher.

#### Mutating operation contracts

Every registered mutating command must map to exactly one operation class below. The
listed inputs are mandatory, not optional examples. If a command cannot supply every
applicable input, it remains unavailable or read-only.

| Operation class | Required fingerprint and scope | Additional required authority | Maximum authorization age |
| --- | --- | --- | --- |
| Copy or create-new materialization, including cloud copy-only and incomplete-download materialization | Exact source-lineage fingerprint, exact destination-plan fingerprint, provider and account scope when applicable, bounded destination, byte total, and no-clobber collision result | Human-attributed approver, rationale, exact backend-authored confirmation phrase, explicit execution flag, and restricted receipt location | 15 minutes |
| Existing-copy adoption | Exact existing-object fingerprint, candidate identifier, destination, provider and account scope, and review fingerprint | Human-attributed approver, rationale, exact backend-authored confirmation phrase, explicit execution flag, and restricted receipt location | 15 minutes |
| Local eviction, trash, quarantine, or cleanup | Exact current candidate-set fingerprint, action class, source scope, current safety-plan fingerprint, and rollback or trash destination | Human-attributed approver, rationale, exact backend-authored confirmation phrase, explicit execution flag, and restricted receipt location | 15 minutes |
| Organize, archive, recover, or transform | Exact source-lineage fingerprint, transformation schema and version, destination-plan fingerprint, bounded destination, and collision result | Human-attributed approver, rationale, exact backend-authored confirmation phrase, explicit execution flag, and restricted receipt location | 15 minutes |

At issuance, the authorization record contains an operation identifier, schema
version, all scope fields, all fingerprints, the exact `repository_head_sha`,
`issued_at_utc`, `expires_at_utc`, approver identity, rationale, and confirmation phrase.
Rust computes `expires_at_utc` as exactly 15 minutes after `issued_at_utc`. The trusted
Rust clock evaluates UTC expiry and, for an authorization created and consumed in one
process, also requires monotonic elapsed time to remain below 15 minutes. At execution,
Rust compares `repository_head_sha` with the current integrated repository head; any
mismatch fails closed with `approval-scope-mismatch` before mutation.

At or after `expires_at_utc`, execution fails with `approval-expired`. A current UTC
clock earlier than the recorded issue time, a reversed monotonic interval, or any
inconsistent clock pair fails with `approval-clock-invalid`. A changed candidate,
destination, provider, account, action class, schema, fingerprint, phrase, or receipt
scope fails with `approval-scope-mismatch`. Revalidation that produces a different
current plan fails with `plan-stale`. No retry, workflow, service, UI state, or model
response may extend, refresh, or substitute the approval; a new plan and new human
approval are required.

Authorization is single-purpose. It cannot be reused for another candidate,
destination, provider, account scope, operation class, plan revision, schema revision,
or repository head.

### Repository authorization

A pull request may merge only on the exact current head SHA and only when every
required check, security gate, review policy, branch rule, and independent non-author
approval is satisfied. Queued, pending, cancelled, skipped-required, neutral-required,
absent, failed, stale-head, or older-head evidence is not passing.

No document, review, status, or artifact from an older head may be reused to authorize
merge or release; local validation and CI evidence remain distinct from durable
repository authorization.

## Data and privacy boundaries

DiskSage minimizes disclosure by separating evidence into two classes:

- **shareable evidence:** versioned, bounded, path-free summaries, stable blocker
  codes, aggregate byte counts, capability flags, and cryptographic fingerprints;
- **private evidence:** exact paths, provider-local identifiers, offsets, digests,
  collision details, or operator receipts written only to an explicitly requested,
  create-new, restricted local file.

Private evidence is not uploaded by default. Logs and errors use stable codes where
raw details could expose account, path, command, or content information. Evidence
schemas preserve missing observations as unknown rather than inventing values.

Storage-security design is informed by ISO/IEC 27040:2024, including lifecycle-aware
protection for stored data, storage services, media, and management activity. Security
risk management is informed by ISO/IEC 27001:2022 and its 2024 amendment. These are
design references, not certification claims.

## Reliability, migration, and rollback

### Failure model

DiskSage assumes power loss, process termination, concurrent filesystem change,
provider delay, partial copy, stale plans, unavailable model services, malformed
archives, and permission changes are normal operational conditions.

Read-only commands must be repeatable and must report incomplete observation. Mutating
commands revalidate current evidence immediately before execution and refuse stale
plans. Writes use create-new or no-clobber semantics where possible. Invocation-owned
partial output is tracked so a failed operation can remove only what that invocation
created. Source material is retained unless an independently authorized operation
explicitly governs its removal.

### Schema and database migration

Versioned evidence remains backward-readable for explicitly supported historical
formats. New readers reject ambiguous or future schema versions. Any persistent
schema change requires forward migration, rollback instructions, realistic fixtures,
and verification that old and new readers cannot confuse authority states.

Database objects must contain at least two descriptive words and use `snake_case` by
default. CamelCase or PascalCase is permitted only where an ecosystem convention
requires it. Renames require collision checks, reversible migration evidence, and a
rollback path; aliases must not create two competing sources of truth.

### Operational rollback

Rollback evidence identifies the exact version, migration, artifact digest, source
revision, and operator action. Rollback does not waive security fixes or restore
revoked credentials. A release is not considered rollback-ready merely because an
older binary exists.

## Release and acquisition evidence

A release candidate is evidence-complete only when the integrated exact head passes:

- repository tests and 100% production statement, branch, function, and line coverage;
- beginner-readable public documentation and configured docstring contracts;
- required Test, Security Scan, SAST, dependency, secret, and CodeQL checks;
- packaging and clean-install verification for supported platforms;
- artifact digest, SBOM, provenance, and release-acceptance verification;
- migration and rollback tests when state changes;
- accessibility checks for affected workflows;
- review-thread resolution and independent non-author approval; and
- branch protection and repository policy without administrative bypass.

The local entry points for this evidence are `.github/workflows/test.yml` and
`.github/workflows/release.yml`; shared required workflows may add stricter gates.
The Test workflow runs `npm run coverage` directly. The package `build` script also
starts with `npm run coverage`, and Tauri's `beforeBuildCommand` invokes that script;
therefore the Release workflow's `npm run tauri -- build` inherits the same
`npm run coverage` gate before packaging on every build-matrix platform. The Vitest
scope includes all source-controlled production TypeScript modules under `src/lib` and
`src/routes`, excluding only tests and declarations. Rust coverage remains a separate
exact-head gate owned by the Rust toolchain.

A successful workflow run is bound to its exact workflow source, base revision, head
revision, run attempt, and artifacts. A release workflow does not replace the required
Test result; both remain independently inspectable evidence.

For acquisition diligence, a reviewer should be able to trace each material claim to:

1. source and architecture documentation;
2. current tests and coverage output;
3. current security and privacy controls;
4. exact-head review and check evidence;
5. packaged artifact digests, SBOM, and provenance;
6. operational migration, rollback, and recovery evidence; and
7. the release entry in `CHANGELOG.md`.

NIST SP 800-218 SSDF practices inform the secure-development evidence model. SLSA
Version 1.2 informs source, build, provenance, and verification vocabulary. OWASP ASVS
5.0.0 informs application-security verification targets. The October 2023 W3C
Recommendation for WCAG 2.2 informs accessible user workflows. ISO/IEC 40500:2025 is a
separate ISO/IEC publication based on that October 2023 recommendation. DiskSage
records the exact control implementation and test rather than claiming blanket
compliance from a citation.

## Database object naming

Persistent database tables, indexes, views, triggers, constraints, sequences, and
migration identifiers use at least two descriptive words. The default representation
is `snake_case`, for example `cleanup_receipts`, `evidence_fingerprints`, and
`provider_capacity_snapshots`.

A migration introducing or renaming a database object must include:

- an inventory of affected readers, writers, exports, and rollback scripts;
- a deterministic forward migration;
- a deterministic rollback or documented irreversible boundary;
- data-preservation and collision tests;
- compatibility evidence for standalone and MSA operation; and
- removal timing for any temporary compatibility alias.

## Architecture change control

A change affecting trust, authority, privacy, persistence, integration schemas,
deployment, rollback, or release evidence updates this document or a linked ADR in the
same pull request. The change must state what is authoritative, what remains advisory,
what fails closed, what can cross service boundaries, and how current-head evidence
proves the claim.

## References

All references below are formatted in APA 7th style.

International Organization for Standardization. (2022). *ISO/IEC 27001:2022:
Information security, cybersecurity and privacy protection—Information security
management systems—Requirements*. https://www.iso.org/standard/27001

International Organization for Standardization. (2024). *ISO/IEC 27001:2022/Amd
1:2024: Information security, cybersecurity and privacy protection—Information
security management systems—Requirements—Amendment 1: Climate action changes*.
https://www.iso.org/standard/88435.html

International Organization for Standardization. (2024). *ISO/IEC 27040:2024:
Information technology—Security techniques—Storage security*.
https://www.iso.org/standard/80194.html

International Organization for Standardization. (2025). *ISO/IEC 40500:2025:
Information technology—W3C Web Content Accessibility Guidelines (WCAG) 2.2*.
https://www.iso.org/standard/91029.html

National Institute of Standards and Technology. (2022). *Secure software development
framework (SSDF) version 1.1: Recommendations for mitigating the risk of software
vulnerabilities* (NIST Special Publication 800-218).
https://doi.org/10.6028/NIST.SP.800-218

Open Worldwide Application Security Project. (2025). *Application Security
Verification Standard 5.0.0*.
https://owasp.org/www-project-application-security-verification-standard/

Supply-chain Levels for Software Artifacts. (2025). *SLSA specification, version
1.2*. https://slsa.dev/spec/v1.2/

World Wide Web Consortium. (2023). *Web Content Accessibility Guidelines (WCAG) 2.2*
(W3C Recommendation, October 5, 2023).
https://www.w3.org/TR/2023/REC-WCAG22-20231005/

## Reference verification note

The standards above were rechecked against their official publishers for this
architecture decision. WCAG 2.2 and ISO/IEC 40500:2025 are recorded separately because
they have distinct publishers, dates, and canonical URLs. The repository uses the
references as current design and evidence inputs and records them in APA 7th format;
certification or formal conformity requires an independent scope-specific assessment.
