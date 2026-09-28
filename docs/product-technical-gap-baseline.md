# DiskSage product and technical gap baseline

**Snapshot:** 2026-08-24 18:51 +0900 (Asia/Seoul)
**Repository heads at snapshot:** the dated inventory and 18:30 correction below supersede earlier
historical captures; hosted checks and protected review remain authoritative, and no merge is
claimed from queued, stale, or bot-only status.
**Product boundary:** local-first macOS disk pressure relief with iCloud, OneDrive, and Google Drive destinations.
**Evidence rule:** this document is a dated baseline, not an authority for transfer or deletion. Runtime receipts, provider attestations, object identity, and current GitHub checks remain authoritative.

## Current product contract

1. Scan and metadata profiling are read-only and metadata-first: embedded metadata precedes an unambiguous filename token, then filesystem creation/modification time. A filename token such as `2026-04-28` or `251210` is secondary evidence and never proves ownership, upload, or eviction authority.
2. A cloud candidate follows `copy-verified → pending-provider-sync → provider-sync-confirmed → eviction-ready → source-evicted`. `local-current` with `is_uploaded=false` is `pending-upload`; no eviction permit is issued.
3. Native File Provider copy is bounded, re-hashed, and source-identity rechecked. Provider-global timeout, quota/auth uncertainty, local headroom shortage, stale worktree metadata, or incomplete metadata fail closed.
4. Regenerable caches are a separate reclaim domain. They are per-child, identity-bound, active-use checked, journaled, and moved to OS Trash; they are not uploaded as user data.
5. Deterministic Rust gates own safety. A local model may judge only the fixed maintenance command after dry-run evidence, calibration, and explicit human confirmation. No external LLM or OAuth service is a runtime prerequisite for the standalone product.

## 2026-08-24 18:11 +0900 current protected PR inventory

This is the current review queue captured from GitHub immediately before this snapshot. A commit
SHA is authoritative only for the PR row where it appears; a later push invalidates predecessor
checks and approvals.

| PR | Exact head | Draft | Merge state | Review state | Current interpretation |
| --- | --- | --- | --- | --- | --- |
| #249 | `dc9ccf2a215061fba5bea2a23e8df3e84a0cd072` | yes | blocked | review required | Git worktree audit help; process tests use Cargo's shipped binary without nested temp builds |
| #247 | `e4cfd1ce84148f490a94e0093e59a9ce9fb2f735` | yes | blocked | review required | iCloud provider indexing plus live Finder/provider stall evidence; dated headroom-fixture repair, explicit Finder Accessibility permission guidance, and dynamic ADR maintenance |
| #246 | `1972614ee5488cca34deeb3bd999d369c61b3de1` | no | blocked | review required | Storybook/accessibility contract; iCloud stall clock test slice is 7/7 |
| #244 | `13caeb04333e50e57c8a51a11b64aeb131c080b2` | no | blocked | review required | Rust 1.97.1 compiler baseline |
| #204 | `5bf86a9c593888fe5f08bff9f8dea74e5f1299ae` | yes | blocked | review required | DiskSage shell/icon identity; every Test/Release Node bootstrap and zlib ABI pinned after hosted runtime mismatch |
| #227 | `753352a1d0cd7e297bb656d5edf9339235a628a3` | yes | blocked | review required | symlink-root audit hardening; Ubuntu active-use tests now install `lsof` |
| #225 | `3715a5ada760072d3675026fe7f264b4ee47964f` | yes | blocked | review required | cwd-relative organize targets fail closed; Windows regressions use platform-absolute homes |
| #212 | `75d728e403cf0b30511e149a7e650731f6472733` | no | blocked | review required | cloud operational CLI help |
| #206 | `2e7b845b7610a871ec5981d964bcab5cb99df41d` | no | clean | none | content-bound Homebrew execution; no qualifying approval |
| #205 | `5c86668a6e503a174ff0b07151f67226b39547ff` | no | clean | none | Intel Homebrew target support; no qualifying approval |
| #203 | `5f0bd51be4b2faca8a30aadc661bf651a619c549` | no | blocked | review required | TopFiles accessibility contract |
| #202 | `ec2db50307d0d6bccd2546a820c7a6822f054df5` | no | blocked | review required | bounded scan/navigation failure feedback |
| #189 | `66d7aa767d416048a752c5c550e8d64e03213e0e` | no | blocked | review required | Homebrew cleanup status UI |

Additional draft dependency/security PRs remain open and are not merge candidates. No protected
merge is inferred from `clean`, green predecessor checks, bot comments, or queued reviews. The queue
is processed exact-head-first: review, repair, recheck, then normal protected merge.

## Buyer-observable product gaps

| Priority | Gap / observable symptom | Evidence | Acceptance criterion |
| --- | --- | --- | --- |
| P0 | Cloud offload can remain blocked while a provider is syncing or reports `local-current`/`is_uploaded=false`; the user sees no safe reclaim despite free cloud capacity. | Existing provider-global and iCloud native-state gates; `bird`/`fileproviderd` remain active during the current incident, while the root volume has about 96 GiB available. | UI explains the exact blocker, last evidence time, and next bounded retry; a verified provider attestation alone can advance a candidate, never a stale projection. |
| P0 | A long Finder/provider copy can appear hung and consume the remaining local headroom. | A repeated exact-head iCloud dump remained unchanged for 21 seconds with `pending-indexable-count=12474`, upload `0/5038` at `0.0000`, active upload/download markers, and 18 filename plus 2 root exclusions. | UI reports the indexing backlog and stable blocker duration; Finder copy, attestation, and eviction remain fail-closed until a fresh quiet provider observation. |
| P1 | Personal desktop-client capacity is not the same as API quota; OAuth is unnecessarily implied for a single-user installation. | ADR-0001 permits copy-only desktop-client mode marked `capacity-unverified`; the cloud connection UI defaults to read-only OAuth consent and requires an explicit write-access opt-in. | Settings clearly distinguish local desktop client, API quota, and organization OAuth; no OAuth prompt is required for the local-only path. |
| P1 | Users cannot yet see a full lineage graph connecting source, metadata, archive member, provider item, receipt, Goal, and eviction decision. | The candidate UI now exposes a compact source→metadata→archive→provider lineage panel using the stable fingerprint, confidence, and blocker state; provider item/receipt/permit remain explicitly pending until their evidence exists. | Export and UI show stable content IDs, provenance edges, confidence, and blockers without exposing raw private paths. |
| P1 | “Orphan”/duplicate cleanup is difficult to trust because relationship evidence is not visible before action. | Ontology and duplicate/orphan PRs are open; current default path remains fail-closed. | Every proposed removal has an explainable parent/child/duplicate relation, identity recheck, reversible Trash action, and a no-candidate result when evidence is incomplete. |
| P2 | Cross-platform behavior and accessibility are not presented as one release contract. | macOS/Linux/Windows release checks exist; several UI accessibility PRs remain open. | Release notes and UI expose platform capability matrix, keyboard/assistive labels, and bounded failure messages for each action. |

## Technical and operational gaps

