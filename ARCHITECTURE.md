# DiskSage Architecture

## Document status

**Status:** Proposed canonical architecture for a clean documentation replacement based on current protected `main`. The architecture describes durable product boundaries and intentionally avoids embedding transient pull-request SHAs as timeless facts.

DiskSage is a local-first cross-platform storage intelligence and conservative reclaim application built with Tauri 2, Rust, and Svelte. Rust owns security-relevant interpretation, authorization, mutation, rollback/recovery, and receipts. The UI and optional model paths remain advisory.

## Product and system context

DiskSage exists to answer four separate questions without collapsing them into one unsafe heuristic:

1. **What consumes storage?**
2. **What does the evidence say about recoverability or reclaimability?**
3. **What exact action is being proposed and authorized?**
4. **What actually happened, and what evidence proves it?**

The product is not a generic “delete large files” utility. It treats filesystem metadata, provider state, model output, cloud capacity, process observations, archive structure, repository state, and UI state as separate evidence classes with separate authority.

## Architectural planes

### Observation plane

Read-only Rust scanners and parsers collect bounded evidence about local filesystems, volume capacity, cloud/provider state, archives, incomplete downloads, development worktrees, model artifacts, and other supported sources.

Observation is evidence only. A successful scan cannot grant mutation authority.

### Decision-support plane

Deterministic planners and optional model-assisted explanation transform observations into candidates, warnings, blockers, proposed actions, confidence/uncertainty, and exact fingerprints.

Decision support is advisory. Model output, rankings, risk labels, and fingerprints do not become human approval.

### Authorization plane

Rust validates the exact proposed operation, evidence fingerprints, destination/provider/account scope where applicable, backend-authored confirmation phrase, attributed human approval, rationale, current-state revalidation, and freshness. Runtime authorization is single-purpose and bounded in time.

A repository checkout, Git reference, green CI result, model verdict, or UI state is never runtime operator authorization.

### Execution plane

Rust performs only the authorized operation. Mutation paths prefer create-new, no-clobber, OS-trash, identity-bound cleanup, and invocation-owned rollback/recovery semantics. Concurrent namespace changes and TOCTOU are expected failure modes, not edge cases.

### Evidence plane

DiskSage emits bounded result evidence and, where explicitly requested, restricted local private dossiers or receipts. Result evidence describes what was observed or executed; it does not grant new future authority.

## Standalone and modular deployment

### Standalone desktop

DiskSage must remain useful as an independently installed desktop application without Naruon, contextual-orchestrator, or any CWL network service.

Core standalone boundaries:

- Svelte renders evidence and collects explicit operator choices.
- Tauri exposes an allow-listed typed command surface.
- Rust owns security-relevant filesystem and provider interpretation.
- The local on-device model is optional and advisory.
- Private path-bearing evidence remains local by default.
- Optional provider/network capabilities fail closed without broadening local authority.

### ContextualWisdomLab/.github

The organization repository is an external software-development control plane. It may provide reusable review, security, coverage, provenance, and release workflows. Those controls govern source integration; they do not become runtime filesystem authorization.

### Naruon

Naruon may consume versioned path-free readiness/evidence envelopes and stable action or blocker identifiers. DiskSage must not export raw filesystem paths, provider-local account identifiers, unrestricted command output, or a reusable mutation token as the default integration contract.

### contextual-orchestrator

contextual-orchestrator may route optional model-backed explanation or evaluation. DiskSage remains functional without it. Model orchestration cannot bypass deterministic Rust validation or human authorization.

### Other CWL services

Integration uses explicit versioned schemas, capability negotiation, stable reason/action identifiers, bounded evidence, fingerprints, and fail-closed parsing. Direct hidden cross-database coupling is not part of the product architecture.

## Trust and authority boundaries

The following are untrusted until validated for the current operation:

- file names, metadata, links, archive indexes, and file contents;
- operating-system and provider-client output;
- provider APIs and OAuth responses;
- imported plans, receipts, and snapshots;
- model artifacts and model output;
- UI state;
- data received from another CWL service;
- pull-request text, automated review prose, statuses, workflow artifacts, and external reports.

Unknown, missing, contradictory, malformed, stale, unsupported, or resource-incomplete evidence fails closed.

