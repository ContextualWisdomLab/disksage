# DiskSage product and technical gap baseline

**Snapshot:** 2026-08-22 (Asia/Seoul)
**Repository heads at snapshot:** PR #213 `a6ec6e2`, PR #247 `a0fa7bc`, PR #246 `741ab30`,
supporting PR #156 `39a08a7`, and PR #192 `30ceea2`; hosted checks and protected review remain
authoritative, and no merge is claimed from queued or stale status.
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
| P0 | Cloud offload can remain blocked while a provider is syncing or reports `local-current`/`is_uploaded=false`; the user sees no safe reclaim despite free cloud capacity. | Existing provider-global and iCloud native-state gates; `bird`/`fileproviderd` remain active during the current incident, with about 3.8 GiB available at the latest observation. | UI explains the exact blocker, last evidence time, and next bounded retry; a verified provider attestation alone can advance a candidate, never a stale projection. |
| P0 | A long Finder/provider copy can appear hung and consume the remaining local headroom. | The `real_datasets` Finder copy remained at “준비 중” for hours; the latest bounded iCloud dump retained 125 no-progress fetch/create markers, a 95.24% upload, and a zero-progress 1.06GB download while scheduling was `running`. Bounded `/bin/cp`/`mkdir` and global probes use private process groups and headroom gates. | Preview shows required bytes + staging reserve; timeout cleans only the child-created destination and leaves a durable receipt. |
| P1 | Personal desktop-client capacity is not the same as API quota; OAuth is unnecessarily implied for a single-user installation. | ADR-0001 permits copy-only desktop-client mode marked `capacity-unverified`; the cloud connection UI defaults to read-only OAuth consent and requires an explicit write-access opt-in. | Settings clearly distinguish local desktop client, API quota, and organization OAuth; no OAuth prompt is required for the local-only path. |
| P1 | Users cannot yet see a full lineage graph connecting source, metadata, archive member, provider item, receipt, Goal, and eviction decision. | The candidate UI now exposes a compact source→metadata→archive→provider lineage panel using the stable fingerprint, confidence, and blocker state; provider item/receipt/permit remain explicitly pending until their evidence exists. | Export and UI show stable content IDs, provenance edges, confidence, and blockers without exposing raw private paths. |
| P1 | “Orphan”/duplicate cleanup is difficult to trust because relationship evidence is not visible before action. | Ontology and duplicate/orphan PRs are open; current default path remains fail-closed. | Every proposed removal has an explainable parent/child/duplicate relation, identity recheck, reversible Trash action, and a no-candidate result when evidence is incomplete. |
| P2 | Cross-platform behavior and accessibility are not presented as one release contract. | macOS/Linux/Windows release checks exist; several UI accessibility PRs remain open. | Release notes and UI expose platform capability matrix, keyboard/assistive labels, and bounded failure messages for each action. |

## Technical and operational gaps

