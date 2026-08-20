# DiskSage product and technical gap baseline

**Snapshot:** 2026-08-20 (Asia/Seoul)  
**Repository head:** `feat/provider-sync-dynamic-goals` @ `18a9b41`
**Product boundary:** local-first macOS disk pressure relief with iCloud, OneDrive, and Google Drive destinations.  
**Evidence rule:** this document is a dated baseline, not an authority for transfer or deletion. Runtime receipts, provider attestations, object identity, and current GitHub checks remain authoritative.

## Current product contract

1. Scan and metadata profiling are read-only and metadata-first: embedded metadata precedes an unambiguous filename token, then filesystem creation/modification time. A filename token such as `2026-04-28` or `251210` is secondary evidence and never proves ownership, upload, or eviction authority.
2. A cloud candidate follows `copy-verified → pending-provider-sync → provider-sync-confirmed → eviction-ready → source-evicted`. `local-current` with `is_uploaded=false` is `pending-upload`; no eviction permit is issued.
3. Native File Provider copy is bounded, re-hashed, and source-identity rechecked. Provider-global timeout, quota/auth uncertainty, local headroom shortage, stale worktree metadata, or incomplete metadata fail closed.
4. Regenerable caches are a separate reclaim domain. They are per-child, identity-bound, active-use checked, journaled, and moved to OS Trash; they are not uploaded as user data.
5. Deterministic Rust gates own safety. A local model may judge only the fixed maintenance command after dry-run evidence, calibration, and explicit human confirmation. No external LLM or OAuth service is a runtime prerequisite for the standalone product.

## Buyer-observable product gaps

| Priority | Gap / observable symptom | Evidence | Acceptance criterion |
| --- | --- | --- | --- |
| P0 | Cloud offload can remain blocked while a provider is syncing or reports `local-current`/`is_uploaded=false`; the user sees no safe reclaim despite free cloud capacity. | Existing provider-global and iCloud native-state gates; `bird` remains near 100% CPU during the current incident, while cleanup restored about 6.8 GiB available. | UI explains the exact blocker, last evidence time, and next bounded retry; a verified provider attestation alone can advance a candidate, never a stale projection. |
| P0 | A long Finder/provider copy can appear hung and consume the remaining local headroom. | The read-only File Provider dump repeatedly reports `no progress` and `hard expired`; bounded `/bin/cp`/`mkdir` groups and headroom gates exist. | Preview shows required bytes + staging reserve; timeout cleans only the child-created destination and leaves a durable receipt. |
| P1 | Personal desktop-client capacity is not the same as API quota; OAuth is unnecessarily implied for a single-user installation. | ADR-0001 permits copy-only desktop-client mode marked `capacity-unverified`. | Settings clearly distinguish local desktop client, API quota, and organization OAuth; no OAuth prompt is required for the local-only path. |
| P1 | Users cannot yet see a compact lineage graph connecting source, metadata, archive member, provider item, receipt, Goal, and eviction decision. | Naruon/semantic export contracts exist, but there is no buyer-facing graph view. | Export and UI show stable content IDs, provenance edges, confidence, and blockers without exposing raw private paths. |
| P1 | “Orphan”/duplicate cleanup is difficult to trust because relationship evidence is not visible before action. | Ontology and duplicate/orphan PRs are open; current default path remains fail-closed. | Every proposed removal has an explainable parent/child/duplicate relation, identity recheck, reversible Trash action, and a no-candidate result when evidence is incomplete. |
| P2 | Cross-platform behavior and accessibility are not presented as one release contract. | macOS/Linux/Windows release checks exist; several UI accessibility PRs remain open. | Release notes and UI expose platform capability matrix, keyboard/assistive labels, and bounded failure messages for each action. |

## Technical and operational gaps

| Priority | Gap | Current state | Smallest next proof |
| --- | --- | --- | --- |
| P0 | Provider end-to-end receipt is absent for the current iCloud incident. | Global probe can time out and CloudDocs state is intentionally not force-killed or deleted. | Capture a bounded fresh provider evidence receipt after sync settles; keep transfer/eviction disabled until it is complete. |
| P0 | Disk pressure telemetry is not durable enough for incident comparison. | `df`, process, and bounded probe results are operator evidence only. | Store redacted capacity/process summaries with timestamp and evidence hash, never raw provider dumps. |
| P1 | Hourly product-development/review loop is not yet live in this repository environment. | `.github/workflows/hourly-product-loop.yml` is now scheduled and secret-gated; the first live agent receipt and configured secret names are not available here. | Configure only `CONTEXTUAL_ORCHESTRATOR_URL` and `CONTEXTUAL_ORCHESTRATOR_TOKEN`, run once manually, and retain a bounded completion receipt without enabling mutation. |
| P1 | Open PR queue prevents a clean protected release line. | 50 PRs are open; #240 merged normally. #187 is ready with successful checks but still lacks a fresh approving review; #213 checks are pending and #209 is unstable. | Process one PR at a time: current-head review → fix → required checks → normal protected merge; never bypass or self-approve. |
| P1 | Current UI coverage is contract-heavy rather than runtime E2E for native File Provider states. | The UI now displays `로컬 최신본·업로드 미확인` and maps blockers without backend detail; provider operations are not safely reproducible on this full disk. | Add a deterministic Rust fixture-backed state machine test for `local-current + is_uploaded=false`, provider timeout, and receipt invalidation. |
| P1 | Ontology/catalog integrations are export boundaries, not deployed services. | Naruon/semantic catalog and Zotero local API docs/contracts exist; no Noema/contextual-orchestrator runtime dependency is required. | Keep integrations optional and path-free; add live service tests only when a concrete consumer and secret boundary exist. |
| P2 | 100% documentation/docstring and edge-case coverage is not yet evidenced. | Existing checks cover core Rust/TS behavior, not a repository-wide percentage claim. | Publish measured coverage per language and close high-risk edge paths before claiming 100%. |
| P2 | Figma design source is not part of the current change. | No visual redesign or Figma artifact was introduced in this baseline. | If a product UI redesign is approved, record the Figma File ID in a new ADR before implementation. |

## Architecture and decision linkage

- ADR-0001 defines provider evidence, metadata precedence, native copy, headroom, and eviction gates.
- ADR-0002 defines per-item cache cleanup and the narrow no-second-approval incident policy.
- ADR-0003 defines the local Zotero metadata handoff and keeps cloud receipts independent.
- ADR-0004 defines bounded fixed Homebrew maintenance execution and process-group cleanup.
- Dynamic Goal/ADR projections are replaceable views over receipts; they cannot authorize mutation.
- Rust remains the computation and security boundary. Noema, contextual-orchestrator, semantic-data-portal, pg-erd-cloud, fast-mlsirm, or Gemma are added only when a measured gap requires them and their boundary is documented first.

## Loop update rule

At each scheduled or operator loop, update this file only with new dated evidence: current head, open-PR/check state, provider receipt state, disk headroom, and the smallest acceptance proof completed. Do not convert an incomplete provider probe, filename date, model answer, or GitHub review comment into a transfer or deletion authority.
