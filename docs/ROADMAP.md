# DiskSage Commercial Product Roadmap

## Purpose

This roadmap orders evidence-backed product work after the current protected-main baseline. Planned milestones are not shipped claims and the acquisition-quality bar is not a valuation claim.

## Prioritization principles

1. Finish and merge existing safe work before creating parallel speculative branches.
2. Close authorization, data-loss, and security gaps before convenience work.
3. Prefer complete buyer-visible vertical workflows over backend-only stubs.
4. Keep standalone operation strong and CWL integrations optional and versioned.
5. Require representative real-world accuracy, performance, and recovery evidence before commercial claims.
6. Treat accessibility, privacy, supportability, provenance, and rollback as product requirements.

## Commercial readiness milestones

### M0 — Canonical evidence and delivery baseline

**Buyer-visible outcome:** a reviewer can understand what DiskSage does, how it fails, and how a release is proven without reconstructing chat history.

Exit evidence includes the canonical PRD/TRD/Architecture/ADR/UML/ERD/API/security/threat/test/operability/release/traceability graph, exact owned production coverage, accurate package metadata, exact artifact admission, SBOM/provenance, rollback guidance, and convergence of stale broad PRs into bounded current-main replacements without losing unique work.

### M1 — Complete conservative reclaim lifecycle

**Buyer-visible outcome:** major reclaim and recovery categories provide an end-to-end Inspect → Explain → Plan → Execute → Prove flow, or remain explicitly read-only when safe execution cannot be proven.

Priorities include cache/developer artifact lifecycle, duplicate review, incomplete-download recovery/materialization, worktree cleanup only with exact retention/activity evidence, and interrupted-operation recovery UX.

### M2 — Cloud synchronization and local-eviction assurance

**Buyer-visible outcome:** users can distinguish copied locally, provider accepted, remotely durable, synchronized, and safe-to-evict without guesswork.

Provider-specific proofs, capability matrices, bounded retry/backoff, delayed-provider states, and evidence-chain UX are required. No provider capability is marketed stronger than its evidence source supports.

### M3 — Container, VM, and large-storage workflows

**Buyer-visible outcome:** high-impact developer storage consumers such as Podman/container VM storage are understandable and safely actionable when sufficient evidence exists.

Start with privacy-safe read-only evidence, distinguish logical candidates from host reclaim, and add mutation only after platform semantics and recovery are defensible.

### M4 — Representative performance and capacity evidence

**Buyer-visible outcome:** procurement and operators receive reproducible throughput, latency, memory, and scaling evidence for representative storage profiles.

Benchmark methodology records platform, filesystem, item count, depth, logical/allocated sizes, cloud involvement, archive workloads, and model-related operations. Publish variance and methodology, not only best-case numbers.

### M5 — Accessibility and operator UX hardening

**Buyer-visible outcome:** evidence, confirmation, failure, recovery, and audit workflows work by keyboard and assistive technology and do not rely on color-only meaning.

Use Figma/Product Design when interaction complexity materially benefits, then validate the implemented states rather than treating design artifacts as completion.

### M6 — Enterprise governance and interoperability

**Buyer-visible outcome:** enterprises can integrate DiskSage without granting ambient filesystem or database authority and can audit sensitive operations.

Candidate scope includes managed capability profiles, purpose-bound privileged operations, privacy/retention/export policy, stable CWL contracts, auditable support/break-glass procedure, and deployment/version compatibility guidance.

### M7 — Release and acquisition acceptance

**Buyer-visible outcome:** a clean exact integrated protected head can be built, verified, upgraded or rolled back, and independently evaluated as a distributable product.

Exit evidence includes all declared release gates, exact provenance/SBOM, supported-platform compatibility, rollback rehearsal, security/privacy/accessibility acceptance, representative performance evidence, no demo-only production paths, and version/CHANGELOG/release notes aligned with artifacts.

## Buyer-gap discovery loop

When current PRs and accepted issues are genuinely exhausted, inspect activation, reclaim accuracy, recovery confidence, cloud evidence, performance, accessibility, privacy, enterprise integration, supportability, deployment, release assurance, and incidents. Select the smallest high-impact buyer-visible slice that is independent of active writer/dependency conflicts, implement it test-first, then return to the PR queue.

## Explicitly out of scope without new ADR/PRD evidence

- permanent-delete convenience paths;
- a central cloud service becoming required for local core operation;
- model output becoming autonomous mutation authority;
- unbounded private-data upload;
- certification or provider-durability claims without evidence.