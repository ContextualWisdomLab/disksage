# DiskSage product and technical gap baseline

The canonical product outcomes, supported capabilities, non-goals, and safety invariants are defined in the [DiskSage PRD](PRD.md). This file is the current implementation/ownership snapshot; runtime receipts, exact filesystem evidence, released contracts, protected refs, and live GitHub governance remain authoritative. The prior 2026-08-22 detailed incident baseline is preserved without loss at [docs/archive/product-technical-gap-baseline-2026-08-22.md](archive/product-technical-gap-baseline-2026-08-22.md).

**Snapshot:** 2026-09-04 (Asia/Seoul)

**Protected main:** `6125310d9ea562c0ed36db7ab940a96ac9b32e53`

**Product boundary:** Windows/Linux/macOS local-first disk-space inventory and evidence-bound reclaim; reversible deletion safety, filesystem classification/ontology, recovery, and platform adapters remain DiskSage domain truth. Optional cloud, identity, ontology, LLM, security, and architecture integrations consume released owner contracts through ports/ACLs and do not become mutation authority.

**Evidence rule:** queued/pending/skipped-required/failed/cancelled/stale/predecessor/model-only/status-only evidence is non-passing. Exact current head and independently resolved live base are required for readiness claims.

## Current dependency and ownership map

| Lane | Exact head | Base / owner relation | Current status | Required next proof |
| --- | --- | --- | --- | --- |
| PR #264 — release artifact verifier | `76e2d59f5c3cc7750bca574b40855c59bd3a240e` | `main`; canonical release-verifier owner | Draft, mechanically mergeable. Release-owner contracts inherited from #156 are present; other exact-head gates remain non-terminal. | Preserve the unchanged source head while hosted jobs are queued/running; reacquire terminal Test/Security/SAST/OSV/Scorecard and current review/ruleset evidence before Ready/merge. |
| PR #263 — cache-Trash fail-closed security | `96c38af83b5fba6f23e2763ae7bf6a71bfe7f5d6` | stacked on #264; canonical issue #170 runtime owner | Draft, mechanically mergeable, `behind_by=0` against #264. Permanent cache-Trash deletion remains unavailable. | Integrate/equivalently establish #264, obtain terminal exact-head gates, then merge normally. |
| PR #315 — canonical product/public docs | resolve live; this snapshot is authored on the #315 docs branch | stacked on exact #263; docs/public-surface owner only | Draft. This baseline refresh removes stale routing SHAs and records the active UI accessibility stack without moving runtime authority into docs. | Keep #263/#264 ownership intact, reacquire exact-head checks/reviews after this document change, and remain Draft until protected runtime truth is established. |
| PR #338 — contract-doc CI path filter | `5b005dd052ba92e15e9577429a5d0c0ef190074f` | stacked on #264 | Draft, mechanically mergeable; semantic delta is the Test workflow path filter plus its contract test. | Prove supported ordered `paths` semantics on the unchanged exact head and satisfy current gates/review before parent-first integration. |
| PR #337 — exact-head coverage foundation | `af39bce9bb6ac3186e3940e2c94dd8381080f619` | stacked on exact #338 | Draft, mechanically mergeable. Exact 100% thresholds remain unchanged; coverage diagnostics and inherited regression evidence are still active-PR truth. | Finish the #156 inheritance ledger, obtain terminal exact-head Test/Release/security/review evidence after #338, and prove non-vacuous integrated coverage. |
| PR #202 — scan/navigation UI owner | `502c4288fa4e4e69e2917b27981dc65f33ce08f9` | stacked on #264; canonical `src/routes/+page.svelte` owner | Draft, mechanically mergeable, `behind_by=0` against #264. RED `190a9ee...` → GREEN `502c4288...` makes a successful zero-top-files scan render the TopFiles empty state while initial/loading/error states remain hidden. | Obtain terminal exact-head gates and preserve #202 as the only parent-page state owner before integrating descendant #203. |
| PR #203 — TopFiles accessibility | `8543caee32d4d62a3fab165e1fb8db2d2936e6ee` | stacked on exact #202; TopFiles component/accessibility owner | Draft, mechanically mergeable, `behind_by=0`; semantic delta is exactly `TopFiles.svelte` plus its focused test. SSR validates rendered structure. RED `d7c06d8...` → GREEN `8543caee...` makes the named overflow region sequentially focusable with `tabindex="0"`. | Issue #340 owns real browser proof for Tab/focus/scroll, fragment-shortcut continuity, viewport overflow, and state transitions. Resolve the broader browser-interaction review only after current-lineage E2E exists. |
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
| P0 | The protected release line is blocked by a dependency stack and non-terminal hosted gates. | #264 → #263 → #315, #264 → #338 → #337, and #264 → #202 → #203 are Draft stacks; queued/running hosted jobs are non-passing. | Dependency roots land first through normal protected merge with exact-head terminal gates; descendants are non-force restacked and revalidated. |
| P1 | Cross-platform capability and failure semantics are broader than the currently protected product evidence. | Windows/Linux/macOS adapters and ontology-backed reclaim work remain active PR evidence, not shipped truth. | Release notes and UI expose a verified platform capability matrix with real current-head integration evidence and bounded unsupported states. |
| P1 | TopFiles had a buyer-visible empty-state and keyboard-evidence gap. | #202 now makes zero-result guidance reachable after a successful scan. #203 validates rendered heading/table/header/region/status structure with Svelte SSR and makes the scroll region part of sequential focus with visible `:focus-visible`. Issue #340 now owns the remaining real-browser evidence. | Current-head browser E2E exercises normal/empty/loading/error states, Tab focus and keyboard scrolling on `#top-files-table`, fragment-shortcut continuity, visible focus, and long-path responsive overflow. The broader interaction review is resolved only after that evidence exists. |
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
