# DiskSage product and technical gap baseline

The canonical product outcomes, supported capabilities, non-goals, and safety invariants are defined in the [DiskSage PRD](PRD.md). This file is the current implementation/ownership snapshot; runtime receipts, exact filesystem evidence, released contracts, protected refs, and live GitHub governance remain authoritative. The prior 2026-08-22 detailed incident baseline is preserved without loss at [docs/archive/product-technical-gap-baseline-2026-08-22.md](archive/product-technical-gap-baseline-2026-08-22.md).

**Snapshot:** 2026-09-05 (Asia/Seoul)

**Protected main:** `6125310d9ea562c0ed36db7ab940a96ac9b32e53`

**Product boundary:** Windows/Linux/macOS local-first disk-space inventory and evidence-bound reclaim; reversible deletion safety, filesystem classification/ontology, recovery, and platform adapters remain DiskSage domain truth. Optional cloud, identity, ontology, LLM, security, and architecture integrations consume released owner contracts through ports/ACLs and do not become mutation authority.

**Evidence rule:** queued/pending/skipped-required/failed/cancelled/stale/predecessor/model-only/status-only evidence is non-passing. Exact current head and independently resolved live base are required for readiness claims.

## Current dependency and ownership map

| Lane | Exact head | Base / owner relation | Current status | Required next proof |
| --- | --- | --- | --- | --- |
| PR #264 — release artifact verifier | `188d7b9f90973983f90c4e7246ecec30f26270cc` | `main`; canonical combined #341/Test + release-verifier owner | Draft, mechanically mergeable. Exact-head Release `33887808512` is terminal success; Test `33887808500` remains queued and therefore non-passing. | Preserve the unchanged source head while Test/security/review evidence is non-terminal; reacquire every applicable gate and live ruleset immediately before Ready/merge. |
| PR #263 — cache-Trash fail-closed security | `0401a4f90aa517e8c93499287d593fed29b1ea73` | stacked on exact #264; canonical issue #170 runtime owner | Draft, mechanically mergeable after a non-force two-parent restack. Fresh #264→#263 is `behind_by=0` with merge base exactly #264; semantic delta remains the 13 cache/security files. Test `33889803678` is queued and Release `33889803695` is in progress. | Integrate/equivalently establish #264, obtain terminal exact-head gates, then merge normally. Permanent cache-Trash deletion remains unavailable. |
| PR #315 — canonical product/public docs | resolve live on `codex/canonical-prd` | stacked on exact #263; docs/public-surface owner only | Draft. Non-force restack preserved exactly ten documentation/public files, and this snapshot refreshes live routing without moving runtime authority into docs. | Re-resolve the branch after each docs commit, keep #263/#264 ownership intact, reacquire exact-head checks/reviews, and remain Draft until protected runtime truth is established. |
| PR #338 — contract-doc CI path filter | `7440cebfdf1a0670826a1d583ae63e972d8ec798` | stacked on exact #264 | Draft, mechanically mergeable; semantic delta remains `.github/workflows/test.yml` plus `src/lib/testWorkflowPathFilterContract.test.ts`. Exact-head Test is queued and Release is non-terminal. | Prove supported ordered `paths` semantics on the unchanged exact head and satisfy current gates/review before parent-first integration. |
| PR #337 — exact-head coverage foundation | `969f790ef92d2b6c2705281f2d468008ac63c93d` | stacked on exact #338 | Draft, mechanically mergeable. Exact 100% thresholds remain unchanged; command/cloud/adapted-iCloud/real-Git-worktree donor evidence is present, while provider OAuth remains #339-owned. Test is queued and Release non-terminal. | Finish the #156 inheritance ledger, obtain terminal exact-head Test/Release/security/review evidence after #338, and prove non-vacuous integrated coverage. |
| PR #202 — scan/navigation UI owner | `07c0d89f9232abb5a564f5a42422f1e57805c81c` | stacked on exact #264; canonical `src/routes/+page.svelte` owner | Draft, mechanically mergeable after non-force restack. Fresh #264→#202 is `behind_by=0`, merge base exactly #264, with exactly five page-owned files. Test `33890237093` and Release `33890237222` are queued. | Obtain terminal exact-head gates and preserve #202 as the only parent-page state owner before integrating descendant #203. |
| PR #203 — TopFiles accessibility | `5786231ef7d4a67366f7119eaf5301909e2c4aee` | stacked on exact #202; TopFiles component/accessibility owner | Draft, mechanically mergeable after non-force restack. Fresh #202→#203 is `behind_by=0`, merge base exactly #202, and semantic delta remains exactly `TopFiles.svelte` plus its focused test. Immediately after the new head, no exact-head workflow run had yet been emitted; absence of a run is non-passing. | Issue #340 owns real browser proof for Tab/focus/scroll, fragment-shortcut continuity, viewport overflow, and state transitions. Reacquire exact-head workflows before any readiness claim. |
| PR #334 — ontology-backed parallel reclaim planning | `39cef1fcafb9cc03382ce2de9f78af8c003f0d81` | independent buyer-visible reclaim stack on its declared parent | Draft, mechanically mergeable; broad runtime/ontology work remains active-PR evidence, not protected product truth. | Continue current-head destructive/recovery/platform findings test-first; do not transfer stale review or predecessor checks. |

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
| P0 | The protected release line is blocked by a dependency stack and non-terminal hosted gates. | #264 → #263 → #315, #264 → #338 → #337, and #264 → #202 → #203 remain Draft stacks. This snapshot repaired stale #263/#315/#202/#203 ancestry with non-force descendant restacks; queued/running or absent exact-head workflows are still non-passing. | Dependency roots land first through normal protected merge with exact-head terminal gates; descendants remain non-force restacked and are revalidated on unchanged heads. |
| P1 | Cross-platform capability and failure semantics are broader than the currently protected product evidence. | Windows/Linux/macOS adapters and ontology-backed reclaim work remain active PR evidence, not shipped truth. | Release notes and UI expose a verified platform capability matrix with real current-head integration evidence and bounded unsupported states. |
| P1 | TopFiles had a buyer-visible empty-state and keyboard-evidence gap. | #202 makes zero-result guidance reachable after a successful scan. #203 validates rendered heading/table/header/region/status structure with Svelte SSR and makes the scroll region part of sequential focus with visible `:focus-visible`. Issue #340 owns the remaining real-browser evidence. | Current-head browser E2E exercises normal/empty/loading/error states, Tab focus and keyboard scrolling on `#top-files-table`, fragment-shortcut continuity, visible focus, and long-path responsive overflow. The broader interaction review is resolved only after that evidence exists. |
| P1 | Product documentation previously mixed historical cache-purge authority with the current fail-closed contract. | #315 reconciles PRD, ADR-0002/ADR-0012, runbook, README, architecture, and this baseline while preserving #263 runtime ownership. | No current document reauthorizes permanent purge; status, preview schema, automatic reversible policy, current PR routing, and protected-main maturity agree. |
| P2 | Repository-wide 100% docstring/test/edge-case coverage is a target but is not yet proven on one integrated protected head. | #337 converges coverage scope and fail-closed diagnostic evidence without lowering thresholds. | Exact integrated head produces non-vacuous 100% owned-production statement/branch/function/line and public-doc evidence under repository policy. |