### Runtime authorization

A mutation authorization binds the exact operation class, source/candidate identity, destination/provider/account scope where applicable, current fingerprints, backend-authored phrase, attributed human approver, rationale, issuance/expiry time, and current preconditions. The current cloud-copy authorization family uses a maximum 15-minute lifetime and rejects clock inconsistencies.

Authorization cannot silently refresh after plan drift. A changed plan requires a new plan and new human approval.

### Repository authorization

Repository decisions are separate from runtime decisions. A merge or release must bind the **exact current source head** and an **independently resolved live base tip** plus the evidence classes required by current repository policy. Check runs, commit statuses, formal reviews, automated reviewer findings, scanner findings, package/provenance evidence, and branch/ruleset authority remain distinct.

Queued, pending, cancelled, skipped-required, neutral-required, absent, stale-head, predecessor-head, synthetic-only, rate-limited, action-required, and failed evidence is not success.

## Filesystem and concurrency model

### No-clobber publication

Preflight existence checks are diagnostic only. Final publication must re-establish collision safety at mutation time through create-new or equivalent no-clobber semantics where the operation creates a new artifact.

### Identity-bound cleanup

A pathname is not durable ownership. Cleanup removes only invocation-owned output or an exact captured file identity. If another actor replaces a path, DiskSage preserves the foreign replacement.

### Source preservation

Source material is retained unless a separately reviewed and exactly authorized operation governs removal. Failure cleanup must not turn a partially successful operation into unreviewed deletion authority.

## Cloud/provider evidence model

DiskSage keeps these concepts separate:

1. provider-root discovery;
2. provider/account scope;
3. local provider-client runtime presence;
4. quota/capacity evidence;
5. local placeholder/materialization state;
6. provider queue state;
7. item-level synchronization evidence;
8. remote checksum/durability evidence where available;
9. destination collision state;
10. copy/adoption receipt;
11. local-source eviction authorization.

No earlier state implies a later state. In particular, client presence does not prove account ownership; queue silence does not prove remote durability; capacity does not prove sync; and a copy receipt does not automatically authorize local eviction.

## Model artifact boundary

The default on-device GGUF is treated as executable supply-chain input. Current protected main binds the reviewed model to an immutable upstream revision, exact byte count, and SHA-256 digest; performs bounded installation with race-resistant publication; and re-verifies the installed artifact immediately before llama.cpp loading while retaining a verified identity through initialization.

A digest proves reviewed-byte identity only. It does not prove behavioral safety, absence of backdoors, training-data provenance, model quality, or license suitability.

Model bytes and model output remain untrusted inputs to the deterministic product boundary.

## Data and privacy boundaries

### Shareable evidence

May contain version identifiers, bounded path-free counts, stable result/blocker/action codes, cryptographic fingerprints, capability flags, and explicit unknown/incomplete states.

### Private evidence

May contain exact local paths, provider-local identifiers, archive offsets/ranges, detailed digests/collision coordinates, or operator receipts. Private evidence requires an explicit local destination and controlled access. It is not uploaded by default.

Purpose-bound authorization, encryption where applicable, retention limits, and auditable access are preferred over blanket masking that destroys operational utility.

## Persistence model

DiskSage does not currently claim one central relational application database. Durable data exists through workflow-specific local files, receipts, source-controlled specifications, GitHub evidence, and provider-specific local state.

`docs/DATA_MODEL.md` defines the conceptual/logical entities and explicitly distinguishes actually persisted forms from conceptual records. If relational persistence is introduced later, database objects use at least two descriptive words in `snake_case` by default and require migration/rollback evidence.

## Reliability, migration, and rollback

Expected failures include power loss, process termination, concurrent filesystem change, provider delay, partial output, malformed archives, stale plans, permission change, model unavailability, network failure, and external control-plane outages.

Design rules:

- read-only operations are repeatable and explicit about incomplete evidence;
- mutating operations revalidate current preconditions immediately before mutation;
- outputs use no-clobber/identity-aware semantics where possible;
- invocation-owned partial output has bounded recovery behavior;
- rollback never waives a security fix or fabricates evidence;
- schema/format changes require versioning, compatibility analysis, forward migration, and rollback or an explicit irreversible boundary.