| Priority | Gap | Current state | Smallest next proof |
| --- | --- | --- | --- |
| P0 | Provider end-to-end receipt is absent for the current iCloud incident. | Global probe can time out and CloudDocs state is intentionally not force-killed or deleted; the native copy boundary now requires an integrity-checked three-stream pre-copy cohort before mutation. | Capture a bounded fresh provider evidence receipt after sync settles; keep transfer/eviction disabled until it is complete. |
| P0 | Disk pressure telemetry and provider queue evidence must remain comparable across loops without retaining raw provider output. | Cloud plans and explicit provider health refreshes persist bounded, path-free `LocalVolumeSnapshot`, `ProviderClientRuntimeSnapshot`, `IcloudSyncHealthEvidenceSnapshot`, and `ProviderGlobalSyncEvidenceSnapshot` records under `volume-pressure-evidence`, `provider-client-runtime-evidence`, `icloud-sync-health-evidence`, and `provider-global-sync-evidence`; iCloud plans combine their three-stream cohort, while third-party plans retain the provider-global stream for restart-safe blocker duration. | Missing, incomplete, malformed, or more-than-five-minute-skewed iCloud cohort observations remain blocked; third-party provider-global history can only extend a matching diagnostic clock and never grants copy, attestation, or eviction authority. |
| P1 | Hourly product-development/review loop is not yet live in this repository environment. | The repository-local `.github/workflows/hourly-product-loop.yml` is intentionally `workflow_dispatch`-only because its direct contextual-orchestrator HTTP call is advisory and not a pinned OpenCode worker. The trusted central [`disksage-hourly-review-repair.yml`](https://github.com/ContextualWisdomLab/.github/blob/main/.github/workflows/disksage-hourly-review-repair.yml) runs at `37 * * * *` and dispatches the pinned scheduler `a3fdaa1aacaba9443a18573f3c309fe1841fc2f0`, which performs the OpenCode OIDC exchange. The local workflow still uploads a seven-day path-free receipt when manually configured; no external endpoint or deployment receipt is available here. | Verify one central scheduler receipt and one local manual advisory receipt; preserve read-only permissions, exact-head binding, and no provider-secret import or mutation. |
| P1 | Open PR queue prevents a clean protected release line. | The current exact-head inventory above still has protected review/quorum gaps; PR #189 and #247 have checks running, while PR #238 is green but has no qualifying approval. | Process one PR at a time: current-head review → fix → required checks → fresh approval → normal protected merge; never bypass or self-approve. |
| P1 | Current UI coverage is contract-heavy rather than runtime E2E for native File Provider states. | The UI now displays `로컬 최신본·업로드 미확인` and maps blockers without backend detail; provider operations are not safely reproducible on this full disk. Rust fixtures now cover `local-current + is_uploaded=false`, provider timeout, timeliness transitions, and receipt/evidence invalidation; native runtime E2E remains unavailable while the provider is unhealthy. | Keep the fixture-backed state machine green and add a bounded native E2E receipt only after a quiet provider observation is authoritative. |
| P1 | Preview headroom could disagree with native mutation headroom on cross-volume layouts. | The mutation boundary already probes the destination staging ancestor, while the old preview/UI gate used the source-volume snapshot. | The planner now probes the destination for every visible unblocked candidate and the native UI follows the resulting insufficient/unverified notices; provider-API upload remains separate. |
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

## 2026-08-24 18:51 +0900 exact-head and host delta

- PR #225 is at `3715a5ada760072d3675026fe7f264b4ee47964f`; its Windows failure was reproduced from
  the hosted log: two tests supplied POSIX `/home/u`, which the fail-closed Windows resolver correctly
  rejects. The existing `platform_home()` fixture now supplies a Windows absolute path; pinned Rust
  organize tests pass 21/21, with `rustfmt --check` and `git diff --check` passing.
- PR #227 is at `753352a1d0cd7e297bb656d5edf9339235a628a3`; its Ubuntu failure was caused by the
  runner missing `lsof`, which the fail-closed active-use probe requires. The existing system-dependency
  step now installs `lsof`; the local full Rust suite passes 735/735 with one ignored live-provider test.
- A fresh read-only host observation measured 16 GiB available on `/` (926 GiB total, 43% used), while
  `brctl status` still reports iCloud `needs-sync` and repeated `pending-scan` entries roughly 1.37 hours
  old. Finder, `fileproviderd`, and `bird` are running; no DiskSage process was present. This supports a
  provider reconciliation/indexing stall, not local disk exhaustion or a proven DiskSage lock.
- The Finder `real_datasets` copy remains unmaterialized in the bounded provider evidence. No Finder
  cancellation, provider restart, CloudDocs write, cloud mutation, source mutation, attestation, or
  eviction was performed; `provider_sync_attested=false`, `local_eviction_authorized=false`, and
  `mutation_performed=false` remain the only safe state until fresh per-item evidence exists.

## 2026-08-21 23:30 +0900 live iCloud Finder-preparation receipt

- The exact-head `disksage-icloud-sync-health` binary completed a bounded, read-only iCloud
  observation. The report was `evidence_complete=true`, native status `needs-sync-up` plus
  `needs-sync-down`, and File Provider activity schema 3 with one no-progress fetch, active
  upload/download markers at `953100`/`988500` millionths, and `pending_indexable_count=17547`.
  The upload queue also retained six `blocked_on_sync_up` items; 18 filename exclusions and two
  root exclusions were observed. New-copy admission is `blocked`; `mutation_performed=false`.
- `fileproviderctl` also showed active iCloud materialization/fetch jobs and a roughly 14,965-entry
  reconciliation backlog. This is consistent with the Finder `real_datasets` “복사 준비 중”
  symptom, but it is not a per-item copy receipt. DiskSage keeps copy, attestation, and source
  eviction fail-closed until a fresh complete quiet observation and per-item provider evidence
  exist. No Finder/provider daemon, CloudDocs database, cloud object, or source file was changed.
- Current exact PR heads are UX #246 `fc9f4a4c465fc5ef355f7fbf552ff4295cf4f609` and provider
  #247 `45214018dff43c6ba7c71253bc50e8c0eab0e1bd`; their hosted checks remain pending, while
  local Rust/UX validation is green. The live observation does not authorize a protected merge.

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

## 2026-08-21 bounded-planning and evidence-retention follow-up

- Exact duplicate detection now runs before the candidate presentation limit, so a duplicate pair
  split by `limit` still marks the visible candidate for human canonical selection and remains in
  the path-free cluster summary. A focused Rust regression test covers this boundary.
- Provider evidence retention protects the just-written record when its timestamp is older than
  the existing history, preventing clock regression from deleting fresh proof. The retention
  integration test covers the bounded 128-record history. Local implementation commit `c5aa3a1`
  is not yet published because the repository ruleset currently rejects direct branch updates.

## 2026-08-21 current exact-head iCloud/indexing and PR audit

- Exact-head local commit `c5edabd` adds the path-free iCloud File Provider
  `pending_indexable_count` field, emits `icloud-file-provider-indexing-pending`, includes it in
  Naruon readiness and the stable UI blocker fingerprint, and records the change in ADR-0001,
  this baseline, and `CHANGELOG.md`. The pinned Rust 1.97.1 parser test passed; `npm run check`
  reported 0 errors/0 warnings and the CloudArchive contract suite passed 3/3.
- The rebuilt `disksage-icloud-sync-health` observed at `2026-08-21 21:54:33 +0900` returned
  `schema_version=5`, complete evidence, native `idle`/`has-synced-down`, File Provider activity
  schema 3 with pending indexable `12474`, active upload/download `1/1`, and filename/root
  exclusions `18/2`. Admission remains blocked by upload-in-flight, both exclusion blockers,
  indexing-pending, and transfer-active; `mutation_performed=false`.
- A normal push of `c5edabd` was rejected by ruleset `GH013` because branch changes must go through
  a pull request and the central required workflows are unsatisfied. No bypass, force-push, admin
  merge, or self-approval was used. Remote PR #213 therefore remains at `108bba0`; the local
  follow-up is explicitly unprotected until a normal PR path becomes available.

## 2026-08-21 iCloud indexing backlog follow-up

- A repeated read-only iCloud File Provider observation remained unchanged for 21 seconds with
  `pending-indexable-count=12474`, upload progress `0/5038` at `0.0000`, 18 filename exclusions,
  and 2 root exclusions. DiskSage now exports the aggregate indexing backlog, blocks new-copy
  admission with `icloud-file-provider-indexing-pending`, and surfaces the count in the Finder
  “복사 준비 중” warning. No provider or source mutation was performed.

## 2026-08-21 current iCloud indexing and transfer receipt

- A fresh bounded read-only `fileproviderctl` observation completed at `2026-08-21 22:22:53 +0900`.
  It reported `needs-indexing=no`, pending indexable `13737`, a `12449`-entry reconciliation
  backlog, active upload/download markers, one no-progress fetch, and filename/root exclusions
  `18/2`. The bounded dump was truncated; DiskSage persisted only path-free aggregate evidence and
  set no-progress, indexing-pending, transfer, and exclusion blockers. `mutation_performed=false`;
  Finder preparation is not a copy receipt.

## 2026-08-21 provider disk-full marker boundary

- Provider-global sync parsing now treats only the exact numeric markers `errno 28`,
  `odresult_errno 28`, and `OSStatus -34` as local-disk-full evidence; longer values such as
  `errno 280` are retained as generic provider errors. The focused boundary regression passes,
  and no provider, source, or cloud state was mutated.

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

## 2026-08-22 File Provider disk-import detection

- A fresh bounded `fileproviderctl` observation showed the iCloud domain with Finder enumerators,
  `disk import: yes`, active upload progress of `5,202,024,494 / 5,462,125,152` bytes, and
  `pending-indexable-count=30,960`. These aggregate markers explain why Finder can remain in
  “복사 준비 중”, but they do not bind the operation to `real_datasets` or prove a per-item cloud
  copy. DiskSage now records the redacted `icloud-file-provider-disk-import-active` notice,
  projects it into the new-copy admission blockers and Naruon readiness export, and shows it next
  to the existing fixed Finder-cancel action. Copy, attestation, and source eviction remain
  fail-closed; no provider process, source, CloudDocs database, or cloud object was mutated.

## 2026-08-22 04:05 +0900 unchanged iCloud preparation queue

- A second read-only host observation found the same bounded aggregate values after the earlier
  disk-import evidence: `pending-indexable-count=31,024`, upload
  `5,202,024,494/5,462,125,152` (95.24%), download `0/828`, and `disk import: yes`.
  `brctl` still reported `needs-sync-up`/`needs-sync-down`, with last sync at
  `2026-08-21 20:20:10.166 +0900`; many `pending-scan` entries were three or more hours old.
- This strengthens the product diagnosis of a stalled File Provider preparation queue but does
  not bind the state to a particular Finder item or prove a cloud copy. DiskSage performed no
  cancellation, daemon restart, provider-database write, materialization, cloud mutation, or
  source mutation. The runtime Goal remains `provider-sync-incomplete`; copy, attestation, and
  source eviction remain blocked until a fresh complete quiet observation and independent
  per-item evidence exist.

## 2026-08-22 persisted stall-duration wiring

- The current bounded probe at `2026-08-21 20:21:04 +0000` still reports two no-progress fetches,
  `pending-indexable-count=31882`, unchanged aggregate upload/download counters, and active disk
  import. The iCloud health command already derives `admission_blocked_since_ms` from the
  integrity-checked evidence journal, but the frontend previously ignored that field and restarted
  its 15-minute clock after a UI/system restart.
- DiskSage now carries the field through the TypeScript report contract and uses it as the UI stall
  clock origin, with a current-observation fallback only when persistence is unavailable. This
  makes the screenshot's long-running “복사 준비 중” state remain visible as a stall after restart;
  it does not cancel Finder, write provider state, or authorize copy, attestation, or eviction.

## 2026-08-22 06:05 +0900 repeated Finder preparation stall

- A new bounded, read-only observation still found four `fetchContentsForItemWithID` requests with
  no progress, `pending-indexable-count=31882`, active disk import, unchanged aggregate upload
  (`5205160706/5465661912`) and download (`10647837/11116116`) counters, and `brctl` flags
  `needs-sync-up|needs-sync-down`. Finder had remained alive for roughly 18 hours and the data
  volume had only about 4.9 GiB available.
- The observation confirms provider-level preparation debt but remains aggregate evidence: it does
  not identify the seven Finder items in `real_datasets` or prove any cloud copy. DiskSage keeps
  the runtime Goal `provider-sync-incomplete`, copy/attestation/source eviction fail-closed, and
  exposes only the explicit bounded Finder-cancel action; no Finder/provider process, CloudDocs
  database, source, or cloud object was mutated.

## 2026-08-24 current File Provider reconciliation backlog

- A fresh bounded, read-only `fileproviderctl` observation after the system restart reported
  `needs-indexing=no` but `pending-indexable-count=32377`, a `28123`-entry reconciliation queue,
  upload progress `6229217391/6540678102` (95.24%), `scheduling state: running`, `disk import: yes`,
  and `stream reset: yes`. `brctl status` still reported `needs-sync-up|needs-sync-down` with the
  last sync at `2026-08-21 20:20:10.166 +0900`; repeated pending scans were roughly 54 hours old.
- These are provider-global markers and do not bind to the seven Finder items in `real_datasets` or
  prove a per-item cloud write. DiskSage therefore keeps Goal `provider-sync-incomplete`, copy,
  attestation, and source eviction fail-closed, and leaves only the explicit bounded Finder-cancel
  action available. No Finder/provider process, CloudDocs database, source, or cloud object was
  mutated by this observation.

## 2026-08-24 11:13 +0900 provider-specific Finder stall follow-up

- A fresh read-only Google Drive File Provider dump was approximately 4.99 MiB, confirming that
  the provider-wide probe must retain its 32 MiB bounded cap; the product branch already carries
  that cap and parses `temporarily disconnected`, `NSFileProviderErrorDomain -1004`, active
  transfer, reconciliation, and item-not-found markers without retaining paths.
- The live log recorded Google Drive root materialization failures with File Provider error
  `-1004` (server/device connection unavailable) while iCloud continued redacted item ingestion.
  This makes the provider identity part of the user diagnosis: a Finder “preparing to copy” dialog
  is not sufficient evidence of a cloud write and cannot be mapped to `real_datasets` without an
  item-level receipt.
- Current exact heads are PR #247 `3e43e0d4d3aa15a7f25161f4107bf3f2c29d261f` and PR #156
  `25b3e42be7e0e22cafca878ef25383959dd773d6`; both have hosted checks still running/queued and
  neither has a qualifying protected approval. No process, provider database, source file, or
  cloud object was mutated. The runtime Goal remains `provider-sync-incomplete` and all copy,
  attestation, and source-eviction gates remain fail-closed.

## 2026-08-24 11:34 +0900 worsening iCloud preparation queue

- A subsequent bounded read-only iCloud observation increased `pending-indexable-count` to `39404`
  and reconciliation to `35150`; upload remained `6229217391/6540678102` (95.24%) with scheduling
  running, `disk import: yes`, and `stream reset: yes`. `brctl` still reports
  `needs-sync-up|needs-sync-down`, with pending scans about 55 hours old.
- The aggregate queue is worsening rather than quieting. It remains incident evidence only: it
  does not identify the seven `real_datasets` items or prove a cloud write. DiskSage keeps the
  runtime Goal `provider-sync-incomplete`, exposes only the bounded Finder-cancel action, and
  keeps copy, attestation, and source eviction fail-closed. No provider process, CloudDocs
  database, source file, or cloud object was mutated.

## 2026-08-24 provider-stall duration persistence

- The provider-global admission report now carries a backend observation timestamp and an optional
  `admission_blocked_since_ms` value. OneDrive/Google Drive probes persist only bounded, path-free
  aggregate snapshots with create-only `0400` records, `0700` directory permissions, SHA-256
  integrity, and 128-record retention; raw File Provider dumps and user paths are not retained.
- CloudArchive consumes the persisted onset for the same provider/blocker fingerprint. Therefore
  a Finder “복사 준비 중” dialog that survives a restart is shown as a continuing stall instead of
  a newly observed five-minute window. Invalid or tampered history falls back to the current
  observation and remains fail-closed; the feature never cancels Finder or authorizes cloud copy,
  attestation, or source eviction.

## 2026-08-24 12:16 +0900 exact-head and review repair correction

- PR #247's latest source fix is `7e82b0c` after the provider-global restart-duration
  implementation. It scopes the persisted stall walk to the observed provider and preserves the
  onset already accumulated when an older record cannot be read or parsed. The earlier
  `7fa3f7d...`, `3db3c33...`, `3e43e0d...`, and `2ee31ea...` rows are predecessor evidence; their
  checks and reviews are stale. The PR remains ready for review with no qualifying approval, and
  the live PR head/checks must be re-fetched after this documentation publication.
- Parent PR #213 is ready for review at exact head `0584bcc600e037d564a4ff254b6e8570361d9218`;
  its hosted coverage/security/release checks are green, but protected review quorum is absent.
- Local proof for the source fix is Rust provider-global 20/20, provider/readiness
  integration tests 6/6, frontend Vitest 32 files/133 tests, and `svelte-check` 0 errors/0
  warnings. These checks do not authorize a protected merge or any Finder/provider/source/cloud
  mutation.

## 2026-08-24 12:26 +0900 repeated Finder preparation stall

- Finder PID 1422 has been alive for about 1 hour 42 minutes; `fileproviderd` remains active at
  about 22% CPU while `bird` is present. The `real_datasets` destination remains 14 entries,
  512 bytes, and mtime `2026-08-20 03:28:07`, so no destination byte-copy progress was observed.
- The root volume has about 91 GiB available. `brctl status` still reports iCloud
  `needs-sync-up|needs-sync-down` with the last sync at `2026-08-21 20:20:10.166 +0900` and
  repeated pending scans. This is a provider preflight/indexing stall, not local capacity pressure
  and not proof of a cloud write for the seven Finder items shown in the dialog.
- DiskSage performed read-only inspection only. It did not cancel Finder, restart/kill provider
  daemons, modify CloudDocs/provider state, or mutate source/cloud data; Goal
  `provider-sync-incomplete` and all copy/attestation/eviction gates remain fail-closed.

## 2026-08-24 12:35 +0900 Strix provider-prefix failure in the open-PR queue

- DiskSage PR #249 exact head `44390608d30417477f6a66601b18a53ca87b0a9c` has a failed Strix
  check. The run reached its configured fallback `openai-direct/gpt-5.6-luna`, but LiteLLM
  rejected that hyphenated provider prefix (`LLM Provider NOT provided`) before producing a
  vulnerability report; the required check correctly failed closed rather than treating zero
  findings as authoritative evidence.
- The root repair is in the central `.github` PR #1263 exact head
  `3669bceba9679883d10ffa859eea87bf4705dfd3`: normalize `openai-direct/` to
  `openai_direct/`, dispatch LiteLLM as `openai/gpt-5.6-luna`, and switch the credential/API-base
  boundary for cross-provider fallbacks. Its current head has no unresolved review thread, but
  its protected review decision remains stale `CHANGES_REQUESTED` while the Strix check runs.
- This is CI-provider infrastructure evidence, not a DiskSage data or Finder mutation. No local,
  provider, source, or cloud data was changed by the diagnosis.

## 2026-08-24 12:39 +0900 executed DiskSage iCloud admission probe

- The exact product head's `disksage-icloud-sync-health` binary completed a read-only local
  CloudDocs/WAL snapshot with `evidence_complete=true`, `mutation_performed=false`,
  `provider_sync_attested=false`, and `local_eviction_authorized=false`. The snapshot is
  supplementary global evidence; it does not claim a per-item cloud write for `real_datasets`.
- iCloud reported `needs-sync-up|needs-sync-down`, 343 uploads blocked on sync-up, one active
  upload at 95.24%, one active download, and `pending_indexable_count=58183`. The admission state
  is `blocked` with transfer-active, disk-import, indexing-pending, root/filename exclusion, and
  native sync-up/down blockers. This directly explains why Finder remains in “복사 준비 중”.
- The probe read SQLite through a copy-on-write snapshot including WAL files, redacted paths, did
  not write CloudDocs/provider state, and did not cancel Finder or mutate any source/cloud object.

## 2026-08-24 12:15 +0900 current Finder preparation stall observation

- Finder has remained alive since `10:43:49 +0900`, while the visible `real_datasets` operation
  remains in “복사 준비 중”. The local destination directory's mtime and size stayed unchanged
  at `2026-08-20 03:28:07` and 512 bytes across bounded checks from `12:14:02` through
  `12:14:12`; no new destination or temporary file appeared after the incident start window.
- The root volume had 86 GiB available, so local capacity is not the current blocker. A bounded
  Finder sample stayed in DesktopServices/FileProvider URL-property and child-synchronization
  work rather than a byte-copy path. This is evidence of preflight/provider waiting, not a copy
  receipt and not proof that any of the seven displayed items reached a cloud object.
- `brctl` still reports `needs-sync-up|needs-sync-down` with the last sync at
  `2026-08-21 20:20:10.166 +0900`. OneDrive's latest diagnostic reported zero bytes/files
  queued and no download/upload failures; this does not prove Finder's source selection, so the
  product keeps the provider identity and item-level receipt separate.
- DiskSage performed only read-only inspection. It did not cancel Finder, restart or kill
  `bird`/`fileproviderd`, write a CloudDocs/provider database, or mutate a source or cloud object.
  Goal `provider-sync-incomplete`, copy/attestation/eviction gates, and the explicit bounded
  Finder-cancel action remain unchanged.

## 2026-08-24 12:51 +0900 impossible stall-onset values rejected at the UI boundary

CloudArchive now accepts a persisted blocker onset only when it is a safe integer in the observed
time range. Negative, future, non-finite, or otherwise impossible values fall back to the current
backend observation instead of producing a negative duration or hiding the 15-minute Finder-stall
warning. The focused Vitest contract passes four cases and `svelte-check` reports zero diagnostics;
this diagnostic guard grants no copy, attestation, cloud-write, or source-eviction authority.

## 2026-08-24 12:57 +0900 frontend coverage evidence

The exact product worktree ran all 32 frontend test files (134 tests) successfully. V8 reports
100% statements (211/211), branches (70/70), functions (83/83), and lines (173/173) for the
instrumented frontend surface, including the impossible stall-onset contract. This is frontend
test evidence only; it does not imply repository-wide 100% coverage or provider/cloud authority.

## 2026-08-24 12:58 +0900 iCloud preparation queue remains blocked

The latest exact-head read-only probe still reports `new_copy_admission_state=blocked` and
`mutation_performed=false`. The native summary remains `needs-sync-up|needs-sync-down`; 343 upload
items are blocked on sync-up, one upload and one download are active, and FileProvider's pending
indexable count increased to 64,969 (from 58,183 at 12:39). Disk import, transfer activity, and
the 28 filename/2 root exclusions remain present. The Finder target is unchanged at 14 entries,
512 bytes, mtime `2026-08-20 03:28:07 +0900`, with about 101GiB free on `/`. DiskSage therefore
continues to block new copy, attestation, and source eviction; no Finder/provider/source/cloud
mutation was performed.

## 2026-08-24 13:04 +0900 iCloud indexing backlog increased

A subsequent exact-head read-only probe observed the same native `needs-sync-up|needs-sync-down`
state, 343 uploads blocked on sync-up, one active upload at 95.24%, one active download, and
`new_copy_admission_state=blocked`. FileProvider pending indexable items increased from 64,969 to
67,017 while disk import, transfer activity, and the 28 filename/2 root exclusions remained
present. Aggregate evidence still has `provider_sync_attested=false`, `local_eviction_authorized=false`,
and `mutation_performed=false`; no Finder/provider/source/cloud mutation was performed.

## 2026-08-24 13:12 +0900 iCloud backlog growth and process attribution

The next exact-head read-only probe observed `pending_indexable_count=74,946` (up from 67,017),
the same native `needs-sync-up|needs-sync-down` state, 343 uploads blocked on sync-up, one active
upload at 95.24%, one active download, and `new_copy_admission_state=blocked`. Finder's
`real_datasets` destination remained 512 bytes with mtime `2026-08-20 03:28:07 +0900`; `/` had
about 99GiB available. Bounded process inspection saw `fileproviderd` at 72–129% CPU but no
DiskSage process, CloudDocs database, or source path open in it. This is provider-side backlog
evidence, not proof of a DiskSage database lock; provider attestation, eviction authorization, and
all mutations remain disabled.

## 2026-08-24 13:12 +0900 live protected PR inventory correction

The earlier 12:51 table is historical. The live GitHub inventory now has PR #247 on `main` at
`584d0ede1a6fef75b7bfc2191aa3ea47e59b2a66` (open, non-draft, checks pending, review required), PR
#246 at `476678c150ded97b400d62566292adfff56a84c2` (open, non-draft, all required checks pass but
no approvals), and the remaining open queue includes #249, #244, #238, #236, #234, #232, #231,
#230, #228, #227, #225, #223, #222, #220, #218, #217, #216, #215, #214, #212, #209, #208,
#207, #206, #205, #204, #203, #202, #200, #199, #198, #197, #195, #193, #192, #190, #189,
#188, #187, #186, #182, #181, #179, #174, #156, #150, and #149. No protected merge is inferred
from `CLEAN`, green predecessor checks, or bot comments; the live ruleset still requires two
independent approvals, last-push approval, resolved threads, and normal merge/squash.

