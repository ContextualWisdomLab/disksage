# DiskSage Threat Model

## Scope

This threat model covers the local desktop runtime, filesystem and provider boundaries, on-device model artifact, bounded CWL integrations, and the software-delivery control plane. It is an engineering assurance record, not a certification claim.

## Security objectives

DiskSage protects four separate properties:

1. **local data integrity** — do not destroy, overwrite, or misclassify foreign/current data;
2. **authorization integrity** — only the exact current approved operation may mutate state;
3. **evidence privacy and integrity** — evidence says only what was measured and reveals no unnecessary private detail;
4. **software supply-chain integrity** — reviewed source, model artifacts, workflows, packages, and releases remain bound to exact identities.

## Assets

- local files, directories, archives, worktrees, container/VM storage, and cloud-synchronized material;
- private paths, provider-local identifiers, account scope, and receipts;
- action plans, fingerprints, human approval records, and recovery evidence;
- provider credentials and other secrets;
- on-device GGUF model artifact and reviewed specification;
- source revisions, workflows, dependency locks, SBOM/provenance, and release artifacts;
- formal reviews, checks, scanner findings, and release evidence;
- bounded evidence exchanged with Naruon or contextual-orchestrator.

## Trust boundaries

### Svelte UI -> Tauri IPC

The UI is not mutation authority. Rust validates the complete current request.

### Rust -> local filesystem

Names, links, metadata, type, content, permissions, allocation state, and namespace ownership are untrusted and may change concurrently.

### Rust -> provider/native service

Provider output, local-client observations, queue state, capacity, and item state are bounded evidence rather than ambient truth.

### Rust -> model

Model bytes are executable supply-chain input; model output is untrusted advisory data.

### DiskSage -> CWL service

Only explicit versioned bounded evidence crosses the service boundary. Receiving evidence does not grant filesystem authority.

### Repository -> organization control plane

Checks, reviews, scanners, statuses, and release automation are software-delivery evidence and remain separate from runtime operator authorization.

## Threat inventory

| Threat | Failure/attack | Controls | Residual risk |
| --- | --- | --- | --- |
| Path traversal | untrusted input escapes intended scope | typed/bounded destination validation, fail-closed path checks | equivalent-privilege local actor may alter state outside app control |
| Symlink/non-regular confusion | pathname redirects or changes type | non-following metadata, type checks, identity-aware handling | platform semantics require continued regression testing |
| TOCTOU race | source/staging/destination changes between checks | mutation-time revalidation, retained/open identity, create-new/no-clobber, identity-bound cleanup | malicious equivalent-privilege actor can force refusal/retry |
| Foreign-object deletion | cleanup removes another actor's replacement | invocation-owned recovery, exact identity comparison, source preservation | conservative recovery may leave manual cleanup |
| Stale/forged approval | old or mismatched intent is replayed | exact plan/scope/fingerprint binding, backend phrase, approver/rationale, expiry/clock checks | compromised local account can act as that user |
| UI authority confusion | frontend invents/broadens permission | Rust owns validation/authorization; stale-plan refusal | misleading UI remains a UX risk, not authority |
| Provider evidence confusion | runtime/capacity/queue is treated as sync/durability | independent evidence types, explicit unknown states | some providers may not expose strong proof |
| Secret disclosure | token/private detail reaches logs/model/evidence | purpose-bound secret handling, bounded stable errors, private/shareable separation | host compromise remains outside app-only controls |
| Model substitution | upstream/local GGUF changes | immutable revision, size, SHA-256, bounded installation, load-time verification, retained verified identity | byte integrity does not prove behavior/provenance |
| Model/prompt injection | content/model output attempts to become instruction | advisory model plane; deterministic Rust authority | explanation quality remains an evaluation concern |
| Resource exhaustion | hostile file/archive/response/model data consumes resources | explicit size/count/depth/time bounds and streaming | large legitimate workloads may require graceful refusal |
| Private evidence leakage | paths/digests/provider IDs escape | path-free shareable schemas, restricted dossiers, purpose limitation | explicit user export can disclose by intent |
| Cross-service confused deputy | integration request is mistaken for local permission | versioned capabilities; no ambient DB/filesystem authority | integration outage can reduce features, not authorization |
| Review/check spoofing | text/status/model verdict is treated as formal gate | exact evidence-class separation and current-head binding | unavailable reviewer may delay merge |
| Stale-head evidence | predecessor success is reused | exact source head + current live base, stale-head refusal | long checks may need rerun after movement |
| Self-modifying repair automation | CI mutates its own branch/source | writer lease and prohibition on one-shot/self-modifying repair paths | manual emergency repair still needs reviewed authority |
| Release substitution | published artifact differs from accepted build | artifact admission/digest, SBOM/provenance, separated build/attest/publish authority, post-publish verification | package-host compromise requires independent verification |

## Fail-closed abuse cases

A scan or model says “safe” -> still no mutation without exact current plan and approval.

A cloud client is running -> does not prove account ownership, item synchronization, remote durability, or eviction safety.

A model file exists -> does not authorize execution; current protected main re-verifies the reviewed artifact before llama.cpp load.

A bot writes “approved” -> not a qualifying formal review where policy requires one.

Central CI is broken -> does not authorize weaker local product/security gates.

## Privacy posture

DiskSage uses purpose-specific minimization. Shareable evidence favors bounded path-free summaries and stable codes. Paths, provider-local identifiers, sensitive offsets/digests, credentials, and detailed receipts stay private unless an explicit controlled export needs them. Least privilege, encryption where applicable, retention limits, and auditable access are preferred to blanket masking that destroys operator utility.

## AI assurance

The on-device model is optional and advisory. AI/model integrity and behavior are independently evaluated. OWASP AISVS 1.0 and NIST SP 800-218A are design inputs; neither is a certification claim.

## Verification obligations

Security-relevant source changes require a realistic failing regression, narrow root-cause repair, focused/full relevant tests, exact-current-head security/check evidence, review of privacy/recovery/migration/interoperability impact, and canonical documentation/ADR updates when an authority boundary changes.

## Residual-risk rule

If DiskSage cannot prove a required property within available evidence and bounds, it reports unknown/incomplete/blocking state. Availability never justifies converting uncertainty into permission.

## References — APA 7th

Booth, H., Souppaya, M., Vassilev, A., Ogata, M., Stanley, M., & Scarfone, K. (2024). *Secure software development practices for generative AI and dual-use foundation models: An SSDF community profile* (NIST Special Publication 800-218A). National Institute of Standards and Technology. https://doi.org/10.6028/NIST.SP.800-218A

National Institute of Standards and Technology. (2022). *Secure software development framework (SSDF) version 1.1: Recommendations for mitigating the risk of software vulnerabilities* (NIST Special Publication 800-218). https://doi.org/10.6028/NIST.SP.800-218

Open Worldwide Application Security Project. (2025). *Application Security Verification Standard 5.0.0*. https://owasp.org/www-project-application-security-verification-standard/

Open Worldwide Application Security Project. (2026). *Artificial Intelligence Security Verification Standard 1.0*. https://owasp.org/www-project-artificial-intelligence-security-verification-standard-aisvs-docs/

Supply-chain Levels for Software Artifacts. (2025). *SLSA specification, version 1.2*. https://slsa.dev/spec/v1.2/