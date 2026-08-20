# DiskSage product and technical gap baseline

**Snapshot:** 2026-08-21 (Asia/Seoul)
**Repository heads at snapshot:** `feat/provider-sync-dynamic-goals` implementation @ `2a33ed5`; documentation @ `097e7db` (latest committed baseline before this evidence update)
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
| P0 | Cloud offload can remain blocked while a provider is syncing or reports `local-current`/`is_uploaded=false`; the user sees no safe reclaim despite free cloud capacity. | Existing provider-global and iCloud native-state gates; `bird`/`fileproviderd` remain active during the current incident, while regenerable worktree artifacts were reclaimed and about 7.4 GiB remained available at the latest check. | UI explains the exact blocker, last evidence time, and next bounded retry; a verified provider attestation alone can advance a candidate, never a stale projection. |
| P0 | A long Finder/provider copy can appear hung and consume the remaining local headroom. | The `real_datasets` Finder copy remained at “준비 중” for hours; a fresh read-only CloudDocs `fileproviderctl dump` timed out after 15 seconds while `bird`/`fileproviderd` remained active. Bounded `/bin/cp`/`mkdir` and global probes use private process groups and headroom gates. | Preview shows required bytes + staging reserve; timeout cleans only the child-created destination and leaves a durable receipt. |
| P1 | Personal desktop-client capacity is not the same as API quota; OAuth is unnecessarily implied for a single-user installation. | ADR-0001 permits copy-only desktop-client mode marked `capacity-unverified`. | Settings clearly distinguish local desktop client, API quota, and organization OAuth; no OAuth prompt is required for the local-only path. |
| P1 | Users cannot yet see a full lineage graph connecting source, metadata, archive member, provider item, receipt, Goal, and eviction decision. | The candidate UI now exposes a compact source→metadata→archive→provider lineage panel using the stable fingerprint, confidence, and blocker state; provider item/receipt/permit remain explicitly pending until their evidence exists. | Export and UI show stable content IDs, provenance edges, confidence, and blockers without exposing raw private paths. |
| P1 | “Orphan”/duplicate cleanup is difficult to trust because relationship evidence is not visible before action. | Ontology and duplicate/orphan PRs are open; current default path remains fail-closed. | Every proposed removal has an explainable parent/child/duplicate relation, identity recheck, reversible Trash action, and a no-candidate result when evidence is incomplete. |
| P2 | Cross-platform behavior and accessibility are not presented as one release contract. | macOS/Linux/Windows release checks exist; several UI accessibility PRs remain open. | Release notes and UI expose platform capability matrix, keyboard/assistive labels, and bounded failure messages for each action. |

## Technical and operational gaps

| Priority | Gap | Current state | Smallest next proof |
| --- | --- | --- | --- |
| P0 | Provider end-to-end receipt is absent for the current iCloud incident. | Global probe can time out and CloudDocs state is intentionally not force-killed or deleted; the native copy boundary now requires an integrity-checked three-stream pre-copy cohort before mutation. | Capture a bounded fresh provider evidence receipt after sync settles; keep transfer/eviction disabled until it is complete. |
| P0 | Disk pressure telemetry and provider queue evidence must remain comparable across loops without retaining raw provider output. | Cloud plans and explicit iCloud health refreshes persist bounded, path-free `LocalVolumeSnapshot`, `ProviderClientRuntimeSnapshot`, and `IcloudSyncHealthEvidenceSnapshot` records under `volume-pressure-evidence`, `provider-client-runtime-evidence`, and `icloud-sync-health-evidence`; iCloud plans now combine them into a timestamp/fingerprint-bound cohort. | Missing, incomplete, malformed, or more-than-five-minute-skewed cohort observations remain blocked; a fresh exact-head native incident plan is still needed to compare the emitted cohort with the live incident. |
| P1 | Hourly product-development/review loop is not yet live in this repository environment. | `.github/workflows/hourly-product-loop.yml` is scheduled and uses only contextual-orchestrator's published `/v1/models` and `/v1/chat/completions` APIs against the exact event SHA; no endpoint or deployment receipt is available here. | Configure the orchestrator URL/token in its deployment, run once manually, and retain a bounded advisory completion receipt without importing provider secrets or enabling mutation. |
| P1 | Open PR queue prevents a clean protected release line. | PR #213 is at exact implementation head `a6f309e` (latest docs head) on base `5f7c7ae`; its required checks remain queued/in progress and the review decision is still a stale `CHANGES_REQUESTED`. PR #189 is rebased to `f748769f` and PR #209 is stacked on it at `0fc67a4`; both have fresh checks queued. PR #235 was protected-squash merged as `5f7c7ae`. | Process one PR at a time: current-head review → fix → required checks → fresh approval → normal protected merge; never bypass or self-approve. |
| P1 | Current UI coverage is contract-heavy rather than runtime E2E for native File Provider states. | The UI now displays `로컬 최신본·업로드 미확인` and maps blockers without backend detail; provider operations are not safely reproducible on this full disk. Rust fixtures now cover `local-current + is_uploaded=false`, provider timeout, timeliness transitions, and receipt/evidence invalidation; native runtime E2E remains unavailable while the provider is unhealthy. | Keep the fixture-backed state machine green and add a bounded native E2E receipt only after a quiet provider observation is authoritative. |
| P1 | Ontology/catalog integrations are export boundaries, not deployed services. | Naruon/semantic catalog and Zotero local API docs/contracts exist; no Noema/contextual-orchestrator runtime dependency is required. | Keep integrations optional and path-free; add live service tests only when a concrete consumer and secret boundary exist. |
| P2 | 100% documentation/docstring and edge-case coverage is not yet evidenced. | Existing checks cover core Rust/TS behavior, not a repository-wide percentage claim. | Publish measured coverage per language and close high-risk edge paths before claiming 100%. |
| P2 | Figma design source is not part of the current change. | No visual redesign or Figma artifact was introduced in this baseline. | If a product UI redesign is approved, record the Figma File ID in a new ADR before implementation. |