## 2026-08-24 13:53 +0900 live Finder/provider follow-up

- A bounded local recheck at `13:48:21 +0900` found about 96 GiB available on `/`. Finder PID 1422,
  `fileproviderd` PID 1450, and `bird` PID 1462 had all remained alive for roughly 3 hours; the
  `real_datasets` target was still 512 bytes with mtime `2026-08-20 03:28:07 +0900`. No target
  handle appeared in the bounded process handle sample; File Provider held only its Mobile Documents
  root and `bird` held CloudDocs session database shared-memory files.
- The latest complete DiskSage iCloud health receipt available for this loop (`13:12`) reported
  `new_copy_admission_state=blocked`, 343 uploads blocked on sync-up, one active upload at 95.24%,
  one active download, and 74,946 pending indexable items. The evidence is aggregate and does not
  identify the seven Finder items or prove a cloud write; `provider_sync_attested=false`,
  `local_eviction_authorized=false`, and `mutation_performed=false` remain explicit.
- This confirms a File Provider reconciliation/indexing backlog rather than local disk exhaustion or
  a DiskSage lock. The UI keeps the explicit bounded Finder-cancel action as the only operator
  mutation, while new copy, attestation, and source eviction remain fail-closed. No Finder/provider
  process, CloudDocs database, source file, or cloud object was changed.

