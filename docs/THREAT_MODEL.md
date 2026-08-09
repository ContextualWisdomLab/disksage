# DiskSage Threat Model

## Scope

This threat model covers DiskSage's local desktop runtime, filesystem evidence, provider integrations, optional local/network model paths, private evidence/receipts, Tauri/Svelte boundary, GitHub repository automation, and release evidence. It is a living engineering artifact, not a certification.

## Security objectives

1. Prevent observation or advice from becoming unintended mutation authority.
2. Prevent stale or mismatched human approval from authorizing a different operation.
3. Prevent path, link, archive, and concurrent filesystem races from escaping the reviewed scope.
4. Preserve private local evidence and secrets by default.
5. Prevent provider/model/repository supply-chain inputs from being trusted without exact validation.
6. Preserve standalone operation and least privilege under external outages.
7. Keep source, review, check, release, and runtime authorization evidence semantically separate.

## Assets

- user files and recoverable content;
- cloud-synchronized local sources;
- provider OAuth tokens and account scope;
- private dossiers, receipts, and detailed source lineage;
- model artifacts and model specifications;
- action plans, fingerprints, and human approvals;
- GitHub source branches, workflow definitions, release artifacts, SBOM/provenance;
- product integrity and buyer-verifiable audit evidence.

## Trust boundaries

### Local untrusted storage boundary

Names, metadata, links, file contents, archive indexes, partial downloads, sparse files, hard links, and concurrent local processes are untrusted.

### Presentation/IPC boundary

Svelte and UI state are not mutation authority. Tauri exposes only typed allow-listed commands; Rust revalidates security-relevant data.

### Provider/network boundary

Provider APIs, native provider tooling, process observations, network responses, OAuth callbacks, and capacity/sync claims are untrusted until schema/scope/freshness validation.

### Model boundary

Model bytes are supply-chain inputs. Model output is advisory untrusted data. Neither can bypass deterministic authorization or validation.

### CWL integration boundary

Naruon/contextual-orchestrator/other CWL services are optional peers. Their evidence is versioned and bounded; no peer receives ambient filesystem authority.

### Repository/CI boundary

PR descriptions, commits, reviews, comments, statuses, check runs, workflow artifacts, scanners, automated reviewers, Actions source, and release assets have independent authority semantics and must be bound to exact source identities.

## Threats and controls

| Threat | Example | Required controls |
| --- | --- | --- |
| Path traversal | `../` escapes selected root | Public path validation, security-relevant ancestor checks, fail-closed errors |
| Symlink substitution | candidate becomes link to protected content | non-following metadata, type checks, identity binding, mutation-time revalidation |
| TOCTOU race | staging or destination replaced after preflight | create-new/no-clobber publication, captured file identity, concurrent regression tests |
| Hard-link confusion | cleanup removes content shared by another name | allocation/content semantics separated; identity-aware cleanup only |
| Archive/resource exhaustion | hostile archive declares huge output | entry/decompressed-size/count/time bounds and incomplete state |
| Stale plan replay | operator approves then source changes | exact plan/evidence fingerprints, current revalidation, new approval after drift |
| Approval substitution | approval for one provider/path reused elsewhere | exact action/scope/fingerprint/phrase binding, human attribution, 15-minute expiry |
| Clock manipulation | wall clock moves backward/forward | UTC consistency and monotonic elapsed checks within one process |
| Provider spoofing | local process presence treated as sync completion | provider runtime, account, capacity, sync, remote proof, and eviction authority separated |
| OAuth/token leakage | provider token appears in log/export | purpose-bound local storage, redacted errors, no token in shareable evidence |
| Cloud data loss | copy interpreted as remote durability then source removed | separate provider confirmation and local eviction permit; retain source absent proof |
| Model artifact tampering | pre-positioned or replaced GGUF loaded | immutable revision/size/SHA-256 install and load verification in active PRs #141/#142 |
| Model prompt/output injection | content tells model to authorize deletion | model output advisory only; deterministic policy and human approval remain independent |
| Webview injection | unexpected remote content/script/navigation | reviewed CSP contract in active PR #139; Tauri allow-listed surface |
| Sensitive evidence disclosure | raw paths/account IDs sent to another service | private/shareable evidence split, bounded path-free schemas, explicit private output |
| Malicious PR/review text | comment attempts to influence automation as authority | treat review text as untrusted feedback; verify formal review/check/ruleset state independently |
| Stale CI reuse | predecessor head was green | exact-current-head/live-base evidence, no older-head authorization reuse |
| Reviewer impersonation | comment/status looks like approval | eligible formal non-author review only when approval is required |
| Workflow supply-chain drift | reusable action branch changes | immutable workflow/action source pinning where privileged, least privilege, provenance checks |
| Self-modifying repair automation | temporary workflow writes arbitrary patches | prohibit repair finalizers/self-modifying workflows as steady-state mechanism; CAS-bound edits/trusted checkout |
| Release substitution | published asset differs from tested asset | exact integrated head, checksums, artifact-set admission, SBOM/provenance, release acceptance |
| Dependency compromise | package/action update introduces malicious code | lockfiles, dependency/security scans, immutable action pins, package build/install tests |
| Denial of service | huge input or provider hang freezes desktop | explicit resource/time/output bounds, cancellation, bounded subprocess handling |

