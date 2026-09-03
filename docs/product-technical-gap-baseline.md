# DiskSage product and technical gap baseline

The canonical product outcomes, supported capabilities, non-goals, and safety invariants are defined in the [DiskSage PRD](PRD.md). This file is the current implementation/ownership snapshot; runtime receipts, exact filesystem evidence, released contracts, protected refs, and live GitHub governance remain authoritative. The prior 2026-08-22 detailed incident baseline is preserved without loss at [docs/archive/product-technical-gap-baseline-2026-08-22.md](archive/product-technical-gap-baseline-2026-08-22.md).

**Snapshot:** 2026-09-03 (Asia/Seoul)

**Protected main:** `6125310d9ea562c0ed36db7ab940a96ac9b32e53`

**Product boundary:** Windows/Linux/macOS local-first disk-space inventory and evidence-bound reclaim; reversible deletion safety, filesystem classification/ontology, recovery, and platform adapters remain DiskSage domain truth. Optional cloud, identity, ontology, LLM, security, and architecture integrations consume released owner contracts through ports/ACLs and do not become mutation authority.

**Evidence rule:** queued/pending/skipped-required/failed/cancelled/stale/predecessor/model-only/status-only evidence is non-passing. Exact current head and independently resolved live base are required for readiness claims.

## Current dependency and ownership map

| Lane | Exact head | Base / owner relation | Current status | Required next proof |
| --- | --- | --- | --- | --- |
| PR #264 — release artifact verifier | `10b2b83f92ec08025d3e4839420033ec88d51da3` | `main`; canonical release-verifier owner | Draft. Exact-head Release has succeeded; Test has Linux jobs queued with no runner assignment, so the gate is non-terminal. | Preserve the unchanged source head while hosted jobs are merely queued; reacquire terminal Test/Security/SAST/OSV/Scorecard and current review/ruleset evidence before any Ready/merge transition. |
| PR #263 — cache-Trash fail-closed security | `3cd19880f1972ae759da0445d8f2acc1d050edad` | stacked on #264; canonical issue #170 runtime owner | Draft, mechanically mergeable. Exact-head Release succeeded; Test remains queued. | Integrate/equivalently establish #264, obtain terminal exact-head gates, then merge normally. Permanent cache-Trash deletion remains unavailable. |
| PR #315 — canonical product/public docs | `d182282d920bc005c3c445f481ef565f85cc0a63` at the start of this baseline refresh; this file update advances it again | stacked on exact #263; docs/public-surface owner only | Draft, mechanically mergeable. Non-force restack has `behind_by=0` against #263. Current review findings are being repaired on this branch. | Keep #263/#264 ownership intact, resolve only current addressed documentation findings, reacquire exact-head checks/reviews, and remain Draft until protected runtime truth is established. |
| PR #338 — contract-doc CI path filter | `bf8be647d5c312c5bb9ab45858ec6c8848ae1808` | stacked on #264 | Draft, mechanically mergeable. | Prove supported ordered `paths` semantics on the unchanged exact head and satisfy current gates/review before parent-first integration. |
| PR #337 — exact-head coverage foundation | `bb5aedafad309fce927d0b7393d325bfc565889f` | stacked on #338 | Draft, mechanically mergeable. | Preserve exact 100% thresholds and bounded diagnostics; obtain terminal exact-head Test/Release/security/review evidence after #338. |
| PR #334 — ontology-backed parallel reclaim planning | `39cef1fcafb9cc03382ce2de9f78af8c003f0d81` | independent buyer-visible reclaim stack | Draft, mechanically mergeable. | Continue current-head destructive/recovery/platform findings test-first; do not transfer stale review or predecessor checks. |

The table is a dated routing aid, not a substitute for live GitHub queries. If any head/base moves, re-resolve the stack before mutation or merge classification.

## Current product contract

1. Scan and classification are read-only until a separately authorized reclaim boundary is reached. Logical size, allocated size, object identity, active use, materialization, provider state, ontology labels, and model advice remain distinct evidence.
2. User-file removal is reversible and journaled. Permanent user-file deletion is not a supported DiskSage action.
3. The ADR-0002 automatic regenerable-cache policy is a narrow reversible exception to second human approval: only fixed catalogued macOS cache roots are eligible, and each direct child still requires fresh identity and complete active-use evidence before an OS-Trash move. Roots, unrelated children, DiskSage staging entries, active/ambiguous candidates, user files, provider data, and irreversible deletion remain outside that policy.
4. `--purge-proven-cache-trash` is inspection-only. `--execute --purge-proven-cache-trash` must fail before journal/filesystem mutation. ADR-0012 owns the requirements for any future same-object, full-descendant, freshness-bound, recoverable irreversible capability.
5. Cloud copy and local eviction are separate state transitions. Provider-local presence is not remote-sync proof; stale, incomplete, quota/auth/headroom, collision, active-use, placeholder, or identity evidence fails closed.
6. Models and optional ecosystem services may classify, explain, or recommend but cannot override deterministic safety evidence or mutate DiskSage domain truth.
7. Source/package metadata, local test results, open PRs, and development measurements are not immutable release evidence.