## 2026-08-24 14:11 +0900 live iCloud probe confirms worsening backlog

- The exact-head `disksage-icloud-sync-health` binary completed another read-only CloudDocs/WAL
  snapshot with `evidence_complete=true`, `new_copy_admission_state=blocked`,
  `provider_sync_attested=false`, `local_eviction_authorized=false`, and
  `mutation_performed=false`.
- The aggregate provider state still has 343 uploads blocked on sync-up and one active upload at
  95.24%; File Provider pending indexable items increased from 74,946 to 103,013, with one active
  download and disk-import/transfer activity still present. This is provider reconciliation
  evidence, not proof that any Finder item reached the cloud.
- The target remained 14 entries, 512 bytes, and mtime `2026-08-20 03:28:07 +0900`; `/` retained
  about 94 GiB available. DiskSage therefore continues to block new copy, attestation, and source
  eviction. The only available operator mutation remains the explicit Finder-cancel action.

## 2026-08-24 14:27 +0900 exact-head PR audit

The following live heads were re-queried before this documentation update; predecessor reviews and
checks are not reused:

- DiskSage #189 is open/non-draft at `288904ff8b81d769847869f7b434065d7613b1d7` after absorbing
  current `main`; required checks are queued/in progress, with no unresolved review threads.
- DiskSage #212 is open/non-draft at `779afa48cc8bc534a6e5cc910714324d85f7358b`; its OAuth help
  contract and dead-entrypoint fixes are pushed, required checks are queued/in progress, and all
  current review threads are resolved.