## Deployment and failure domains

The local workstation is the primary runtime trust domain. Provider APIs and CWL services are optional external failure domains. A failure in one optional integration degrades only that capability and must not move authority into a less trusted layer.

Repository automation is a separate control-plane failure domain. A broken central review or coverage workflow does not justify weakening DiskSage product tests or runtime safety.

## Release and acquisition evidence

A releasable exact integrated protected head requires, as applicable:

- repository tests and exact owned production coverage;
- beginner-readable public documentation/docstrings;
- required security, SAST, dependency, secret, and CodeQL evidence;
- packaging and supported-platform compatibility;
- artifact integrity, SBOM, provenance, and release acceptance;
- migration/rollback/recovery evidence;
- accessibility evidence for affected workflows;
- zero valid unresolved findings;
- qualifying review/approval and repository/ruleset policy.

The published artifact must be independently verifiable and tied back to the exact integrated source revision. Detailed procedure is in `docs/RELEASE_AND_ROLLBACK.md`.

## Documentation and change control

The canonical documentation graph is indexed by `docs/README.md`. Product, authority, persistence, API/schema, security, deployment, release, or lifecycle changes update the affected canonical documents and traceability in the same reviewed change.

Unimplemented ideas are marked Proposed or Planned. Documentation never promotes an unmerged or unimplemented feature to shipped truth.

## References

All references are formatted in APA 7th style.

Booth, H., Souppaya, M., Vassilev, A., Ogata, M., Stanley, M., & Scarfone, K. (2024). *Secure software development practices for generative AI and dual-use foundation models: An SSDF community profile* (NIST Special Publication 800-218A). National Institute of Standards and Technology. https://doi.org/10.6028/NIST.SP.800-218A

International Organization for Standardization. (2022). *ISO/IEC 27001:2022: Information security, cybersecurity and privacy protection—Information security management systems—Requirements*. https://www.iso.org/standard/27001

International Organization for Standardization. (2024). *ISO/IEC 27001:2022/Amd 1:2024: Information security, cybersecurity and privacy protection—Information security management systems—Requirements—Amendment 1: Climate action changes*. https://www.iso.org/standard/88435.html

International Organization for Standardization. (2024). *ISO/IEC 27040:2024: Information technology—Security techniques—Storage security*. https://www.iso.org/standard/80194.html

National Institute of Standards and Technology. (2022). *Secure software development framework (SSDF) version 1.1: Recommendations for mitigating the risk of software vulnerabilities* (NIST Special Publication 800-218). https://doi.org/10.6028/NIST.SP.800-218

National Institute of Standards and Technology. (2025). *Secure software development framework (SSDF) version 1.2: Recommendations for mitigating the risk of software vulnerabilities* (NIST Special Publication 800-218 Rev. 1, Initial Public Draft). https://csrc.nist.gov/pubs/sp/800/218/r1/ipd

Open Worldwide Application Security Project. (2025). *Application Security Verification Standard 5.0.0*. https://owasp.org/www-project-application-security-verification-standard/

Open Worldwide Application Security Project. (2026). *Artificial Intelligence Security Verification Standard 1.0*. https://owasp.org/www-project-artificial-intelligence-security-verification-standard-aisvs-docs/

Supply-chain Levels for Software Artifacts. (2025). *SLSA specification, version 1.2*. https://slsa.dev/spec/v1.2/

World Wide Web Consortium. (2024). *Web Content Accessibility Guidelines (WCAG) 2.2* (W3C Recommendation, December 12, 2024). https://www.w3.org/TR/2024/REC-WCAG22-20241212/

## Reference verification note

The current publisher pages were rechecked on 2026-08-10 (Asia/Seoul). NIST SP 800-218 v1.1 remains the final SSDF baseline while SP 800-218 Rev. 1 / SSDF 1.2 remains an Initial Public Draft; NIST SP 800-218A is final. SLSA 1.2 is Approved. OWASP ASVS lists 5.0.0 as its latest stable release, and OWASP AISVS 1.0 was released in June 2026. W3C recommends using WCAG 2.2 and lists the December 2024 Recommendation as the latest published WCAG 2.2 version. These references are design inputs and do not imply certification or blanket conformance.