## Architecture and decision linkage

- ADR-0001 defines provider evidence, metadata precedence, native copy, headroom, and eviction gates.
- ADR-0002 defines per-item cache cleanup and the narrow no-second-approval incident policy.
- ADR-0003 defines the local Zotero metadata handoff and keeps cloud receipts independent.
- ADR-0004 defines bounded fixed Homebrew maintenance execution and process-group cleanup.
- ADR-0006 defines bounded, redacted iCloud health evidence persistence and timestamped comparison.
- ADR-0007 defines the integrity-checked three-stream cohort at the native-copy mutation boundary.
- ADR-0008 keeps the hourly contextual-orchestrator integration read-only at the foreign-repository and provider-secret boundaries.
- Dynamic Goal/ADR projections are replaceable views over receipts; they cannot authorize mutation.
- Rust remains the computation and security boundary. Noema, contextual-orchestrator, semantic-data-portal, pg-erd-cloud, fast-mlsirm, or Gemma are added only when a measured gap requires them and their boundary is documented first.

## 2026-08-21 loop evidence

- The implementation head observed before this documentation update was `88001d8`: existing-copy
  adoption no longer requires native-copy staging headroom, so a low-disk user can verify and adopt
  an already-present cloud copy without creating local staging data.
- Naruon cloud-copy readiness is now schema version 7 and carries the path-free pre-copy evidence
  cohort plus `pre_copy_evidence_met`; missing or incomplete iCloud evidence remains a blocker in
  the exported contract. Focused Rust readiness tests passed: 14 passed, 0 failed.
- Only stale generated CodeGraph databases were removed from unrelated temporary worktrees during
  this loop; source files and user data were not deleted. The local volume measured about 8.3 GiB
  free after that generated-artifact cleanup; later bounded local build activity measured 6.1 GiB
  free and was not treated as user-data cleanup authority.
- A fresh read-only macOS observation reported iCloud `needs-sync-up`; iCloud quota still had about
  4.3 TB remaining, so DiskSage keeps native copy and eviction blocked on provider state rather than
  quota. A bounded Google Drive File Provider dump reported active upload/download markers and a
  14,558-entry reconciliation backlog; the latest dump additionally reports Google Drive
  `temporarily disconnected`, `needs-indexing`, and File Provider error `-1004`. The existing
  `provider-global-sync-*` blockers cover this Finder "copy preparing" failure mode without
  terminating `bird` or `fileproviderd`; provider diagnostics and bounded client recovery remain
  available even while the destination root is unreadable.
- The follow-up observation of the user-visible `real_datasets` Finder copy still showed “준비 중”
  after hours while the same Google Drive domain remained temporarily disconnected; the local APFS
  volume measured only 150 MiB available (99% full). DiskSage therefore instructs the operator to
  cancel the Finder operation, records any new copy as failed until a fresh plan exists, preserves
  the source, and keeps provider copy, attestation, and eviction disabled until a bounded probe
  reports usable headroom and a readable destination.
- A later bounded probe observed the Google Drive domain readable again but still with active upload
  and download progress plus a 168-entry reconciliation backlog (`needs-indexing: no`). This is
  still `provider-global-sync-transfer-active`/`provider-global-sync-reconciliation-pending`, so
  the Finder copy remains unsafe to retry until a fresh quiet probe is authoritative.
- A subsequent bounded probe also reported repeated File Provider `-1005 itemNotFound` entries
  (last activity roughly 2h40m old) in the same Google Drive domain. The provider parser now emits
  `provider-global-sync-item-not-found` and the UI labels it as a missing-provider-item error;
  queue-count changes do not reset the same-blocker duration.