- DiskSage #238 is merged at `d44b23bdf4108bf6b6f6378f7e0ac305187deec6`; it is no longer an open
  merge candidate.
- DiskSage #247 is draft/open at `c9ac3b2041cc6736fb60fde773c3c6fbd21fcdc2`; the latest iCloud
  evidence/ADR update is pushed, required checks are queued, and no review thread is unresolved.
- DiskSage #249 remains draft/open at `44390608d30417477f6a66601b18a53ca87b0a9c`; its previous
  Strix failure remains a provider-gate issue and is not treated as a product merge approval.
- Central `.github` #1263 is open/non-draft at `14cd0e8438b6d670a0f036d1e47f35bd4c3f97a7`; the
  cross-repository documentation reference is qualified, but protected checks/reviews are pending.
  No merge is inferred from queued checks or bot comments.

## 2026-08-24 14:31 +0900 live iCloud probe confirms copy-preparation stall

- A fresh exact-head `disksage-icloud-sync-health` read-only probe again reported
  `evidence_complete=true`, `new_copy_admission_state=blocked`, `provider_sync_attested=false`,
  `local_eviction_authorized=false`, and `mutation_performed=false`.
- The aggregate state remained 343 uploads blocked on sync-up with one active upload at 95.24%,
  one active download, and File Provider pending indexable items increased to 110,652. Native
  status still reported `client_state=needs-sync` with sync-up/down pending; filename/root
  exclusions and disk-import/transfer activity remained present.
- The root volume had about 83 GiB available (13% used), and a bounded `lsof` sample found no
  handle on `real_datasets`. The Finder “preparing to copy” dialog is therefore a provider
  reconciliation/indexing stall, not local disk exhaustion or evidence of a completed cloud
  copy. DiskSage keeps copy, attestation, and source eviction fail-closed; only the explicit
  bounded Finder-cancel action is available to the operator.

## 2026-08-24 14:38 +0900 exact-head queue refresh

- DiskSage #189 advanced to `8809e6cdc8da14915a9e0219481f75a1faebfdb9` after absorbing current
  `main`; it is open/non-draft with required checks pending and no unresolved review threads.
- DiskSage #212 remains open/non-draft at `779afa48cc8bc534a6e5cc910714324d85f7358b`; checks are
  pending and no qualifying approval is present.
- DiskSage #247 is draft/open at `16f511f8af8320ccd885c06c1de60ad00dfbbf12`; the current iCloud
  evidence update is pushed, checks are re-running, and no review thread is unresolved.
- DiskSage #249 remains draft/open at `44390608d30417477f6a66601b18a53ca87b0a9c`; its prior
  provider-gated Strix result is not merge evidence. Central `.github` #1263 has advanced to
  `7011fee275eaa257ce491efb4812dd3e98ed649e` and remains blocked with changes requested.
  No merge is inferred from queued checks or bot comments.

## 2026-08-24 14:40 +0900 exact-head queue refresh

- DiskSage #247 advanced to `618acff21b78ba93a40a7c0d48b99961ba79f4dc` with an additional public
  plan regression for folded mail headers; the preceding destination-headroom test and iCloud
  evidence remain in the exact ancestry. Checks are re-running and the draft remains blocked.
- The other live references are unchanged: #189 `8809e6cdc8da14915a9e0219481f75a1faebfdb9`,
  #212 `779afa48cc8bc534a6e5cc910714324d85f7358b`, #249
  `44390608d30417477f6a66601b18a53ca87b0a9c`, and central `.github` #1263
  `7011fee275eaa257ce491efb4812dd3e98ed649e`. No protected merge is inferred from pending
  checks, historical reviews, or bot comments.

## 2026-08-24 14:45 +0900 exact-head product queue refresh

- DiskSage #247 is now exact head `f23539684651e9280962271759841f9d0fdd377a`, a draft/open
  provider-indexing follow-up that also contains the folded-header and destination-headroom
  regressions plus the standards-safe UI label convergence. Local frontend checks passed on this
  tree; hosted checks are pending and no review thread is unresolved.
- The next product gaps are visible in the live queue: #246 `9cf11c0194aece52a2769b9d10b8f20b7d2658e5`
  (accessible Storybook UX contracts) and #244 `b9941295ac354bb63cf911a064a1f4df1f8eb60b`
  (Rust 1.97.1 baseline) are draft/open; #189 remains `8809e6c`, and #212 remains `779afa4`.
  Central `.github` #1263 remains `7011fee` with changes requested. Protected merge is not inferred
  from draft status, queued checks, or historical approvals.

## 2026-08-24 14:47 +0900 exact-head regression repair

- A local exact-head run initially exposed `source-snapshot-stale` in #247's destination-headroom
  test because its fixture used sentinel timestamps (`created_ms=1`, `modified_ms=1`) for a file
  that the public planner revalidates. The test—not the destination-authority implementation—was
  stale. Head `e9a3fd8` now binds the fixture bytes/mtime to the materialized source and uses the
  observed clock.
- Pinned Rust 1.97.1 execution now passes both runtime regressions: destination ancestor headroom
  authority and folded mail-header planning (2 passed). This preserves the real source freshness
  gate while testing the intended symlinked-staging safety behavior.

## 2026-08-24 14:55 +0900 live iCloud recheck

- The bounded `disksage-icloud-sync-health` probe completed with `evidence_complete=true` and
  `new_copy_admission_state=blocked`: 343 uploads remain blocked on sync-up, one upload is active
  at 95.24%, one download is active, and File Provider pending indexable items reached 121,859.
- The root volume has 66 GiB available; `real_datasets` still has 14 entries and 512 bytes with
  its 2026-08-20 mtime, and the bounded `lsof` sample has no handle on that directory. This
  confirms provider reconciliation/indexing stall evidence rather than disk exhaustion or a
  Finder copy receipt. Copy, per-item attestation, and source eviction remain fail-closed.

## 2026-08-24 15:34 +0900 iCloud queue continues to grow

- The next bounded read-only receipt still reported `evidence_complete=true` and
  `new_copy_admission_state=blocked`: 343 uploads remained blocked on sync-up, one upload stayed
  active at 95.24%, and one download stayed active. File Provider pending indexable items grew
  from 121,859 to 128,917; disk import, transfer activity, and the 28 filename/2 root exclusions
  remained present. Native status remained `client_state=needs-sync` with sync-up pending.
- This is provider-global reconciliation evidence, not a per-item receipt for the seven
  `real_datasets` entries and not proof of a cloud write. DiskSage keeps Goal
  `provider-sync-incomplete`, copy/attestation/source eviction fail-closed, and performed no
  Finder, provider, source, or cloud mutation.