## STRIDE-oriented review

### Spoofing

Relevant identities include human approver, provider/account scope, model artifact identity, release source revision, reviewer identity, and workflow source. Identity must be cryptographically or platform-authoritatively bound where meaningful; display text is not identity.

### Tampering

Plan/evidence fingerprints, restricted create-new records, digests, no-clobber writes, file-identity checks, exact-head CI, and release provenance provide tamper-evidence or tamper resistance appropriate to each boundary.

### Repudiation

Human approvals carry identity/rationale and bounded timestamps. Mutation receipts record exact operation evidence. Repository merges/releases retain GitHub review/check/source evidence. This is auditability, not non-repudiation in the legal/cryptographic-signature sense unless separately implemented.

### Information disclosure

The default cross-boundary representation is path-free and bounded. Secrets, local paths, account identifiers, command output, model bytes, and private receipts are not shareable by default.

### Denial of service

Untrusted files, archives, network bodies, subprocesses, scans, and model requests require bounded memory/time/count/output. Failure to complete an observation returns incomplete evidence rather than partial success.

### Elevation of privilege

The critical elevation risk is promotion from observation/advice to mutation authority. The typed observation→plan→approval→execution separation, Tauri/Rust boundary, exact human approval, and last-moment revalidation prevent that promotion from occurring implicitly.

## Abuse cases

### "Delete the largest files automatically"

Rejected as a product policy shortcut. Size alone is insufficient authority.

### "Provider app is running, so evict local copy"

Rejected. Runtime presence does not prove account, sync, remote checksum, or eviction safety.

### "The model says this is safe"

Rejected as mutation authority. Model advice may inform the user but cannot satisfy human approval or deterministic evidence requirements.

### "The previous PR head passed all checks"

Rejected as current merge authority. A changed head requires fresh relevant evidence.

### "CI is delayed, so bypass and fix later"

Rejected. Waiting blocks only that action; automation rotates to other safe work without weakening gates.

## Security testing expectations

- hostile Unicode/path/link inputs;
- race tests at source/staging/destination boundaries;
- malformed provider/API/native-tool output;
- stale/mismatched/expired approval tests;
- resource exhaustion and timeout tests;
- private/shareable evidence redaction tests;
- model artifact short/long/digest/race/load tests when #141/#142 integrate;
- CSP and frontend navigation/resource tests when #139 integrates;
- dependency, secret, SAST/CodeQL/scanner checks;
- exact-head repository evidence tests;
- package/SBOM/provenance/release artifact verification.

## Residual risks

- A malicious process running with equivalent user privileges may mutate local files after a successful observation or verification.
- Cryptographic digest identity cannot prove model quality or absence of model backdoors.
- Provider APIs and operating-system private interfaces can change semantics.
- Human operators can approve risky actions despite accurate warnings.
- Organization/reviewer availability can delay protected merges without indicating a product defect.

These risks are documented rather than hidden; mitigations reduce but do not eliminate them.

## Review triggers

Update this threat model when a change adds a new mutation class, provider, external service, credential, persistence layer, model provider, autonomous agent authority, release channel, webview capability, or private evidence export. Link the corresponding ADR and tests in `docs/TRACEABILITY.md`.