## Architecture and owner contracts

DiskSage owns its filesystem/reclaim facts. `.github`, `enterprise-architecture-core`, `context-graph-contracts`, `ConceptWeave`, `semantic-data-portal`, `contextual-orchestrator`, `noema`, `keyverse`, `EgressWeave`, `OriginWeave`, `pingora-gateway`, `quarantine-sandbox-runtime`, `appguardrail`, `wardnet`, `pg-llm-batch`, `EmbedRelay`, `fast-mlsirm`, `TEPP`, `RankWeave`, `ThreadWeave`, `inkspan`, `DiagramWeave`, and `mhtml-etl-gateway` remain canonical owners of their respective shared capabilities. DiskSage consumes only released/versioned contracts through explicit adapters/ACLs; source copies, cross-service application-table SQL, and mutable/unreleased production dependencies are defects.

For LLM work, `contextual-orchestrator` is the production routing/provider boundary. DiskSage does not own provider credential discovery or direct provider fallback. Deterministic safety, merge, security, coverage, and release gates remain independent of model judgment.

## Quality, security, accessibility, and performance gaps

- Destructive/recovery acceptance uses real filesystem/path/symlink/hardlink/mount/permission/race scenarios; synthetic data is unit-test support only.
- Material UI acceptance requires rendered normal/loading/empty/error/permission states plus real browser keyboard/focus/scroll evidence where interaction semantics matter. SSR structure is useful contract evidence but is not a substitute for browser interaction E2E.
- Rust remains the preferred hot-path, numerical, performance, and security runtime. Python requires an explicit bounded validation role where a practical Rust alternative is absent.
- Web/API buyer paths use async behavior and realistic E2E/load evidence when present. A p95 ≤20 ms claim is made only for a defined, measured path and environment; sampling, exclusion, or artificial cache warming must not manufacture a pass.
- Necessary personal data is protected by purpose-bound authorization, least privilege, minimization, encryption/retention where applicable, and access/export evidence rather than blanket masking that breaks the workflow.
- CSAP/SOC 2 language is evidence-readiness only unless an actual certification/attestation exists.

## Documentation and release acceptance

AGENTS.md, CLAUDE.md, PRD/TRD, ARCHITECTURE, Context Map/Ubiquitous Language, ADR index/details, UML/ERD or truthful logical data model, UX, SECURITY/THREAT_MODEL, TEST_STRATEGY, OPERABILITY/recovery, TRACEABILITY/doctoring, README, CHANGELOG, and this baseline must stay code-current. A GitHub Pages claim requires actual publish evidence.

A release is permitted only from one exact integrated protected head after applicable Test, security/SAST, exact coverage/docstrings, package/build, SBOM, provenance, reproducibility, review, rollback/recovery, accessibility, operability, and buyer acceptance are terminal-success. Then version metadata, CHANGELOG, tag, immutable package/release assets, provenance/SBOM, and rollback evidence must be created and verified together.

## Historical evidence

The detailed 2026-08-21/22 provider incident, local disk-pressure measurements, older PR topology, and earlier architecture notes were useful evidence but are not current routing authority. They are preserved byte-for-byte in [the archived 2026-08-22 baseline](archive/product-technical-gap-baseline-2026-08-22.md) so no prior incident evidence is lost while this canonical file remains current.