## 2026-08-24 15:46 +0900 iCloud indexing backlog continues to rise

- A fresh bounded read-only probe reported `evidence_complete=true` and
  `new_copy_admission_state=blocked`: 343 uploads remain blocked on sync-up, one upload remains
  active at 95.24%, and one download remains active. File Provider pending indexable items reached
  130,571, with disk import/transfer activity and the 28 filename/2 root exclusions still present;
  native status remains `client_state=needs-sync` with sync-up pending.
- This is aggregate provider reconciliation evidence that explains the Finder “preparing to copy”
  symptom but does not identify the seven items or prove remote upload. DiskSage keeps
  `provider-sync-incomplete`, copy/attestation/source eviction fail-closed, and performs no
  Finder, provider, source, or cloud mutation.

## 2026-08-24 15:42 +0900 Strix provider evidence separated from source readiness

- The exact-head central `.github` PR #1263 Strix artifact (`32693700056`) recorded NVIDIA NIM
  HTTP 429 rate limiting on the primary and retries, followed by a direct OpenAI fallback HTTP
  404 for `openai-direct/gpt-5.6-luna`. The gate correctly retained the provider-failure signal
  and did not promote the fallback's zero-finding report to a successful security result.
- This is external model-provider availability evidence, not proof of a DiskSage source defect or
  a cloud/data mutation. The central repair remains subject to a fresh authoritative same-head
  Strix run and protected approvals; DiskSage's local iCloud admission and eviction gates are
  unaffected and remain fail-closed.

## 2026-08-24 16:03 +0900 exact-head review queue refresh

- DiskSage #227 is ready for review at `fd841e9e6b76dc2d47d62d2fddabe53eecf544b2`; its current
  review threads are resolved, macOS bound-root passed, and the remaining hosted checks plus the
  two independent protected approvals are still required.
- DiskSage #247 is ready for review at `59057c08eb5017ac57b640419a0c7e4779f443d7`; the iCloud
  indexing evidence and customer-facing admission messages are in the exact ancestry. Checks are
  queued and no protected approval is present.
- DiskSage #246 is ready for review at `308be49b56d1c38fbe9a5c00ab46ac2b3e51df73`; frontend
  accessibility/Storybook checks were locally verified, while its hosted Strix result remains an
  external provider gate that must be freshly revalidated. #244 remains open/non-draft with its
  pinned Rust baseline checks queued. No merge is inferred from readiness or queued checks.

## 2026-08-24 16:03 +0900 iCloud health recheck

- The bounded probe remains fail-closed: `evidence_complete=true`, `new_copy_admission_state=blocked`,
  343 sync-up items blocked, one upload at 95.24%, one download active, and native
  `client_state=needs-sync`/`needs-sync-up`.
- File Provider pending indexable items reached 131,214; disk import, transfer activity, and the
  28 filename/2 root exclusions remain. This is aggregate reconciliation evidence, not proof that
  the seven Finder items were uploaded. No provider, Finder, source, or cloud mutation occurred.

## 2026-08-24 16:08 +0900 review metadata and exact-head repair

- DiskSage #244 keeps exact head `13caeb04333e50e57c8a51a11b64aeb131c080b2` with all review threads
  resolved. Its PR description now matches the supported Dependabot configuration and records the
  local pinned Rust documentation-test evidence without claiming a full hosted pass; checks and
  protected approvals remain pending.
- DiskSage #227 advanced to `bf62ea0d74f077add672d0a193de154bde910b97` with a platform-specific
  test-warning cleanup; its prior bound-root test passed 4/4 locally and hosted checks restarted.
- DiskSage #247 remains ready for review at `1535320c2b8b288376d9dcd35485a2af58374873`; its latest
  iCloud evidence is exact-head and all copy/attestation/eviction mutations remain disabled.

## 2026-08-24 16:12 +0900 CLI review repair

- DiskSage #212 advanced to exact head `81c44e43205f21276c39f055f1878805f36e1072` and is ready for
  review. Its mixed help-plus-invalid CLI test now preserves HOME so it exercises argument parsing,
  while standalone help remains environment-independent; the targeted cloud-cli test passed 2/2.
- The provider OAuth environment contract remains intentionally in the default test matrix, and
  its informational review thread was resolved without adding cloud credentials or side effects.

## 2026-08-24 16:14 +0900 worktree-audit queue status

- DiskSage #249 is now ready for review at `44390608d30417477f6a66601b18a53ca87b0a9c`; its
  non-Strix checks passed in the last exact-head run, while Strix remains an external provider
  availability failure requiring a fresh authoritative run.
- The PR is not merge-ready until that provider gate, current coverage, and protected review quorum
  are satisfied. No worktree, source, provider, or cloud mutation was performed by this status
  update.

## 2026-08-24 16:14 +0900 Homebrew execution stack review status

- DiskSage #205 (Intel Homebrew executable admission) is ready at `5c86668a6e503a174ff0b07151f67226b39547ff`; its hosted Test/Release/build checks are green but the stacked base and protected approvals remain.
- DiskSage #206 (content-bound Homebrew execution) is ready at `2e7b845b7610a871ec5981d964bcab5cb99df41d`; GitHub reports clean and hosted Test/Release/build checks are green. No approval bypass or merge was performed.

## 2026-08-24 16:15 +0900 customer-facing UI queue status

- DiskSage #203 (assistive table labels) is ready at `9d573f04145eb4168098623042484fdf73c2ab74`;
  #202 (bounded scan/navigation failure feedback) is ready at
  `1d005586b270ca1fcad445970cf44bf5e7268425`. Both have no unresolved review threads; protected
  checks and approvals remain the merge gates.

## 2026-08-24 16:18 +0900 Homebrew status UI verification

- DiskSage #189 is ready at exact head `66d7aa767d416048a752c5c550e8d64e03213e0e`; the local
  frontend regression slice passed 7/7 (`fmt` and `verdictBadge`), while coverage-source-tree and
  protected approvals remain pending.

## 2026-08-24 16:21 +0900 iCloud Finder copy remains provider-blocked

- The bounded read-only health receipt still reports `evidence_complete=true` and
  `new_copy_admission_state=blocked`: 343 uploads remain blocked on sync-up, one upload remains
  active at 95.24%, and one download remains active. File Provider pending indexable items reached
  132,783; disk-import/transfer activity and the 28 filename/2 root exclusions remain, and native
  status remains `client_state=needs-sync` with sync-up pending.
- This explains a multi-hour Finder “preparing to copy” symptom as provider-global reconciliation
  pressure, but does not prove that DiskSage itself holds a Finder lock, identify the seven items,
  or prove a cloud write. The product keeps `provider-sync-incomplete`, copy/attestation/source
  eviction fail-closed and performs no Finder, provider, source, or cloud mutation.

## 2026-08-24 16:32 +0900 exact-head review repairs

- DiskSage #246 advanced to `1972614`; its coverage configuration now keeps the
  `node_navigation` dead-code allowance without duplicating the attribute on
  `preferred_scan_roots`. The pinned Rust 1.97.1 navigation slice passed 6/6 and the Devin thread
  is resolved.
- DiskSage #227 advanced to `5ad1197`; the bound-root audit parameter now says `stable_root`, and
  the intentional `duplicate_audit::bound_read_root` module contract was documented. The pinned
  Rust 1.97.1 duplicate-audit slice passed 10/10 and both current informational threads are
  resolved. Hosted checks and protected approvals still gate merge.

## 2026-08-24 16:47 +0900 Git-worktree test artifact cleanup

- DiskSage #249 advanced to exact head `db95c54` and is ready for review. Its three feature-gated
  CLI integration tests now reuse deterministic private Cargo target directories and remove stale
  output before nested builds, closing the repeated-test disk accumulation gap. The affected test
  targets compile under pinned Rust 1.97.1; the help process slice passed 8/8 before this cleanup.
- The metadata-failure diagnostic remains a bounded generic fallback by design; it does not expose
  paths or weaken the fail-closed private-report contract. Current hosted checks and protected
  approvals remain required.

## 2026-08-24 16:50 +0900 exact-head queue refresh

- #203 is ready at `5f0bd51` with the current TopFiles accessibility contract; #244 is ready at
  `13caeb0`; and #249 is ready at `db95c54` after the test-artifact cleanup. Their review threads
  are resolved where applicable, but current hosted checks and the protected independent-approval
  quorum remain merge gates. No merge or approval bypass was performed.