- The current implementation head for this loop is `2a33ed5`; generated CodeGraph indexes are now
  included in the bounded, identity-checked development-artifact cleanup, and provider evidence
  directories now reject shared-writable authority while records are private from creation; the
  authority regression fixture also binds the current provider `sync_state` contract.
  Frontend tests remain 25 files / 117
  tests and `svelte-check` remains 0 errors / 0 warnings. Hosted Rust, security, and review gates
  remain authoritative before any protected merge.
- The local Zotero endpoint is readable (`GET 200`, Zotero `9.0.6`, 8,312-item library), but the
  documented Zotero 9 write route remains `zotero-local-api-write-unsupported` and no local API
  key is present in this environment. DiskSage therefore does not duplicate or mutate references;
  the manifest remains a bounded, explicit handoff until Zotero 10+ and an operator-provided key
  are available.
- PR #209 current head `0fc67a4` now bounds Homebrew and iCloud eviction error feedback and its
  privacy contract passes locally (Vitest 2/2; svelte-check 0 errors/0 warnings). Its hosted checks
  are running and a fresh approval is still required. PR #189 is its rebased base PR at `f748769f`;
  both branches remain protected from bypass merges. PR #235 completed its
  protected squash merge at `5f7c7ae`. PR #213 remains open at current head `2a33ed5`; hosted
  release, test, security, and review checks are authoritative, and no source eviction was claimed.
- Independent CLI/UI/dependency PRs #212, #214, #215, #217, #218, #220, #222, #230, #232,
  #234, and #238 were rebased to current `main` `5f7c7ae`; their fresh checks are queued or in
  progress, and no new failure is treated as resolved until the exact rebased head is green.
- CloudArchive now routes all 15 user-visible asynchronous failure phases through bounded,
  operation-specific messages. The exact-head frontend suite passed 25 files / 117 tests and
  `svelte-check` passed with 0 errors and 0 warnings; raw backend exception details remain
  diagnostic-only and are not rendered in the UI.
- A regression fixture now proves a bare File Provider `ENOSPC` marker is classified as an error
  with both a local-disk-full blocker and the aggregate provider-error blocker, preventing a
  contradictory healthy `clear` envelope.
- The provider-global-sync panel now shows the last local evidence observation and its bounded
  one-minute automatic recheck, so a Finder “copy preparing” incident has an actionable next step
  instead of an indefinite spinner.
- The provider panel now fingerprints the current blocker cohort and reports how long the same
  blocker has persisted; after 15 minutes it explicitly directs the operator to cancel Finder's
  pending copy and wait for a quiet provider observation before retrying.
- Provider probe failures now show the same fail-closed Finder-cancel guidance as a blocked
  aggregate report, preventing an unavailable diagnostic from looking like a safe retry state.
- Background reconciliation now retains at most 128 immutable provider evidence records per
  receipt, so the one-minute UI loop cannot grow the evidence directory without bound. Active
  iCloud File Provider upload/download progress is also accepted as a blocked Naruon readiness
  envelope.
- A fresh bounded macOS observation on this loop measured about 1.2 GiB available (91% used) and
  Google Drive still reporting upload/download progress, a 168-entry reconciliation backlog,
  `error generation: 403`, and repeated File Provider `-1005 itemNotFound` entries. The
  `brew cleanup --prune-prefix --dry-run --verbose` probe returned no reclaimable Homebrew items;
  no provider process, Finder operation, or user data was terminated or deleted.
- A fresh iCloud File Provider observation reported an active upload at 95.24% (28,124,151,529 of
  29,530,341,516 bytes) and an active download at 0% (0 of 1,066,167,994 bytes), with scheduling
  still running and error generation 1143. The bounded probe exceeded its wall-clock limit, so the
  evidence remains incomplete and new-copy admission stays blocked. CloudArchive now fingerprints
  iCloud blocker/progress state, displays the same-blocker duration, and directs Finder-copy
  cancellation after 15 minutes; the focused contract test and `svelte-check` both pass.
- To restore the emergency local headroom without touching user or provider data, only Podman
  dangling (untagged and unreferenced) images were pruned; the one active container and all volumes
  were retained. Host APFS free space rose from roughly 150 MiB to 1.8 GiB, matching the bounded
  `prune_dangling_images` reclaim boundary.
- Unreadable provider roots remain selectable for diagnosis/recovery while preview, copy,
  attestation, and eviction controls stay disabled until the root is readable again.

## Loop update rule

At each scheduled or operator loop, update this file only with new dated evidence: current head, open-PR/check state, provider receipt state, disk headroom, and the smallest acceptance proof completed. Do not convert an incomplete provider probe, filename date, model answer, or GitHub review comment into a transfer or deletion authority.