| Priority | Gap | Current state | Smallest next proof |
| --- | --- | --- | --- |
| P0 | Provider end-to-end receipt is absent for the current iCloud incident. | Global probe can time out and CloudDocs state is intentionally not force-killed or deleted; the native copy boundary now requires an integrity-checked three-stream pre-copy cohort before mutation. | Capture a bounded fresh provider evidence receipt after sync settles; keep transfer/eviction disabled until it is complete. |
| P0 | Disk pressure telemetry and provider queue evidence must remain comparable across loops without retaining raw provider output. | Cloud plans and explicit iCloud health refreshes persist bounded, path-free `LocalVolumeSnapshot`, `ProviderClientRuntimeSnapshot`, and `IcloudSyncHealthEvidenceSnapshot` records under `volume-pressure-evidence`, `provider-client-runtime-evidence`, and `icloud-sync-health-evidence`; iCloud plans now combine them into a timestamp/fingerprint-bound cohort. | Missing, incomplete, malformed, or more-than-five-minute-skewed cohort observations remain blocked; a fresh exact-head native incident plan is still needed to compare the emitted cohort with the live incident. |
| P1 | Hourly product-development/review loop is not yet live in this repository environment. | The repository-local `.github/workflows/hourly-product-loop.yml` is intentionally `workflow_dispatch`-only because its direct contextual-orchestrator HTTP call is advisory and not a pinned OpenCode worker. The trusted central [`disksage-hourly-review-repair.yml`](https://github.com/ContextualWisdomLab/.github/blob/main/.github/workflows/disksage-hourly-review-repair.yml) runs at `37 * * * *` and dispatches the pinned scheduler `a3fdaa1aacaba9443a18573f3c309fe1841fc2f0`, which performs the OpenCode OIDC exchange. The local workflow still uploads a seven-day path-free receipt when manually configured; no external endpoint or deployment receipt is available here. | Verify one central scheduler receipt and one local manual advisory receipt; preserve read-only permissions, exact-head binding, and no provider-secret import or mutation. |
| P1 | Open PR queue prevents a clean protected release line. | At this loop capture PR #213 is exact head `6f424af` on `feat/provider-sync-dynamic-goals`; its required checks reset after the provider-dump pipe repair and the prior review decision remains stale `CHANGES_REQUESTED`. The orphan cleanup follow-up is PR #245, initially implemented at `3d2406c` and subsequently extended with provider-sync and cleanup-refresh safety fixes. Both remain protected and unmerged pending exact-head review. | Process one PR at a time: current-head review → fix → required checks → fresh approval → normal protected merge; never bypass or self-approve. |
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

- The exact-head hosted macOS build exposed a compile regression after the sensitive-config safety
  boundary was added: the generated `disksage-cloud-plan` implementation omitted the new
  `ArchiveKind::SensitiveConfig` wire label. The source was fixed in the single generated-source
  owner (`src-tauri/cloud_plan_implementation.rs.inc`) and now has a focused label contract test;
  the same generated test fixtures bind the new `pre_copy_evidence` field, and the targeted
  `cargo test --locked --features cloud-cli --bin disksage-cloud-plan` passes (1 test). The hosted
  matrix must be rerun on the resulting head before any protected merge; this correction grants
  no copy, cloud-write, or eviction authority.
- The local verification created only regenerable Cargo target artifacts; after all Cargo processes
  exited, `cargo clean --manifest-path src-tauri/Cargo.toml` removed 3.8 GiB and restored about
  8.9 GiB APFS availability. No source, Finder, iCloud/File Provider, OneDrive, or Google Drive
  data was touched.

- The implementation head observed before this documentation update was `88001d8`: existing-copy
  adoption no longer requires native-copy staging headroom, so a low-disk user can verify and adopt
  an already-present cloud copy without creating local staging data.
- Naruon cloud-copy readiness is now schema version 8 and carries the path-free pre-copy evidence
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
- A live low-space observation reached 133 MiB free while iCloud probing was still active. The
  current process table and Git worktree registry showed no holder for the temporary
  `/private/tmp/disksage-pr228-current` review worktree, so it was removed with the normal
  `git worktree remove` path; APFS headroom recovered to 4.3 GiB. Other repositories' worktrees
  were not deleted, and provider data/processes were not touched.
- To restore the emergency local headroom without touching user or provider data, only Podman
  dangling (untagged and unreferenced) images were pruned; the one active container and all volumes
  were retained. Host APFS free space rose from roughly 150 MiB to 1.8 GiB, matching the bounded
  `prune_dangling_images` reclaim boundary.
- Unreadable provider roots remain selectable for diagnosis/recovery while preview, copy,
  attestation, and eviction controls stay disabled until the root is readable again.

## Loop update rule

At each scheduled or operator loop, update this file only with new dated evidence: current head, open-PR/check state, provider receipt state, disk headroom, and the smallest acceptance proof completed. Do not convert an incomplete provider probe, filename date, model answer, or GitHub review comment into a transfer or deletion authority.

## 2026-08-21 lineage graph update

- Source head `677042467b3398866757f39b9475bd0b267abc75` now exports path-free ontology relations for
  source, metadata and production evidence, archive, destination, receipt, review decision,
  provider sync state, provider evidence, and remote object when present. The legacy `archivedTo`
  relation remains for compatibility; missing evidence emits `unknown` and no attestation edge.
- The preceding focused Rust export test passed 1/1; the current head adds an assertion that the
  content and metadata nodes remain distinct. Hosted Rust checks are authoritative for this latest
  head. This closes the export-side P1 lineage relation gap; the UI still shows provider-item/
  receipt/permit details only when those runtime records exist.
- Legacy provider evidence with `sync_complete=true` but `sync_state=unknown` now remains
  unconfirmed in the Naruon export; only explicit `complete` state can support provider-sync
  confirmation or any downstream eviction gate.
- Organization lineage probing now remains bounded at the 200-item export limit; plans larger
  than the bound are rejected by the existing batch-size contract rather than emitting default
  metadata that would make an otherwise realistic export fail late.
- The local APFS volume had about 2.6 GiB available after removing only Cargo-generated build
  artifacts. No user file, CloudDocs database, provider process, Finder operation, or cloud object
  was removed. PR #213 remains protected and awaits fresh exact-head review/check results.
- A subsequent low-space loop used the already-catalogued regenerable pnpm store boundary: `pnpm
  store prune` removed 30,315 stale files / 602 packages (about 1.08 GB). No user files, provider
  databases, CloudDocs data, active processes, or cloud objects were touched; the product's
  `pnpm-cache` cleanup domain remains the reproducible-cache implementation boundary.
- A stale-worktree audit found no process holding the two Naruon review worktrees; only their
  ignored, regenerable `frontend/node_modules` directories were removed after dry-run identity
  checks. Tracked `.Jules/palette.md` edits were preserved, and no branch, source file, provider
  database, or cloud object was removed.

## 2026-08-21 follow-up loop evidence

- Current source fix head is `b9fe4f0`; this evidence update is the next documentation revision
  revision. The only local DiskSage worktree is `/private/tmp/disksage-current`; the temporary PR
  #189 worktree was removed after its focused test passed, so no stale DiskSage worktree remains.
- A bounded read-only iCloud File Provider dump captured at `2026-08-21 04:31:47 +0900` contained
  97 `createItemBasedOnTemplate` and 46 `fetchContentsForItemWithID` requests marked `no progress`;
  no upload/download progress marker was retained in that bounded output. This is incomplete
  provider evidence, so Finder-copy cancellation and new-copy admission remain blocked.
- The local APFS volume currently has about 2.3 GiB available (79% used) while `bird` and
  `fileproviderd` remain active. DiskSage has not terminated provider processes, Finder, or user
  data; the observed pressure is not a deletion authority.
- PR #189 advanced to exact head `635d918` with the missing executed/non-executed Homebrew result
  contract assertions; its focused Vitest passed 5/5 and hosted checks/review are rerunning. PR
  #213 remains at `b7a41a4` with hosted checks pending and its old review decision not yet replaced
  by a fresh approval. No protected merge or source eviction is claimed.
- PR #228's latest macOS build failure was an `E0617` variadic-FFI type error in the private
  evidence publication boundary; its current head `1eb947e` casts the mode argument to
  `libc::c_uint`, and the failed build's source cause is now addressed. The local reproduction
  could not complete a full target build because the disk reached `ENOSPC`; generated Cargo target
  state was then removed with `cargo clean`, recovering about 729 MiB. Hosted exact-head checks are
  authoritative for the final proof.
- A newer bounded iCloud observation at `2026-08-21 04:59:17 +0900` retained 125 aggregate
  fetch/create `no progress` markers, upload progress `28,136,385,681/29,543,186,689` (95.24%),
  download progress `0/1,060,097,218`, scheduler `running`, and error generation `1143`; APFS
  free space was about 3.9 GiB. The Finder “real_datasets” copy remains a File Provider stall,
  not a DiskSage transfer completion. The source is retained; only the Finder progress cancel
  control is an operator action, and new copy, attestation, and eviction remain blocked until a
  fresh complete quiet-provider observation.
- Naruon PR #1434 at exact head `c084801` accepts DiskSage readiness schema versions 6 and 7; stacked Naruon PR #1471 adds schema 8 acceptance for the new iCloud File Provider lock/stall blockers
  and validates the current pre-copy/iCloud-native fields in its redacted verifier summary; the
  focused handoff contract passed 48/48. The change keeps the path-free protocol and grants no
  cloud-write or source-eviction authority. Hosted Naruon security, review, and build checks remain
  authoritative before merge.
- The remote implementation branch advanced concurrently to exact head `a8e0283`, adding a
  regression test that rejects shared-writable provider-evidence lookup authority; it was
  fast-forwarded locally without force-push or conflict resolution. PR #213's Ubuntu/macOS/Windows
  builds are in progress while analysis, SAST, dependency, Noema, Strix, and review checks remain
  queued; PR #213 is still not merge-authorized. The latest local APFS observation is about 3.8 GiB
  free, and no provider process, Finder operation, or user data was terminated or deleted.
- The next concurrent source fix `b9fe4f0` hardens `latest_api_object_id` against shared-writable
  provider-evidence directories; the documentation change was rebased onto it as exact PR #213
  head `3148d71`. This preserves the remote agent's change without force-push and keeps provider
  evidence fail-closed when directory authority drifts.

## 2026-08-21 11:19 +0900 incident follow-up

- A read-only `fileproviderctl dump` probe was bounded at 15 seconds for diagnosis and returned
  partial output before timing out. The system log independently recorded repeated File Provider
  `no progress` fetch/create requests, materialization failures, and file-coordination failures;
  `bird` and `fileproviderd` were active. This explains the Finder `real_datasets` “copy preparing”
  dialog as a provider stall, not a successful cloud copy.
- DiskSage issued the fixed Finder Escape cancellation request and received exit status 0. It did
  not kill Finder, `bird`, `fileproviderd`, or any provider client, and it did not delete or rename
  user files, provider data, cloud objects, or CloudDocs databases. A new-copy admission, attestation,
  and source-eviction decision remains blocked until a fresh complete quiet provider observation.
- Source head `9b1c270` now retains a bounded hourly contextual-orchestrator receipt: only schema,
  exact event SHA, model id, status, response byte count, and response SHA-256 are uploaded for
  seven days; the advisory response body is never persisted. `actionlint` passed, the focused loop
  contract passed 2/2, the full frontend suite passed 26 files/118 tests, and `svelte-check`
  reported 0 errors/0 warnings. PR #213 exact head at this capture was `6ef85e4`; its hosted
  build/security/review checks remain authoritative and no protected merge is claimed.
- Follow-up source head `037a9b3` removes the model identifier from the GitHub Step Summary, leaving
  only fixed status/hash/byte fields visible in logs; the model id remains inside the short-lived,
  bounded JSON artifact. The focused contract and `actionlint` checks still pass.
- PR #212 exact head `7f1ac61` had one Strix attempt fail before repository analysis because the
  runner Caido bootstrap could not connect to `127.0.0.1:48080` after ten attempts; the job log
  contains no source finding. A rerun is queued as job `96570913837`, so the PR remains unmerged and
  its security gate is not treated as passed until that exact job produces a terminal result.

- The bounded read-only probe at `2026-08-21 05:42 +0900` retained at least 71
  `fetchContentsForItemWithID` and 161 `createItemBasedOnTemplate` requests marked `no progress`
  within five seconds; output was capped and no raw dump was written. `fileproviderctl help`
  exposes no supported cancellation command, so DiskSage keeps the Finder cancel control as the
  only operator action, never terminates `fileproviderd`/`bird`, and keeps copy, attestation, and
  eviction blocked. APFS free space fluctuated from roughly 406 MiB to 2.5 GiB; only a clean
  temporary Naruon worktree and regenerable package-manager caches were removed, while user files,
  CloudDocs databases, and provider-managed data were retained.
- At `2026-08-21 06:02 +0900`, the current source head `7be4eb3` passed the frontend suite
  (26 files / 118 tests), `svelte-check` (0 errors / 0 warnings), and the production Vite build.
  The loopback Zotero endpoint still reports 9.0.6 and 8,312 items; a bounded invalid POST returns
  `400 Endpoint does not support method`, so the added DCMI and provenance references remain a
  dry-run manifest handoff and no Zotero item or attachment was mutated. PR #213 hosted checks
  remain queued/in progress and the protected merge gate is unchanged.
- At `2026-08-21 06:10 +0900`, the frontend coverage gate passed at 100% statements, branches,
  functions, and lines (26 files / 118 tests) on source head `7765a4b`; the repository-wide Rust
  coverage percentage remains unclaimed because the local APFS headroom is below a safe full Cargo
  target build threshold. Hosted Rust coverage and required checks remain authoritative.
- At `2026-08-21 06:21 +0900`, the only active local Cargo cache pressure was the regenerable
  `~/.cargo/registry/src` tree (about 1.3 GiB). With no Cargo/rustc process running, that source
  cache was removed; the Cargo index, package archives, git checkouts, user files, CloudDocs DBs,
  and provider-managed data were retained. The path is now an explicit manual-review catalog item,
  not an automatic cleanup target. APFS free space recovered to about 1.6 GiB at observation.
- At `2026-08-21 06:25:37 +0900`, APFS availability reached 289 MiB while the Finder
  `real_datasets` copy remained in “준비 중”. A bounded read-only `fileproviderctl dump -l 20`
  contained upload/download progress markers and old File Provider `itemNotFound` errors; its
  temporary 317 KiB output was removed without retaining paths or item identifiers. Only
  regenerable package/tool caches were removed (no provider process, Finder operation, CloudDocs
  database, cloud object, user file, or active Cargo/uv runtime was touched), recovering about
  1.6 GiB. APFS then fluctuated between 1.7 and 1.9 GiB, so the operation remains blocked and the
  Finder cancel control is still the only supported cancellation action. Caches were not uploaded
  to a provider because they are reproducible cleanup data, not user-file lineage. Evidence is
  bound to source head `e71ecd13e8c91acf10093271fd58414cae5fe349`.
- At `2026-08-21 06:43 +0900`, DiskSage PR #213 advanced to exact head
  `41d27dfa8bd66b5986d00ce84d20c7f7b2cdb3b0` with the observed-cache catalog and incident ADR
  evidence. Its exact-head hosted checks restarted (no terminal failure observed yet), while the
  protected merge state remains blocked pending fresh review/required checks. PR #244 has all
  required build, test, security, and coverage checks terminal-successful except its OpenCode
  review remains queued; no review or merge gate was bypassed.

## 2026-08-21 current-head follow-up

- The live DiskSage branch is `b091fc69799baecc360a9399677fcdd8196745a0`. The repository-local
  hourly advisory contract test passed 2/2 and `svelte-check` reported 0 errors / 0 warnings.
  The local workflow is manual-only by design; the central `.github` scheduler owns the hourly
  OpenCode review/repair cadence at `37 * * * *` using workflow SHA
  `d1868bc20d419a121d59df303428bf633f651e75` and reusable scheduler SHA
  `a3fdaa1aacaba9443a18573f3c309fe1841fc2f0`.
- PR #213 is at exact head `b091fc69799baecc360a9399677fcdd8196745a0`; its build checks are
  in progress and analysis/security/review checks are queued, so no protected merge is claimed.
  PR #244 remains exact-head `321c4518399129f5dd78f8a7bc5e68edc8c3e2b8` with all terminal
  required checks successful except its OpenCode review, which remains queued.
- The current APFS volume has about 6.5 GiB available. `StreamingUnzipService` has exited, the
  `real_datasets` target remained 7.2 GiB / 14 files across a bounded 20-second observation, and
  `fileproviderd`/`bird` remain active. No provider process, Finder operation, CloudDocs database,
  cloud object, or user file was terminated or deleted; only the explicitly regenerable pnpm
  cache was removed after confirming no pnpm process was running.
- The release workflow now uploads the source-bound SPDX SBOM only after GitHub provenance
  succeeds and downloads the same run-attempt artifact before publication. The focused SBOM and
  hourly-workflow contracts pass 3/3, the full frontend suite passes 28 files / 122 tests,
  `svelte-check` reports 0 errors / 0 warnings, and `actionlint` passes. The current PR head is
  `c0ae0d8b68d72ba3b9214cb77f9bca365ccaaa00`; hosted checks have restarted and no merge gate is
  bypassed.
- The central `.github` DiskSage hourly workflow has repeated scheduled `startup_failure` runs,
  including `31991358711`, before any job was created. The called scheduler requires
  `id-token: write`, which the caller lacked; central repair PR #1180 adds only that permission,
  passed its hourly-cadence contract check, and is awaiting the normal required checks/review.
  Until a post-merge scheduled run completes, the hourly loop remains an identified gap rather
  than a proven live capability.

## 2026-08-21 current-head incident and authority follow-up

- The live DiskSage branch is `586703e3a994b9b5ef635c33d95d2bab72a0ef64`, fast-forwarded from the
  remote branch without force-push. Its preceding implementation head
  `c6a1524a457e139eeebb766f405bec1858d64717` contains the maintained walker and authority fixes;
  the provider-evidence boundary introduced at `6b9cd694ac9d34e8abc40de47b2ec1106ec55d90` now rejects
  `sync_complete=true` paired with `sync_state=unknown` for authorization while preserving bounded
  compatibility reads; `src-tauri/tests/provider_sync_legacy_eviction_fail_closed.rs` proves that
  the public eviction boundary returns `provider-sync-incomplete`. This closes the legacy-state
  authorization gap without granting cloud-write or source-eviction authority.
- PR #213 is open at this exact head. Required build, analysis, security, Noema, Strix, and review
  jobs are queued or in progress; no terminal failure was observed, but the protected merge state
  remains blocked. No unresolved, non-outdated review thread is present at this head, while the
  prior `CHANGES_REQUESTED` decision is stale; an exact-head OpenCode review was requested and no
  approval or merge bypass was used.
- The bounded runtime observation at `2026-08-21 08:21 +0900` found the user-visible Finder
  `real_datasets` target on the local volume with 14 ZIP files totalling about 7.2 GiB and only
  2.3 GiB available. CloudDocs retained user-initiated downloads that ran for roughly 5,535
  seconds before `cancelled`/`CKInternalError`; the default route was `utun4`, although bounded
  HTTPS checks to iCloud, Apple, and Google endpoints completed. No DiskSage, Finder, `bird`,
  `fileproviderd`, OneDrive, or Google Drive process was terminated, and no provider or user data
  was deleted. The incident remains `provider-sync-incomplete`: Finder cancellation, sufficient
  staging headroom, and a fresh quiet provider observation are required before retry.
- At `2026-08-21 09:14 +0900`, the same 7.2 GiB target was unchanged while APFS availability
  fell to 194 MiB (99% full). `bird` logged SQLite `No space left on device` failures and the
  iCloud File Provider returned internal fetch errors; no provider or Finder process was killed.
  Regenerable old user logs and two identified cache artifacts were removed, recovering about
  0.8 GiB; the source and provider databases were retained. Cloud planning now exposes
  `local-volume-headroom-insufficient` as a plan notice before review; native-copy controls and
  the existing pre-mutation/provider-sync blockers remain authoritative, while the non-staging
  provider-API fallback and existing-copy adoption stay available. The Finder cancel control is
  still the only safe way to end the already-running operation.
- `git diff --check` passes for the current worktree. A repository-wide `cargo fmt --check` still
  reports pre-existing formatting differences across unrelated files, so it is not treated as
  evidence for the new authorization behavior; hosted Rust checks remain authoritative for the
  exact head.
- The production filesystem traversal is now fully migrated from unmaintained `jwalk` to the
  maintained `walkdir` backend across scanner, duplicate, development-artifact, cloud, and
  reclaim paths. The locked Cargo metadata resolves without a direct `jwalk` dependency, and the
  shared symlink/reparse filter plus fail-closed traversal-error accounting remain in place. The
  migration is bound to implementation head `1d8c5caccce26b976f6324164ec74177c71b48a9`; hosted
  Rust compilation and the dependency contract remain authoritative.

## 2026-08-21 sensitive-config boundary

- At implementation head `dc448af600bf35b8bbcd6a4a6ec3a14bd6bf0035`, direct credential-bearing
  names are inventoried as `sensitive-config` candidates without opening their contents. `.env`
  and `.env.*` (except documented examples), credential/private-key names, and key/certificate
  extensions receive the shared `sensitive-config-file` blocker; they are excluded from metadata
  probing and contribute zero potentially reclaimable bytes. The Rust planner regression test
  covers synthetic `.env.api` and `credentials.json` names only. The new wire kind is represented
  in the TypeScript API, review reason labels, and Naruon ontology mapping. This is filename-based
  coverage, not proof that every secret-bearing file is recognizable; hosted Rust checks remain
  authoritative for the exact branch head.
- The follow-up test-only head `4b8c0b2463c60c15691a47fcd606c9182bb79a48` adds explicit coverage for
  `.env` examples, credential names, private-key/certificate extensions, and the no-probe guard.
  `git diff --check`, locked Cargo metadata, and TypeScript `tsc --noEmit` pass locally. The local
  Cargo registry source cache was removed only after confirming no Cargo/rustc process was active;
  hosted Rust, security, and review checks remain the authority for compilation and coverage.
- Head `22748f8147613013966ddaa80928ae73681c60df` records `sensitive-config-file` in the dynamic
  Goal's `blocked_source_classes`, keeping the replaceable Goal projection aligned with the Rust
  planner and ADR rather than relying on a UI-only label.
- Head `a15be7425aba9e80a48bb7eba8a669bd505a23d7` adds explicit wire-name and Naruon ontology
  coverage for `sensitive-config`, preventing the new blocked class from becoming an untested
  serialization branch.
- Head `a4de13e65b6711f97a51eb857642da585f1d0b09` leaves the ontology coverage assertion
  rustfmt-clean; no behavior or authority boundary changed.
- The current source-and-documentation line is `3fc93b1358cdf29ebaa699521aa98850ace7cc76`; the
  implementation head is `df097743eb75b9cc919d631db0ebdeffad8b7995` and the final docs-only
  binding commit records that distinction.

## 2026-08-21 current-head provider-evidence follow-up

- The Finder-cancel implementation is bound to source head `df097743eb75b9cc919d631db0ebdeffad8b7995`,
  following macOS walkdir ownership repair at `6c0347ba53185a85a3a14c4819435c98a6fe8271`. Locked Cargo
  metadata, TypeScript compilation, and `git diff --check` pass locally; a full local Cargo build
  remains intentionally deferred while the host is under provider-copy disk pressure.
- PR #213 is still mergeable but protected and blocked. Its exact-head release, test, security,
  Noema, Strix, and OpenCode checks are queued or in progress; no protected merge or approval
  bypass is claimed. The current documentation head is `ef96322baceaa3089193a9549c47c79d94de93a3`.
  The prior current-head Devin finding about `latest_api_object_id` was fixed
  by filtering `{receipt_id}-*.json` before scanning, with a regression test covering 4,096
  unrelated records; the outdated review thread was replied to and resolved.
- Provider evidence lookup remains advisory locator recovery only. It does not grant cloud-write or
  source-eviction authority: remote revalidation, destination binding, content hash, explicit
  provider-sync state, and the existing approval gates still decide authorization.
- Provider-client recovery now distinguishes `runtime_observed=false` from unavailable runtime
  evidence. OneDrive/Google Drive quit, graceful-term, and post-restart decisions fail closed on
  unavailable observations; the regression is bound to source head `ac299095854f4cd16f124a2b5dcb44023d8fffe5`.
- The replaceable cloud-offload Goal now records the same runtime-evidence fail-closed policy and
  exposes `cancel-finder-copy` as an operator action; neither projection field grants cloud-write,
  copy, attestation, or source-eviction authority.
- The prior manual Finder workaround is now a product action: a macOS-only, fixed-script
  `cancel_finder_copy` command sends Escape with a five-second bound and no user-controlled input.
  The UI exposes it only alongside concrete iCloud File Provider activity evidence; its success
  does not clear admission and the next bounded provider observation remains authoritative. The
  fixed script's two-statement separator is regression-tested at source head
  `df097743eb75b9cc919d631db0ebdeffad8b7995`.
- The UI contract now also asserts that the cancellation action is serialized through the Tauri
  command wrapper and remains disabled while a request or health refresh is active; focused API,
  privacy, admission, Goal projection, TypeScript, and JSON checks passed locally at head `3fc93b1`.
- The Finder `real_datasets` incident remains provider-sync-incomplete. The 7.2 GiB target did not
  change, diagnostic dumps totaling about 1.6 GiB were removed without touching provider state,
  and the local volume recovered to about 1.6 GiB free. The already-running Finder operation must
  be cancelled through the bounded `cancel_finder_copy` UI action and must not be retried until the
  local headroom and a fresh quiet File Provider observation satisfy the plan gates.

## 2026-08-21 immediate disk-pressure follow-up

- A second bounded inventory found an old, unreferenced temporary `trusted.tar.gz` archive under
  the macOS temporary directory; it was removed after the open-handle check, recovering about
  217 MiB. The Node compile cache, active worktrees, iCloud/File Provider state, OneDrive state,
  Google Drive support data, and all user/provider files remain untouched. APFS availability then
  measured about 2.3 GiB, while the 7.2 GiB Finder materialization remains unsafe to retry.

## 2026-08-21 central hourly-loop RCA

- Central `.github` scheduled DiskSage runs `31960074438` through `31991358711` ended in
  `startup_failure` before creating a job. The reusable scheduler requests an OIDC exchange, but
  the DiskSage caller exposed only `contents: read`; GitHub therefore could not start the called
  job with `id-token: write`.
- The minimal cross-repository repair is tracked in [`.github#1188`](https://github.com/ContextualWisdomLab/.github/pull/1188)
  at exact head `7f9f9f0`, with 24 focused contract tests passing locally. The current cadence is
  not claimed operational until a protected merge and a new scheduled run complete; no provider
  secret, Copilot token, or repository write permission is added to the caller.

## 2026-08-21 Finder provider-stall follow-up

- A bounded `fileproviderctl dump com.google.drivefs.fpext -l` at 11:41:49 KST returned complete
  read-only evidence: Google Drive was temporarily disconnected, upload and download progress
  were active, reconciliation contained 2,000 entries, and the extension reported File Provider
  `-1004` (server unreachable); the local volume had about 6.9 GiB available. This is the exact
  evidence behind the user's `real_datasets` Finder copy-preparing dialog, not proof that any
  user file was lost or deleted.
- CloudArchive now offers its existing fixed `cancel_finder_copy` action for these third-party
  provider-global blockers, refreshes the same provider dump afterward, and keeps copy,
  attestation, and source eviction blocked until a fresh clear observation. No provider daemon,
  cloud object, or source file is terminated or mutated; focused TypeScript and contract tests
 pass after the change.

## 2026-08-21 iCloud materialization-stall follow-up

- At `2026-08-21 14:32:54 +0900`, the headless DiskSage iCloud probe completed its bounded
  read-only observation with `timed_out=true`, `no_progress_fetch_count=58`, and
  `no_progress_create_count=114`. New-copy admission remained blocked by
  `icloud-file-provider-no-progress`; this is incomplete provider evidence, not a successful
  copy or eviction receipt.
- The contemporaneous system log also retained File Provider extension termination after
  no-progress requests and `materializationFailed`/`stagedItemMissing` materialization errors.
  DiskSage source head `0ded557893191606ff6f91d4303fb54d5112fe45` now records those aggregate
  markers as path-free `materialization_failure_count` and `staged_item_missing_count` fields,
  exposes the fail-closed blocker and UI warning, and never persists raw paths, item IDs, or
  provider output.
- Focused Rust parser/readiness tests, the active File Provider integration test, the 29-file
  frontend suite (123 tests), `svelte-check`, and TypeScript compilation passed locally. The
  generated Cargo target was then removed, recovering 4.1 GiB; user files, Finder, `bird`,
  `fileproviderd`, CloudDocs databases, and provider-managed data remained untouched.
- PR #213 is at this exact head with checks running/queued, no open non-outdated review thread,
  and a stale `CHANGES_REQUESTED` review decision. The protected merge gate remains unchanged;
  no approval or merge bypass is claimed.

## 2026-08-21 iCloud filename/root exclusion follow-up

- The current bounded iCloud File Provider dump contains active upload/download progress and 18
  `Excluded From Sync Due To Filename` / 2 `Excluded From Sync Under Root` errors. DiskSage
  now retains only aggregate counters and redacted notices, exposes dedicated admission blockers,
  and keeps the Finder preparation state fail-closed. No filename, provider item ID, provider dump,
  Finder process, or cloud object is mutated by this observation.

## 2026-08-21 protected-PR and scheduler audit

- PR #213 follow-up hardening keeps valid provider evidence when bounded retention pruning fails,
  gives the headless Naruon readiness export the same three-stream iCloud pre-copy cohort as the
  GUI path, uses non-overwriting `/bin/cp -n` for macOS native copies, and no longer deletes a
  raced destination after a failed copy. `last-sync` is optional for native-status probe early
  termination; it remains recorded when present.

- The current DiskSage PR #213 head is `988bd24ecaeeba9bae44b38272edceccd9fbe889`, with the iCloud stall implementation, symlink-root fixes, and ADR
  binding pushed. Its checks are running or queued; the protected merge state remains blocked
  because the prior `CHANGES_REQUESTED` decision is stale and no fresh approval exists while the
  current review threads are being addressed.
- Central `.github` PR #1153 is at `035343c8a68e880a4abf27f7c947bfed9dbaafcf` and carries the
  fail-closed Strix infrastructure-unavailable repair. Central `.github` PR #1188 is at
  `82cd117d279a9b870f185b136984d82bb3ac5236` and carries the reusable-workflow OIDC caller
  permission repair. Both have normal protected checks in progress; neither is claimed merged.
- DiskSage PR #222 has a current Strix failure from the known `127.0.0.1:48080` Caido startup
  outage. It is an infrastructure failure, not a source finding; the canonical remediation is
  #1153. No Strix failure was reclassified as a source pass, and no check was bypassed.

## 2026-08-21 fresh headless iCloud incident receipt

- The exact-head `disksage-icloud-sync-health` binary completed a bounded, read-only probe at
  `2026-08-21 17:07:34 +0900`. The report was `schema_version=5`, `evidence_complete=false`,
  `native_status=null`, and `file_provider_activity.timed_out=true`; it retained 85 no-progress
  fetch markers and 144 no-progress create markers. No active upload/download progress was
  reported. New-copy admission remains `blocked` with
  `icloud-sync-health-evidence-incomplete` and `icloud-file-provider-no-progress`.
- The provider probe does not authorize cancellation, retry, cloud mutation, or local eviction.
  The receipt is path-free at the public boundary; the 21.3 GB provider-managed database
  allocation is retained only as bounded disk-pressure evidence. Finder, `bird`,
  `fileproviderd`, CloudDocs databases, and user files were not mutated.
- This receipt is the required fresh exact-head proof for the current materialization incident;
  the acceptance gap remains open until a later complete, quiet provider observation and a
  provider-specific per-item attestation are available. The protected merge state is unchanged.

## 2026-08-21 central hourly scheduler repair evidence

- The latest central `.github` scheduled run `31991358711` ended in `startup_failure` before a
  job was created. Its referenced reusable scheduler requests `id-token: write`, while the
  deployed DiskSage caller exposed only `contents: read`; therefore no OpenCode OIDC exchange or
  review-repair dispatch occurred. This is an infrastructure/workflow admission failure, not a
  DiskSage source result.
- Central `.github` PR #1188 is the current minimal repair at exact head
  `3ab34b57a7ab04eb14b5fca7994dd047df676748`. It grants only job-scoped `id-token: write` to the
  DiskSage and Clearfolio reusable-workflow callers, keeps the workflow token read-only, and
  updates the contract tests. Checks are still pending; hourly operation is not claimed until a
  normal protected merge and one successful scheduled receipt are observed.
## 2026-08-21 ontology-bound orphan cleanup follow-up

- The macOS UI now provides `관계 기반 고아 정리`. A bounded Rust planner compares installed
  application bundle IDs with cache and Application Support directory metadata, emits path-free
  ontology relations and deterministic fingerprints, and uses active-use evidence before any
  candidate can be considered. File contents are not opened and symlinks are not followed.
- Application Support, incomplete inventories/manifests, skipped entries, and active-use or
  truncated evidence are review-only. Only a complete unused cache may pass the separate exact
  phrase/rationale approval and re-plan boundary; mutation uses the existing reversible OS Trash
  journal. The LLM can annotate but cannot authorize cleanup, and no cloud/provider state is
  changed. `plan_orphan_cleanup` and `clean_orphan_candidates` are now explicit dynamic Goal
  operator actions.
- The review hardening now joins bounded Launch Services bundle inventory with the fixed roots;
  an unavailable, timed-out, truncated, or unreadable inventory is incomplete and keeps every
  cache candidate review-only. Installed-app traversal shares the five-second plan deadline,
  Info.plist reads are capped before parsing, directory active-use probes use recursive `lsof +D`,
  and active-use errors are surfaced as explicit review reasons.

## 2026-08-21 current-head live provider confirmation

- A new bounded read-only Google Drive File Provider dump returned 5,159,669 bytes with the domain
  temporarily disconnected, active upload/download progress, a 2,000-entry reconciliation queue,
  and File Provider `-1004` server-unreachable evidence. `bird` remains CPU-active. This confirms
  that the Finder “준비 중” dialog is provider-stall evidence, not a completed copy; copy,
  attestation, and eviction remain blocked. Local APFS availability recovered to about 8.9 GiB
  during the observation. The fixed Finder Escape action remains the only supported cancellation;
  no daemon, cloud object, or source file was touched.
- The PR #245 implementation snapshot `55a1c13ffb5cc1381aa1e86e2e6e73e055669c58` adds the ontology-bound orphan cache action and fails closed when
  installed-app inventory or metadata manifests are truncated, too deep, or contain directory
  iteration errors. Frontend checks passed locally (31 files, 129 tests); focused Rust orphan tests
  pass (12/12 unit plus 2/2 lsof-warning integration tests, including deep inventory/manifest,
  metadata-change, and object identity replacement rejection). Active-use probes share the
  enclosing five-second plan deadline, and the pre-trash batch revalidates the metadata manifest
  without reading cache contents or materializing provider placeholders.
  Launch Services timeout cleanup now terminates the private mdfind process group before joining
  stdout, preventing descendants from holding the planner past its deadline.
  The post-Trash read-only refresh is now separate from the mutation result, so a refresh failure
  preserves the successful cleanup receipt and clears stale UI selection.
  Hosted full Rust,
  security, and review checks remain authoritative and pending. PR #213's live base head is
  `cc693e4`.

## 2026-08-21 12:35 +0900 repeated provider-stall observation

- A fresh bounded `fileproviderctl dump com.google.drivefs.fpext -l` returned about 5.16 MB of
  read-only evidence. Multiple Google Drive domains remained temporarily disconnected with File
  Provider `-1004` server-unreachable errors, active upload/download markers, and reconciliation
  queues of 14,558, 2,000, 201, and 168 entries. `bird` and `fileproviderd` remained CPU-active.
  The Finder “준비 중” operation therefore remains provider-sync-incomplete; DiskSage must not
  treat it as a successful copy or authorize eviction. No provider daemon, cloud object, or source
  file was changed.

## 2026-08-21 13:38 +0900 iCloud active-transfer observation

- A bounded 16 KiB head of a read-only `fileproviderctl dump com.apple.CloudDocs.iCloudDriveFileProvider -l`
  showed Finder enumerators alive for the iCloud domain, sync scheduling `running`, upload progress
  at 95.24% (118,950,548,354 / 124,897,444,934 bytes), and download progress at 2.78%
  (30,311,669 / 1,091,221,225 bytes). `bird` and `fileproviderd` were still running. Because the
  diagnostic head was intentionally capped, it is active-transfer evidence, not a complete quiet
  provider attestation; copy, attestation, and eviction remain blocked.
- The bounded diagnostic was terminated without touching provider daemons, cloud objects, or source
  files. Subsequent scans must avoid raw cloud-placeholder paths because metadata inspection can
  request File Provider materialization even when the command is read-only.

## 2026-08-21 exact-head stacked PR audit

- PR #245 was rebased onto provider-sync base `cc693e4` at implementation snapshot `55a1c13`; its protected
  Release, Test, security, Noema, Strix, and review checks restarted and remain queued. The PR is
  `REVIEW_REQUIRED`/`BLOCKED`; no force-merge or approval bypass was used.
- PR #213 remains exact head `cc693e4` with CodeRabbit passing, Devin and required checks pending,
  and a stale `CHANGES_REQUESTED` review decision. No merge or provider mutation is claimed.

## 2026-08-21 exact-head native staging and OneDrive runtime follow-up

- DiskSage source head `3704dd1` closes the native-copy cleanup race identified in the P0 gap:
  macOS now copies into a command-owned `tempfile` directory, verifies bytes and source identity,
  and finalizes with bounded `/bin/mv -n`; timeout/helper failure drops only that owned staging
  directory and cannot remove a provider-owned final destination. Successful copies continue to
  write the immutable receipt. The preview and mutation-boundary 1 GiB reserve gate remains
  authoritative.
- The live OneDrive File Provider observation reported `temporarily disconnected`, active
  upload/download progress, `databaseInitError`, and root reconciliation failures with `-1004`
  `serverUnreachable`. DiskSage therefore keeps copy, attestation, and eviction blocked. The
  OneDrive client was stopped after a restart attempt reduced available space to 340 MiB; only
  regenerable package cache and an unreferenced temporary clone were removed, and no Finder,
  provider daemon, cloud object, or user file was touched.
- PR #213 now points to exact head `e6c6e34`; hosted checks/review are authoritative and a
  protected approval is still required. The remaining gap is a fresh complete, quiet provider
  receipt plus runtime E2E under safe headroom.

## 2026-08-21 paired projection read-lock follow-up

- PR #213 source head `9cf9665` now makes `read_projection_state` acquire the same receipt-scoped
  ADR/Goal pair lock as writers. The projection files remain replaceable and non-authoritative;
  immutable receipts/evidence still decide copy, attestation, and eviction. The focused cloud ADR
  suite passes 20/20, including same-timestamp concurrent pair writers and late-state regression.

## 2026-08-21 OneDrive post-headroom provider observation

- Local APFS headroom recovered to about 39 GiB, but OneDrive still reported `needs-indexing=yes`,
  77,393 pending indexable items, 81,524 reconciliation entries, active upload/download progress,
  and retained SQLite `databaseInitError` code 11. The provider is therefore still not quiet or
  complete; DiskSage keeps copy, attestation, and eviction fail-closed. This separates the active
  provider-index backlog from the earlier low-space pressure incident.

## 2026-08-22 readiness verifier integration boundary

- The Naruon readiness verifier's source comment is now valid both as a standalone binary and when
  included by the integration test that locks its `--help`/absolute-path parser boundary. This
  repairs the exact-head Rust test failure without changing readiness, copy, or eviction authority.

## 2026-08-22 current Finder/iCloud stall and exact-head repair evidence

- The user-visible Finder operation still reports `real_datasets` as “복사 준비 중” after hours.
  A bounded, read-only iCloud File Provider observation at about `02:08 +0900` found an active
  upload of `4,170,552,115 / 5,462,125,152` bytes (76.35%), 28,694 pending indexable items,
  `scheduling=running`, active materialization, and sync-exclusion notices for filenames and roots.
  The dump exceeded its wall-clock bound and was truncated; these are incomplete provider-global
  markers, not a per-item upload receipt. No retained marker binds an exclusion to `real_datasets`.
- The data volume measured about 926 GiB total, 873 GiB used, 12 GiB available (99%). Finder,
  `fileproviderd`, and `bird` remained alive. DiskSage did not kill a process, touch a CloudDocs
  database, materialize a placeholder, cancel Finder, or mutate a source/cloud object. Its own
  admission remains `provider-sync-incomplete`; only the existing operator-visible Finder Escape
  action may request cancellation, followed by a fresh complete and quiet observation.
- The exact-head fix `a6ec6e2` starts the bounded `lsof` active-use probe in a private process group
  and kills that group before joining output readers on timeout. This closes the shell-descendant
  pipe leak that could starve the independent `ps` probe and report a false active-use timeout.
  The focused Rust test passed 3/3. The same patch is present on stacked PR heads `a0fa7bc` (#247)
  and `741ab30` (#246); hosted checks are rerunning and protected merge/review is still pending.

## 2026-09-06 agent-session preservation experiment (Proposed)

Baseline main `0e90f9cebadbd7f59606baaec4ca1d2f178c899a` does not protect Codex/Claude state in its shared path guard. Eight labeled session paths all pass the baseline gate. Branch `codex/session-preservation-autoresearch` adds state-root and containing-tree protection, including explicit environment overrides and native Git removal. The standalone regression metric improves from 8/8 admitted paths to 0/8; this is not a population false-positive-rate estimate or deployed behavior. Four non-session controls remain eligible under this additional guard. See [experiment evidence and reproducible commands](doctoring/session-preservation/README.md).

PR [#345](https://github.com/ContextualWisdomLab/disksage/pull/345) carries this proposal. The focused offline safety-module harness passed 47 tests at `8bb7e54d`; it does not replace full-repository CI. P0 acceptance remains open until exact-head integration checks and protected merge succeed, the permanent deletion authority delta in PR #263 is integrated, and all native deletion/eviction routes are checked. P1 effectiveness remains open: zero live bytes were reclaimed in this experiment, and real labeled candidate inventory plus allocated-byte/free-space evidence is still required. Never obtain a zero metric by disabling every reclaim path or by excluding failed/incomplete cases. Existing PRs retain their deltas and are not closed by this experiment.