## 2026-08-24 16:50 +0900 iCloud backlog remains the active customer blocker

- The bounded probe now reports File Provider pending indexable items at 135,334, up from 132,783
  at 16:21; 343 uploads remain blocked on sync-up, one upload remains active at 95.24%, and one
  download remains active. Native status remains `client_state=needs-sync` with sync-up pending and
  `new_copy_admission_state=blocked`.
- The growing queue is consistent with Finder’s multi-hour “preparing to copy” state, but still
  does not prove DiskSage holds a Finder lock or identify the seven items. No provider, source,
  Finder, or cloud mutation was performed, and local eviction remains fail-closed.

## 2026-08-24 16:55 +0900 concurrent test-target repair

- #249 advanced to exact head `aa5c37d`; its shared test helper now uses process-scoped Cargo target
  directories and prunes stale outputs without deleting another active run. The affected targets
  compile under pinned Rust 1.97.1, and the current hosted checks have restarted for this head.

## 2026-08-24 17:00 +0900 live brctl confirmation of the Finder stall

- A fresh read-only `/usr/bin/brctl status` completed at 17:00. The iCloud container still reports
  `client:needs-sync` and `sync:needs-sync-up|in-sync-down|prefer-sync-down|oob-sync-ack`; the
  dump contains 1,740 `pending-scan` entries, 343 `pending-sync-up` entries, 1,807 scheduled
  sync-up markers, and 5 upload errors. Individual queued uploads last ran roughly 60–66 hours
  ago, including `CKErrorDomain:4` / “Saving asset failed” records.
- This is stronger provider-global evidence for the screenshot's multi-hour `real_datasets`
  “복사 준비 중” state, but it still cannot identify the seven Finder items or prove a cloud
  write. DiskSage performed no Finder/provider/source/cloud mutation; `provider-sync-incomplete`,
  copy/attestation, and local-eviction gates remain fail-closed. The root volume currently has
  about 36 GiB available, so the live blocker is provider reconciliation/error backlog rather
  than a full root volume.
- At 17:02, the read-only process inventory showed Finder (PID 1422), `fileproviderd` (1450), and
  `bird` (1462) all started at 10:43:49, about 6h18m earlier. This confirms a long-lived provider
  session, not that DiskSage owns or has locked the Finder operation.
- Exact-head review evidence remains current: #249 is now `6b95c59` after centralizing the CLI's
  reference validation in the library; #246 is `1972614`; #227 is `5ad1197`. Hosted checks and
  protected independent approvals remain the only merge gates.

## 2026-08-24 19:04 +0900 live iCloud queue still explains the copy-preparation stall

- A fresh read-only `/usr/bin/brctl status` still reports the iCloud client as `needs-sync` with
  `needs-sync-up|in-sync-down|prefer-sync-down|oob-sync-ack`. The bounded dump contains 1,740
  `pending-scan` entries and 343 `pending-sync-up` entries; the queue remains provider-global and
  does not identify the seven Finder items.
- The host has about 12 GiB available on `/`, and the read-only process inventory contains Finder,
  `fileproviderd`, and `bird` but no DiskSage process. This is consistent with provider
  reconciliation/indexing pressure; it is not proof that DiskSage owns a Finder lock, nor proof
  that the cloud write completed. Per-item copy, attestation, and local eviction remain
  fail-closed; no Finder, provider, source, or cloud mutation occurred.

## 2026-08-24 19:10 +0900 exact-head iCloud health receipt

- The exact-head `disksage-icloud-sync-health` probe completed read-only with
  `evidence_complete=true`, `new_copy_admission_state=blocked`, and
  `pending_indexable_count=151283`; one upload is active at 95.24% and one download is active.
  The report retains `provider_sync_attested=false`, `local_eviction_authorized=false`, and
  `mutation_performed=false`.
- The blockers include native sync-up pending, 343 uploads blocked on sync-up, File Provider
  indexing/disk-import/transfer activity, and filename/root exclusions. This is still aggregate
  provider evidence rather than a per-item receipt for the seven Finder entries; no cloud write or
  source eviction is authorized.

## 2026-08-24 19:28 +0900 exact-head Git worktree audit repair

- DiskSage #249 advanced to exact head `c8ca669262f913de5719ebda377132f1135c06c8`. The hosted
  all-features compile failure was traced to CLI tests referencing the library's private
  `MAX_REFERENCE_BYTES` bound; the bound is now exported once by the library and imported only by
  the CLI test module. Pinned Rust 1.97.1 local proofs passed 7/7 CLI tests and 10/10 black-box
  Git-worktree tests.
- The audit remains read-only, path-redacted, create-once for private evidence, and grants no
  worktree-removal authority. The new exact head has no failed checks yet; hosted checks and
  protected approvals remain authoritative. No user, Finder, provider, or cloud data was changed.

## 2026-08-24 19:38 +0900 post-recovery iCloud recheck

- A fresh read-only `/usr/bin/brctl status` still reports `client:needs-sync` and
  `needs-sync-up|in-sync-down|prefer-sync-down|oob-sync-ack`; native last-sync remains
  `2026-08-21 20:20:10.166`.
- Finder, `fileproviderd`, and `bird` are present with no DiskSage process; `/` has about 12 GiB
  available. This remains aggregate provider reconciliation evidence for the Finder
  `real_datasets` preparation stall, not proof of a DiskSage lock or a completed cloud write.
  Copy admission, attestation, and source eviction remain fail-closed; no mutation was performed.

## 2026-08-24 19:52 +0900 path-free lineage handoff proof

- The buyer-visible P1 lineage gap now has a minimal export path: CloudArchive can download a
  `disksage.cloud-lineage` JSON graph from a verified modern receipt, connecting source,
  metadata, archive, provider, receipt, Goal, and (only after actual eviction) eviction nodes.
- The export includes stable content IDs, production metadata source/confidence, provider sync
  state, and blockers, but no raw local or destination paths. Legacy receipts without a lineage
  fingerprint fail closed. Frontend `npm run check`, all 137 frontend tests, and 100% V8
  statements/branches/functions/lines pass; the export itself is read-only and does not change
  provider, source, cloud, ADR, or Goal state.

- When remote content proof exists, the graph additionally binds a path-free provider-item node to
  the provider and receipt; without it, no provider item is inferred from a local File Provider
  path.

## 2026-08-24 20:00 +0900 live Finder preparation diagnosis

- The latest read-only `brctl status` still reports iCloud `client:needs-sync` with
  `needs-sync-up|needs-sync-down|in-sync-down|prefer-sync-down|oob-sync-ack`; native last-sync
  remains `2026-08-21 20:20:10.166`. Finder, `fileproviderd`, and `bird` are running, while no
  DiskSage process is present. The persistent `pending-scan` queue and absent per-item receipt
  keep the seven-item `real_datasets` operation at `provider-sync-incomplete`; no mutation was
  performed.

## 2026-08-24 20:18 +0900 candidate-scoped local headroom

- The native-copy plan now records `local-volume-headroom-insufficient` or
  `local-volume-headroom-unverified` on the individual candidate that failed its destination
  filesystem probe. The UI retains the aggregate-notice fallback for older reports, so one large
  file no longer disables smaller candidates that independently fit. Rust library tests passed
  741/741 (one live-provider test ignored) and the frontend passed 138/138 with 100% V8 coverage.

- The replaceable Goal now explicitly carries `provider-sync-incomplete` and
  `destination-headroom-bound`, so the persistent iCloud blocker and per-candidate staging gate
  survive projection/restart without being reduced to an ambiguous pending label.

## 2026-08-24 21:00 +0900 native iCloud pending-scan detection

- The live `brctl status` evidence contains repeated `apply{[ pending-scan ... ]}` entries, but the
  prior native-status schema did not expose that count to the admission report. DiskSage now
  records the bounded, path-free `pending_scan_count`, emits
  `icloud-native-status-pending-scan`, propagates it through Naruon readiness, and shows it in the
  CloudArchive UI. This keeps the Finder `real_datasets` “복사 준비 중” state explicitly blocked
  without treating the screenshot as an upload receipt; no Finder, provider, source, or cloud
mutation is performed.

## 2026-08-25 00:00 +0900 dynamic Goal/ADR propagation

- The previous native pending-scan implementation stopped at health/readiness/UI and did not update
  receipt-linked runtime projections. The iCloud health persistence path now applies the selected
  blocker to bounded valid iCloud receipt projections: Goal becomes `blocked` and revokes provider
  completion and eviction gates; the paired ADR records the provider-state blocker. Projection
  failures remain explicit and path-free, and no provider/Finder/source/cloud mutation occurs.