## Buyer-visible gaps

| Priority | Gap | Current evidence | Acceptance criterion |
| --- | --- | --- | --- |
| P0 | Safe reclaim must stay useful under real filesystem races and partial failures. | Active #334 and cache-safety lanes cover symlink/hardlink/path replacement, process descendants, Podman/Colima/runtime storage, recovery receipts, and destructive-action boundaries. | Real filesystem/process integration fixtures demonstrate no unreviewed deletion, bounded cancellation/timeouts, durable recovery evidence, and unchanged protected safety semantics across supported platforms. |
| P0 | The protected release line is blocked by a dependency stack and non-terminal hosted gates. | #264 → #263 → #315 and #264 → #338 → #337 are Draft stacks; queued hosted jobs are non-passing. | Dependency roots land first through normal protected merge with exact-head terminal gates; descendants are non-force restacked and revalidated. |
| P1 | Cross-platform capability and failure semantics are broader than the currently protected product evidence. | Windows/Linux/macOS adapters and ontology-backed reclaim work remain active PR evidence, not shipped truth. | Release notes and UI expose a verified platform capability matrix with real current-head integration evidence and bounded unsupported states. |
| P1 | Product documentation previously mixed historical cache-purge authority with the current fail-closed contract. | #315 is reconciling PRD, ADR-0002/ADR-0012, runbook, README, architecture, and this baseline while preserving #263 runtime ownership. | No current document reauthorizes permanent purge; status, preview schema, automatic reversible policy, current PR routing, and protected-main maturity agree. |
| P2 | Repository-wide 100% docstring/test/edge-case coverage is a target but is not yet proven on one integrated protected head. | #337 converges coverage scope and fail-closed diagnostic evidence without lowering thresholds. | Exact integrated head produces non-vacuous 100% owned-production statement/branch/function/line and public-doc evidence under repository policy. |

## Architecture and owner contracts

DiskSage owns its filesystem/reclaim facts. `.github`, `enterprise-architecture-core`, `context-graph-contracts`, `ConceptWeave`, `semantic-data-portal`, `contextual-orchestrator`, `noema`, `keyverse`, `EgressWeave`, `OriginWeave`, `pingora-gateway`, `quarantine-sandbox-runtime`, `appguardrail`, `wardnet`, `pg-llm-batch`, `EmbedRelay`, `fast-mlsirm`, `TEPP`, `RankWeave`, `ThreadWeave`, `inkspan`, `DiagramWeave`, and `mhtml-etl-gateway` remain canonical owners of their respective shared capabilities. DiskSage consumes only released/versioned contracts through explicit adapters/ACLs; source copies, cross-service application-table SQL, and mutable/unreleased production dependencies are defects.

For LLM work, `contextual-orchestrator` is the production routing/provider boundary. DiskSage does not own provider credential discovery or direct provider fallback. Deterministic safety, merge, security, coverage, and release gates remain independent of model judgment.

## Quality, security, and performance gaps

- Destructive/recovery acceptance uses real filesystem/path/symlink/hardlink/mount/permission/race scenarios; synthetic data is unit-test support only.
- Rust remains the preferred hot-path, numerical, performance, and security runtime. Python requires an explicit bounded validation role where a practical Rust alternative is absent.
- Web/API buyer paths use async behavior and realistic E2E/load evidence when present. A p95 ≤20 ms claim is made only for a defined, measured path and environment; sampling, exclusion, or artificial cache warming must not manufacture a pass.
- Necessary personal data is protected by purpose-bound authorization, least privilege, minimization, encryption/retention where applicable, and access/export evidence rather than blanket masking that breaks the workflow.
- CSAP/SOC 2 language is evidence-readiness only unless an actual certification/attestation exists.

## Documentation and release acceptance

AGENTS.md, CLAUDE.md, PRD/TRD, ARCHITECTURE, Context Map/Ubiquitous Language, ADR index/details, UML/ERD or truthful logical data model, UX, SECURITY/THREAT_MODEL, TEST_STRATEGY, OPERABILITY/recovery, TRACEABILITY/doctoring, README, CHANGELOG, and this baseline must stay code-current. A GitHub Pages claim requires actual publish evidence.

A release is permitted only from one exact integrated protected head after applicable Test, security/SAST, exact coverage/docstrings, package/build, SBOM, provenance, reproducibility, review, rollback/recovery, accessibility, operability, and buyer acceptance are terminal-success. Then version metadata, CHANGELOG, tag, immutable package/release assets, provenance/SBOM, and rollback evidence must be created and verified together.

## Historical evidence

The detailed 2026-08-21/22 provider incident, local disk-pressure measurements, older PR topology, and earlier architecture notes were useful evidence but are not current routing authority. They are preserved byte-for-byte in [the archived 2026-08-22 baseline](archive/product-technical-gap-baseline-2026-08-22.md) so no prior incident evidence is lost while this canonical file remains current.