## 2026-08-25 09:23 +0900 current Google Drive Finder-preparation diagnosis

- The screenshot's destination is Google Drive, not iCloud. A bounded read-only
  `fileproviderctl dump com.google.drivefs.fpext -l` reported `temporarily disconnected`, File
  Provider `-1004` server-unreachable root metadata failures, active upload and download progress,
  and a 2,000-entry reconciliation backlog. This is the provider-global explanation for Finder
  remaining at “복사 준비 중”; it is not a per-item cloud receipt or proof of a completed copy.
- The 7.2 GiB `real_datasets` source remained local and unchanged, no destination folder or receipt
  was observed, and the root volume had about 2.1 GiB free. DiskSage retains the existing stable
  provider-global blockers and refuses copy, attestation, and source eviction until a fresh quiet
  provider observation. No Finder, provider, source, or cloud mutation was performed.
- The exact DiskSage PR #247 head is
  `9fdf2922da2939d96d3c2393539f2b2d42009929`; its hosted checks are still pending and the protected
  PR remains draft/blocked/review-required. The host's `utun4` default route is recorded only as
  context, not as a proven root cause. The filename dates `2026-04-28` and `251210` remain
auxiliary production-time evidence; embedded metadata and context retain precedence.

## 2026-08-25 09:30 +0900 third-party provider blocker projection

- Provider-global sync persistence now applies the existing monotonic ADR/Goal projection contract
  to OneDrive and Google Drive. A blocked provider observation sets the linked Goal to `blocked`,
  revokes provider-sync and eviction gates, and records the stable blocker in the paired ADR;
  clear observations do not rewrite state.
- Reclaiming only disposable DiskSage Rust build artifacts increased root free space to about 3.5
  GiB, but the same Google Drive dump still reported `temporarily disconnected`, File Provider
  `-1004`, active transfer markers, and 2,000 reconciliation entries. This confirms the current
  stall remains provider-global rather than proven local fullness. No Finder/provider/source/cloud
  mutation was performed.
- The exact DiskSage PR #247 head is `87c9089bcd4af49f8f8751c54ebcc45b519d1f0c`; hosted checks are
  pending and the protected PR remains draft/blocked/review-required. Filename dates
  `2026-04-28` and `251210` remain auxiliary production-time evidence only.

## 2026-08-25 10:14 +0900 data-volume headroom and iCloud backlog recheck

- `/Users` is the source/File Provider staging volume; it had about 594 MiB available before
  disposable build-artifact cleanup and about 2.7 GiB after cleanup, while `real_datasets` is about
  7.2 GiB. The system-root `df` value is not a valid staging-volume authority.
- iCloud File Provider reported `pending-indexable-count: 490195`, upload/download progress entries
  stuck at `0.0000`, and a 482,470-entry reconciliation section. The Finder preparation operation therefore remains
  `provider-sync-incomplete`; it is not treated as a cloud receipt or completed upload.
- The Rust preview adapter now keeps unverified destination-ancestor results as diagnostics while
  retaining candidate-specific insufficient-headroom blockers. Mutation-time destination probing
  remains authoritative. Exact PR #247 head: `5c3b87359103b82df3efb4099668b1b17f532259`; hosted
  checks are queued and the protected PR remains draft/blocked/review-required. No Finder,
  provider, source, cloud, or eviction mutation was performed.

## 2026-08-25 10:18 +0900 repeated zero-progress iCloud receipt

- Two read-only iCloud probes 19 seconds apart increased `pending-indexable-count` from `492224` to
  `492507` and reconciliation from `484500` to `484783`, while upload/download markers remained at
  `Fraction completed: 0.0000`.
- No standalone `cp`, `ditto`, or `rsync` process was present. The visible Finder preparation
  window is therefore provider coordination evidence, not proof of a DiskSage copy worker or a
  completed cloud write. Goal remains `provider-sync-incomplete`; copy, attestation, cloud-write,
  and source-eviction gates stay closed. No cancellation or provider/source/cloud mutation was
  performed.

## 2026-08-25 11:00 +0900 deterministic headroom regression proof

- The preview adapter's candidate-scoped behavior is unchanged. Its regression fixture now uses an
  intentionally unfit candidate size, preventing the test from accidentally treating the host
  runner's root filesystem as verified capacity when the synthetic destination has no existing
  ancestor.
- Pinned Rust 1.97.1 ran 745 library tests with one live-provider test ignored; the exact head is
  `dc57a1539b82514f4ceb17ec0fca42ed23ae7988`. This is test evidence only and grants no cloud-write,
  attestation, source-eviction, Finder-cancel, or provider-restart authority.

## 2026-08-25 11:20 +0900 exact-head queue handoff

- DiskSage PR #247 is now `8ad12e1e5b57944960b5389e4d2067f3fcd0e924`; its new hosted test, Strix,
  Noema, and queue checks are pending. It remains draft, blocked, and review-required; the local
  Rust proof above is not a substitute for hosted exact-head evidence or protected approval.
- DiskSage PR #249 remains at `2f1d585398b85f3f1adb3783520ad70e7b4a9c3f`; the stale Strix failure was
  explicitly rerun against the same head after central `.github#1318` moved the smoke contract to
  main. Other substantive checks are green, but the rerun and protected reviews are pending.
- Central `.github#1318` merged as `8fd471a31399a914d9cb22a840f4a4c68e010ea6`; `.github#1316` is
  based on that head at `e4f9865a1b06978324f006ee3861b84953877d8b` and carries the remaining direct
  OpenCode model-pool alignment. No merge or approval is inferred from queued checks or bot reviews.

## 2026-08-25 11:06 +0900 current Finder “복사 준비 중” receipt

- A fresh bounded read-only File Provider dump reports Google Drive as `temporarily disconnected`;
  the root metadata request returns File Provider `-1004` (server unreachable), the reconciliation
  queue is capped at 2,000 entries, and the latest user-initiated root retry is approximately
  57 minutes old. Upload/download markers are present, but there is no per-item destination receipt.
- iCloud is also backlogged: `pending-indexable-count` is 505,103, upload/download progress is
  `0.0000`, `disk import` is active, and reconciliation contains 497,379 entries. The data volume
  has approximately 20 GiB free at this observation, so the Finder wait is provider coordination,
  not proof that the local volume is full.
- DiskSage must display this as `provider-sync-incomplete` and keep cloud write, attestation, source
  eviction, provider restart, and Finder cancellation blocked. The screenshot is not a cloud receipt;
  no Finder, provider, source, cloud, or eviction mutation was performed. Filename dates remain
  auxiliary evidence only; embedded metadata and context retain precedence.

## 2026-08-25 11:09 +0900 persistent provider stall recheck

- The next bounded read-only probe still finds Google Drive temporarily disconnected with File
  Provider `-1004`, a 2,000-entry reconciliation cap, and active upload/download markers. iCloud
  grew to `pending-indexable-count` 506,044 and 498,320 reconciliation entries while both transfer
  fractions remain `0.0000`; disk import and stream reset remain active.
- The data volume remains approximately 20 GiB free. This is persistent provider coordination,
  not evidence that the Finder dialog completed or that the local volume is full. DiskSage keeps
  `provider-sync-incomplete`, cloud write, attestation, source eviction, provider restart, and
  Finder cancellation blocked.
- Current exact-head queue evidence: PR #247 `8e98b74e` (draft/blocked/review-required; hosted
  checks pending), PR #246 `1972614e` (draft/blocked/review-required; prior Strix HTTP 429/404
  infrastructure failure rerun requested), PR #249 `2f1d585` (draft/blocked/review-required;
  Strix rerun pending), and central `.github#1316` `e4f9865a` (blocked with no qualifying approval;
  required checks pending). No merge is inferred.

## 2026-08-25 11:24 +0900 provider-indexing Finder action gap closed

- Provider-global indexing-only stalls now expose the same bounded Finder-cancel action as transfer
  and reconciliation stalls. This covers OneDrive/Google Drive reports of
  `provider-global-sync-indexing-pending` without treating the provider dump as a copy receipt.
- Svelte type-check and the focused CloudArchive admission/timing tests passed. The exact PR #247
  head is `dda0f1d5`; its hosted checks restart on the documentation head. No automatic Finder
  cancellation, provider restart, cloud write, source mutation, or eviction was performed.
