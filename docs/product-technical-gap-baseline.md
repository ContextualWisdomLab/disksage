# DiskSage product and technical gap baseline

**Snapshot:** 2026-08-28 (Asia/Seoul)
**Repository heads at snapshot:** `main` `79067c1160ddedf7fc962cbf8067ce7e83c4564a`, PR #267
`3630e1eefacbeb996e6176373e6010da93bfa16c`, PR #263
`060358340e922db7c36b6303dd0a959007a878c5`, and the current open queue (41 PRs: 20 ready, 21
draft); hosted checks and protected review remain authoritative, and no merge is claimed from
queued or stale status.
**Product boundary:** local-first macOS disk pressure relief with iCloud, OneDrive, and Google Drive destinations.
**Evidence rule:** this document is a dated baseline, not an authority for transfer or deletion. Runtime receipts, provider attestations, object identity, and current GitHub checks remain authoritative.

## 2026-08-29 OneDrive local-space recovery observation

- Production use of Foundation ubiquitous-item eviction on anonymized OneDrive files retained
  uploaded cloud items and reduced local allocation from about 83.46 GB to 21.13 GB; host APFS
  availability increased by about 61 GiB. DiskSage now admits that path only for a bounded regular
  file whose fresh File Provider item-and-version fingerprint, upload/current/conflict/policy
  flags, active-use probe, and exact approval all match immediately before execution. A dataless
  apparent 10 GB file correctly contributes zero reclaim. Mixed folders, active readers, changed
  versions, and stale approvals remain blocked.

- A metadata-only inventory completed across 277,410 entries and found 118 locally materialized
  candidates totaling 62,060,163,072 allocated bytes. A fresh plan after provider drift retained
  110 eligible items totaling 58,836,140,032 allocated bytes and excluded eight items; private
  paths remain outside this document.
- The first exact-item execution performed no eviction because the desktop client would not finish
  its bounded quit sequence. DiskSage now keeps copy/upload admission separate from local-only
  Files On-Demand eviction, observes the primary app separately from its resident File Provider
  helper, and permits only one bounded graceful `SIGTERM` fallback. It never force-kills the sync
  client, deletes a cloud item, or claims reclaimed bytes without post-action allocation proof.
- The vendor `/unpin` command and cross-provider `NSFileProviderManager` identity probe remain
  rejected boundaries: their earlier failures cannot prove eviction. The subsequent Foundation
  FileManager observations supply the missing execution and post-allocation evidence without using
  either vendor-private commands or cloud-object deletion. Finder remains a customer-controlled
  fallback when a freshly approved native request fails.

## 2026-08-28 explicit open-PR worktree cutoff observation

- PR #267 observed head `4b6dc492926a48aa0f29e867316177de31c92f4d` adds an opt-in calendar cutoff
  for same-repository open pull requests. The plan and every removal re-query GitHub state,
  creation time, branch, and exact head OID; the authority fingerprint binds that evidence and the
  operator cutoff. Branches and commits remain preserved, and no implicit age or filesystem-time
  threshold is used.
- Local frontend evidence for that head is green (`npm test -- --run`: 39 files/166 tests;
  `npm run check`: zero errors/warnings). Hosted Rust, coverage, security, and review gates remain
  the authority for integration and merge readiness.
- The hosted Rust test exposed and the next head repaired a temporary-JSON borrow error in the
  Colima runtime-state parser; the fix retains only a validated boolean and state-present flag and
  does not relax the unavailable-state blocker.

## 2026-08-28 container cleanup loop evidence

- On Podman 5.8.2 (the local `docker` wrapper), the live inventory contained two running
  containers (`buildx_buildkit_default` and `accounting-information-platform-test-postgres`), one
  default `podman` network, nine local volumes, and no stopped containers. The running Postgres
  container is mounted on the previously anonymous volume
  `bed31c452be785c238f9cb4c53cb04bc85e3233f0f05450327161c996778a349`; that volume was retained.
- `docker container prune --force`, `docker network prune --force`, `docker image prune --force`,
  `podman image prune --all --force`, and `podman system prune --all --force` removed zero items
  (`Total reclaimed space: 0B`). All seven detached, named compose volumes remain protected as
  data-bearing project stores; BuildKit state remains attached and protected. No source, provider
  database, or cloud object was deleted.
- The local Rust CLI probe was stopped before completion because its fresh Cargo target consumed
  emergency headroom. `cargo clean --manifest-path src-tauri/Cargo.toml` removed the generated
  target; the latest APFS observation was about 4.3 GiB available. This is volatile host evidence,
  not a deletion guarantee.
- Follow-up re-audit found a short-lived `psychometrics-commons-pr427-coverage-20260828`
  PostgreSQL container running with the anonymous volume
  `0208f7d42ddb6bb800a6cda08e3d93b7aed3ac39d8f64718c841e02e44233878`; direct Podman inspection
  shows the volume mounted at `/var/lib/postgresql`, so it remains protected while the container
  is active. The host then had three running containers, one default `podman` network, and ten
  local volumes; no new stale network or image was proven removable.
- The latest read-only Podman inventory could not connect because the machine SSH handshake
  returned EOF. DiskSage therefore did not start, initialize, prune, or remove any runtime
  resource; the host had about 1.3 GiB available at that observation. The failed connection is a
  runtime-availability blocker, not evidence that any volume, image, or network is stale.

## Current product contract

1. Scan and metadata profiling are read-only and metadata-first: embedded metadata precedes an unambiguous filename token, then filesystem creation/modification time. A filename token such as `2026-04-28` or `251210` is secondary evidence and never proves ownership, upload, or eviction authority.
2. A cloud candidate follows `copy-verified → pending-provider-sync → provider-sync-confirmed → eviction-ready → source-evicted`. `local-current` with `is_uploaded=false` is `pending-upload`; no eviction permit is issued.
3. Native File Provider copy is bounded, re-hashed, and source-identity rechecked. Provider-global timeout, quota/auth uncertainty, local headroom shortage, stale worktree metadata, or incomplete metadata fail closed.
4. Regenerable caches are a separate reclaim domain. They are per-child, identity-bound, active-use checked, journaled, and moved to OS Trash; they are not uploaded as user data.
5. Deterministic Rust gates own safety. A local model may judge only the fixed maintenance command after dry-run evidence, calibration, and explicit human confirmation. No external LLM or OAuth service is a runtime prerequisite for the standalone product.

## Reclaim-domain contract

| Domain | What DiskSage may propose | Required proof before mutation | Explicitly out of scope |
| --- | --- | --- | --- |
| Cloud/local duplicate | Copy or adopt an already-present cloud object, then evict only the local copy | Provider item identity, content digest, sync attestation, current local identity, and fresh headroom | Deleting a local placeholder or treating `is_uploaded=false` as uploaded |
| Duplicate photos/files | Keep one user-selected member and move the others to Trash | Exact copies require stable content identity; non-identical candidates require DCT perceptual evidence, measured dimensions/bit depth/compression preservation, fresh source identity, and one explicit survivor per group | Automatic perceptual-duplicate deletion or “best quality” guessed from names |
| Podman/Docker | Remove only stopped, unreferenced resources proven by a runtime re-audit | Runtime inventory, reference/label evidence, size evidence, and exact approval | Removing active volumes, BuildKit state, or raw VM images |
| Colima/Podman VM storage | Run bounded guest `fstrim` while the guest is running | Fresh runtime state, fixed command, exact phrase, and bounded output receipt | `qemu-img`, sparse-file truncation, VM stop/delete, or host allocation claims |
| Shared temporary storage (`/tmp` or macOS `/private/tmp`) | Show current-user-owned children and advisory lifecycle evidence for top-level DiskSage artifacts; permanent execution is disabled | Real-directory root, sealed object identity, bounded tree fingerprint, ownership and active-use evidence, and explicit execution-disabled blocker | Same-user marker as producer authentication, age/name-only authority, symlinks, active references, database/worktree data, journal/receipt races, and deleting the shared root or any child |
| Git worktrees | Remove a clean, inactive secondary worktree whose exact head is no longer retained | Fresh Git registration/status/size/open-file evidence and, for PR authority, same-repository state + head OID | Branch deletion, `git prune`, fork worktrees, dirty/active worktrees, or age-only deletion |
| Standalone Git clones | Move a clean, inactive, single-worktree clone on an exact closed or operator-cutoff stale-open PR head to OS Trash | Fresh same-repository GitHub branch + head OID, retained-reference comparison, recursive active-use check, internal Git directory, object identity, and exact approval | Branch deletion, `git prune`, fork/detached/dirty/active clones, external Git directories, or implicit age thresholds |

The dashboard must sum these domains separately. A displayed target such as 300 GB is a
measurement goal, not an authorization: unresolved bytes stay visible with their blocker and are
never converted into a deletion estimate by a heuristic.

## Customer-observable product gaps

| Priority | Gap / observable symptom | Evidence | Acceptance criterion |
| --- | --- | --- | --- |
| P0 | Cloud offload can remain blocked while a provider is syncing or reports `local-current`/`is_uploaded=false`; the user sees no safe reclaim despite free cloud capacity. | Existing provider-global and iCloud native-state gates; `bird`/`fileproviderd` remain active during the current incident, with about 3.8 GiB available at the latest observation. | UI explains the exact blocker, last evidence time, and next bounded retry; a verified provider attestation alone can advance a candidate, never a stale projection. |
| P0 | A long Finder/provider copy can appear hung and consume the remaining local headroom. | The `real_datasets` Finder copy remained at “준비 중” for hours; the latest bounded iCloud dump retained 125 no-progress fetch/create markers, a 95.24% upload, and a zero-progress 1.06GB download while scheduling was `running`. Bounded `/bin/cp`/`mkdir` and global probes use private process groups and headroom gates. | Preview shows required bytes + staging reserve; timeout cleans only the child-created destination and leaves a durable receipt. |
| P1 | Personal desktop-client capacity is not the same as API quota; OAuth is unnecessarily implied for a single-user installation. | ADR-0001 permits copy-only desktop-client mode marked `capacity-unverified`; the cloud connection UI defaults to read-only OAuth consent and requires an explicit write-access opt-in. | Settings clearly distinguish local desktop client, API quota, and organization OAuth; no OAuth prompt is required for the local-only path. |
| P1 | Users cannot yet see a full lineage graph connecting source, metadata, archive member, provider item, receipt, Goal, and eviction decision. | The candidate UI now exposes a compact source→metadata→archive→provider lineage panel using the stable fingerprint, confidence, and blocker state; provider item/receipt/permit remain explicitly pending until their evidence exists. | Export and UI show stable content IDs, provenance edges, confidence, and blockers without exposing raw private paths. |
| P1 | “Orphan”/duplicate cleanup is difficult to trust because relationship evidence is not visible before action. | Exact decoded-pixel groups now reuse the shared reversible quarantine engine with a unique Pareto keeper or explicit customer selection, fresh audit/root/identity checks, Trash, journal, and receipts. Calibrated near-duplicate grouping and a provenance-bound IQA artifact remain separate evidence gaps. | Every proposed removal has an explainable parent/child/duplicate relation, identity recheck, reversible Trash action, and a no-candidate result when evidence is incomplete. Photo cleanup additionally requires a dataset-calibrated descriptor threshold and separately presented quality, lineage, and metadata evidence without a composite score. |
| P2 | Cross-platform behavior and accessibility are not presented as one release contract. | macOS/Linux/Windows release checks exist; several UI accessibility PRs remain open. | Release notes and UI expose platform capability matrix, keyboard/assistive labels, and bounded failure messages for each action. |
| P0 | A 300 GB target cannot be met by cache pruning alone; VM-backed stores and user data need separate measured plans. | Current host observations show only tens of GB in proven regenerable/runtime candidates, while DaisyDisk’s large Application Support/Mobile Documents totals are not deletion authority. | Dashboard reports measured reclaimable bytes by domain, requires provider confirmation before local eviction, and leaves the remainder explicitly unresolved. |
| P1 | Photo copies with different bytes cannot be safely ranked from a filename or an arbitrary quality score. | The perceptual-photo audit now groups distinct-content candidates using Zauner DCT pHash, exact aspect ratio, and the published pHash Hamming bound; it shows resolution, bit depth, format, compression preservation, and an unweighted Pareto rationale. | Completed in source: managed libraries are excluded, every group requires an explicit survivor, execution re-audits identity and moves only non-survivors to OS Trash with receipts, and no automatic or permanent deletion exists. Runtime review against the user's external photo folders remains a separate operational action. |
| P1 | A stale PR worktree may point at a branch that is still open, so age alone is not deletion authority. | The worktree audit already binds same-repository closed/merged PR head OIDs and protects dirty, active, detached, fork, locked, and retained-tip worktrees. | Require an explicit cutoff and fresh same-repository PR state before proposing an old open-PR worktree; preserve the branch/commit and remove only a clean, inactive worktree after exact approval. |
| P1 | A standalone clone on a closed or explicitly old open PR head was invisible because the worktree remover always preserves its primary checkout. | The standalone-clone plan now reuses exact Git/GitHub worktree evidence, requires one clean inactive checkout and an internal Git directory, then revalidates identity before Trash. | App commands return a measured plan and execute only its exact approval; the original path disappears, branch and Git maintenance commands are untouched, and physical reclaim remains pending until Trash is emptied. |

## Technical and operational gaps

| Priority | Gap | Current state | Smallest next proof |
| --- | --- | --- | --- |
| P0 | Provider end-to-end receipt is absent for the current iCloud incident. | Global probe can time out and CloudDocs state is intentionally not force-killed or deleted; the native copy boundary now requires an integrity-checked three-stream pre-copy cohort before mutation. | Capture a bounded fresh provider evidence receipt after sync settles; keep transfer/eviction disabled until it is complete. |
| P0 | iCloud local-copy eviction needs an exact production plan and runtime receipt. | A 2026-08-29 15:53 +0900 public Foundation snapshot found 61 uploaded, current, idle, conflict/error-free local copies totaling 2,955,091,968 allocated bytes; 10 additional items totaling 632,115,200 bytes reported the iCloud server unavailable and remain blocked. The cohort changed during observation; the aggregate audit opened no content and emitted no mutation fingerprint. | Merge the public per-item evidence path, generate a fresh exact-head batch fingerprint, obtain attributed approval, then require retained uploaded ubiquitous identity, `notDownloaded` status, reduced allocation, and secondary APFS before/after evidence. |
| P0 | Disk pressure telemetry and provider queue evidence must remain comparable across loops without retaining raw provider output. | Cloud plans and explicit iCloud health refreshes persist bounded, path-free `LocalVolumeSnapshot`, `ProviderClientRuntimeSnapshot`, and `IcloudSyncHealthEvidenceSnapshot` records under `volume-pressure-evidence`, `provider-client-runtime-evidence`, and `icloud-sync-health-evidence`; iCloud plans now combine them into a timestamp/fingerprint-bound cohort. | Missing, incomplete, malformed, or more-than-five-minute-skewed cohort observations remain blocked; a fresh exact-head native incident plan is still needed to compare the emitted cohort with the live incident. |
| P0 | Shared `/tmp` physical reclaim remains unavailable. | DiskSage can inventory current-user top-level artifacts and record advisory completion evidence, but permanent execution and approval deliberately fail closed because same-user markers do not authenticate producers and path APIs do not atomically bind final revalidation, deletion, journal durability, and receipt durability. This PR therefore recovers zero bytes from `/tmp`. | Implement an OS-enforced producer authority plus descriptor-relative, race-resistant tree mutation and a deletion protocol whose durable intent and outcome survive receipt failure; keep every `/tmp` child non-mutable until those proofs pass adversarial reality tests. |
| P1 | The central hourly development/review loop is live; the repository-local advisory path remains manual-only. | The repository-local `.github/workflows/hourly-product-loop.yml` remains `workflow_dispatch`-only because its contextual-orchestrator call is advisory. The trusted central [`disksage-hourly-review-repair.yml`](https://github.com/ContextualWisdomLab/.github/blob/main/.github/workflows/disksage-hourly-review-repair.yml) runs at `37 * * * *`; scheduled run [`32986653461`](https://github.com/ContextualWisdomLab/.github/actions/runs/32986653461) completed successfully on central head `e00bd7964f332b69cf7b430b0cb5ad486eef8258`, following four other successful scheduled runs. | Retain successful scheduled receipts, read-only repository permissions, exact-head binding, and no provider-secret import or foreign-repository mutation; verify the local advisory receipt only when manually configured. |
| P1 | Open PR queue prevents a clean protected release line. | At this loop capture PR #213 is exact head `6f424af` on `feat/provider-sync-dynamic-goals`; its required checks reset after the provider-dump pipe repair and the prior review decision remains stale `CHANGES_REQUESTED`. The orphan cleanup follow-up is PR #245, initially implemented at `3d2406c` and subsequently extended with provider-sync and cleanup-refresh safety fixes. Both remain protected and unmerged pending exact-head review. | Process one PR at a time: current-head review → fix → required checks → fresh approval → normal protected merge; never bypass or self-approve. |
| P1 | Current UI coverage is contract-heavy rather than runtime E2E for native File Provider states. | The UI now displays `로컬 최신본·업로드 미확인` and maps blockers without backend detail; provider operations are not safely reproducible on this full disk. Rust fixtures now cover `local-current + is_uploaded=false`, provider timeout, timeliness transitions, and receipt/evidence invalidation; native runtime E2E remains unavailable while the provider is unhealthy. | Keep the fixture-backed state machine green and add a bounded native E2E receipt only after a quiet provider observation is authoritative. |
| P1 | Ontology/catalog integrations are export boundaries, not deployed services. | Naruon/semantic catalog and Zotero local API docs/contracts exist; no Noema/contextual-orchestrator runtime dependency is required. | Keep integrations optional and path-free; add live service tests only when a concrete consumer and secret boundary exist. |
| P2 | 100% documentation/docstring and edge-case coverage is not yet evidenced. | Existing checks cover core Rust/TS behavior, not a repository-wide percentage claim. | Publish measured coverage per language and close high-risk edge paths before claiming 100%. |
| P1 | VM guest free space and host image allocation are conflated by runtime tools. | Podman/Colima logical reclaim values do not prove APFS allocation; raw image rewriting is unsafe while a VM is active. | Runtime maintenance plan offers bounded guest `fstrim`, records before/after host observations, and reports host-image compaction as unsupported without a native proof. |
| P0 | Release artifacts omitted the exact cloud-local eviction batch planner needed to reproduce provider-safe local-space evidence. | The release matrix now builds, help-probes, checksums, exact-set validates, and provenance-attests `disksage-cloud-local-eviction-batch` on macOS. Linux and Windows artifacts remain excluded because their provider-local observation paths are unsupported. Pull-request artifacts remain unattested and cannot authorize execution. | Require exact-head hosted packaging checks, then regenerate the read-only OneDrive plan from the matching macOS artifact; keep human approval and Finder action as separate later boundaries. |
| P0 | OneDrive Finder assistance could report local-space verification from allocation reduction alone, and its verifier was absent from release artifacts. | Verification now also requires fresh upload, download, conflict, pause, trash, and File Provider identity evidence. The macOS verifier is help-probed, checksummed, exact-set validated, and provenance-attested. | Wait for OneDrive synchronization to complete, then rerun verification from the matching exact-head release artifact. |
| P0 | The iCloud-specific public Foundation planner existed in source but was absent from installable release evidence. | The release matrix now builds, help-probes, checksums, exact-set validates, and provenance-attests `disksage-icloud-local-eviction-batch-macos-arm64`; Linux and Windows intentionally ship only the provider-generic planner. | After both stacked changes merge and exact-head macOS packaging is green, generate a fresh read-only production plan and retain its fingerprint; execution still requires a later exact human approval. |
| P0 | The release planner artifact lacked the matching read-only cloud-local inventory producer, so an exact-head plan could not be regenerated without compiling source or reusing stale evidence. | The macOS release matrix now builds, help-probes, checksums, exact-set validates, and provenance-attests `disksage-cloud-local-inventory-macos-arm64` alongside both batch planners. | Require exact-head hosted packaging checks, then use that artifact to generate a fresh inventory and plan; predecessor manifests remain invalid and execution still requires exact fingerprint approval. |
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

## 2026-08-27 central hourly scheduler operation evidence

- Central scheduled run `32986653461` completed successfully on `.github` head
  `e00bd7964f332b69cf7b430b0cb5ad486eef8258`. Runs `32979847404`, `32975212084`,
  `32966118019`, and `32961434095` also completed successfully, replacing the earlier
  startup-failure-only snapshot with repeated operational evidence.
- DiskSage keeps its local advisory workflow manual-only. The central caller remains the hourly
  OpenCode review/repair authority, so the standalone repository does not duplicate scheduler or
  provider-secret ownership.
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

## 2026-08-27 container and worktree reclamation loop

- The standalone-clone authority now has a headless plan/execute boundary. It reuses the desktop
  contract unchanged: current same-repository GitHub PR branch and head, clean status, one worktree,
  inactivity, filesystem identity, exact confirmation, and an external journal are all required
  before moving the clone to OS Trash; branch deletion and Git pruning remain prohibited.
- The live iCloud inventory contained both fully uploaded local copies and `local-current` items
  with `is_uploaded=false`. Batch planning now partitions those states automatically: only exact
  item plans that pass the existing provider, active-use, conflict, and allocation checks remain
  actionable; excluded indices and bounded reasons are fingerprint-bound without disclosing paths.
- Docker dangling-image plans now obtain reclaim bytes from a single exact-identity `image inspect`
  pass. The human-readable `image ls` size is never converted heuristically; missing, duplicate, or
  mismatched numeric size evidence blocks the category instead of overstating the 300 GB target.

- PR #267 observed head `2d0787537a964c319bd4ee994070268bd77f2284` delivers evidence-bound cleanup for stopped
  containers, untagged and unreferenced images, unreferenced volumes, and unused non-default
  networks across Docker, Podman, and the Colima Docker context. Execution re-audits the exact
  candidate identities and requires the matching approval phrase plus a rationale; runtime
  diagnostics remain outside customer-visible and public evidence boundaries. The local frontend
  suite passes 35 files / 155 tests after repairing the stale UI ownership assertion and adding
  the closed-PR worktree contract.
- The Git worktree authority removes clean, inactive secondary worktree folders whose commits are
  strict ancestors of explicitly retained refs. ADR-0013 additionally binds closed-but-unmerged
  cleanup to authenticated GitHub evidence: exact `CLOSED` state, same-repository identity, local
  branch ref, and exact head OID must all match. The evidence is refreshed immediately before
  removal and participates in the approved plan fingerprint; detached and fork PR worktrees remain
  preserved for manual review. Branches and commits are never deleted.
- Review-driven runtime hardening verifies Podman network membership through the authoritative
  all-container listing instead of assuming Docker-shaped network-inspect fields, follows valid
  Homebrew/Docker Desktop CLI symlinks, and collapses unavailable runtimes into one actionable UI
  summary rather than rendering repeated connection-failure panels. Runtime command descendants
  share a private process group so timeout enforcement also closes inherited output pipes; closed
  PR discovery filters merged history before applying its bounded exact-head authority set.
- Current protected-delivery snapshot: `main` is
  `79067c1160ddedf7fc962cbf8067ce7e83c4564a`; 40 PRs are open (17 ready, 23 draft) and none has an
  exact-head independent approval. PR #267 is ready for review and blocked; CodeRabbit passed while
  CodeQL remained queued on the observed head. Combined status alone is not proof of required workflows,
  resolved threads, or approval gate, so no protected merge is claimed.

## 2026-08-28 host container-resource cleanup evidence

- The host-compatible `docker` command is backed by Podman 5.8.2. The running BuildKit builder and
  `accounting-information-platform-test-postgres` container were retained; no running workload was
  stopped or restarted.
- `docker image prune -f` removed two dangling images (the runtime reported them as untagged and
  unreferenced). After confirming that only the BuildKit builder and the test PostgreSQL container were
  running, `docker image prune -a -f` removed 19 additional images that no container referenced; images
  needed by those running containers were retained. `docker network prune -f` removed
  `pg-erd-cloud-pr-alembic-reconcile_default` after authoritative inspection showed an empty container
  map; the built-in `podman` network was retained.
- Three unreferenced zero-byte test-state volumes were removed:
  `github-actions-modernize_data_redis`, `naruonprivmail_live-e2e-state`, and
  `pg-erd-cloud-pr-alembic-reconcile_pgdata`. Non-empty PostgreSQL volumes were retained because a
  zero runtime link count alone does not prove that their data is disposable. PR #267 exposes the same
  evidence-bound per-volume review and exact-identity re-audit in the product UI.
- An unregistered local checkout of open draft PR #247 had no active file holders; `cargo clean` removed
  its 2.7 GiB Rust target while preserving the source, branch head, and PR worktree content. Registered
  open-PR worktrees and active build targets were not removed. The shared uv archive cache was left
  untouched because live MCP processes were executing from it; cache eviction must first obtain the same
  active-use evidence through the product flow.
- The previously requested macOS Homebrew maintenance command was dry-run first and then executed:
  `brew cleanup --prune-prefix` removed 2,169 broken symbolic links and 85 stale Homebrew directories.
  No package or application data outside Homebrew's own prefix was targeted.
- After the customer-copy hardening, the local frontend suite completed with 37 files and 163 tests
  passing; `npm run check` reported zero errors and zero warnings. The exact PR #267 head for this
  evidence is `e14879118377f4716b4bbb5dd5f5dcbc9571fbaf`.

## 2026-08-28 DiskSage cache cleanup execution evidence

- The PR #267 Rust headless cache command was built once in an isolated target directory and run
  with `--execute`. It moved 15 inactive, identity-bound cache children (1,171,384,438 bytes) to
  the user Trash. Active-use or incomplete-evidence blockers rejected the npm `_npx` child and the
  uv lock/archive roots; no live MCP cache was forced out.
- With the separate explicit `--purge-proven-cache-trash` approval flag, the command rechecked
  structural signatures and permanently removed four DiskSage-owned cache directories from Trash
  (npm `_cacache`, pnpm `v11`, uv `simple-v21`, and Edge `Default`, 1,108,914,878 bytes). The
  journal records pending and terminal outcomes for every object; no unrelated Trash entry matched
  a proven cache signature. The data volume's available space rose from about 6.2 GiB during the
  build to 7.2 GiB after purge; APFS accounting may fluctuate while other builds run.
- This is the same fail-closed path exposed by the product UI: inactive regenerable children can be
  staged and explicitly purged only after structural re-audit, while active processes and
  incomplete provider evidence remain preserved.

## 2026-08-28 uv archive child-level reclamation

- The first automatic run correctly skipped the uv `archive-v0` parent when its active-use probe was
  incomplete. The implementation was then corrected so both the reviewed and current snapshots use
  the same child-expanded set; a focused Rust suite passed 7/7.
- A second run re-audited 865 direct uv archive children independently. 855 inactive children
  (4,640,480,301 bytes) were moved to Trash; active or incomplete evidence remained in place. The
  explicit journal-backed purge rechecked the original cache parent, source absence, non-symlink
  directory type, and bounded byte count before permanently removing 846 children
  (4,574,395,074 bytes). Nine entries were no longer purge candidates after re-audit and were
  preserved. The available-space reading reached about 3.1 GiB afterward; the remaining archive
  content is still in use or was not proven safe.

## 2026-08-28 stale container-volume follow-up

- A fresh Podman-backed Docker inventory found no further removable images or networks after the
  earlier 21-image and one-network cleanup. Four anonymous 64-character volumes had zero
  container links, no Compose labels, and were created by the same day's isolated test runs;
  they were removed explicitly. Seven labeled, non-empty database/graph volumes remain retained:
  an unlinked data volume is not evidence that its contents are disposable.
- The exact PR #267 head for this follow-up is `bc57679b6d7bc78dec5fa86dc3922b11e5092751`.
  Host free-space readings remain volatile while unrelated builds run, so the product records
  the resource identities and re-audit result rather than claiming a stable net free-space delta.

## 2026-08-28 Noema sidecar dependency gap

- The required Noema review for PR #267 remains non-passing because the central sidecar pins
  contextual-orchestrator `c60ec889...`, whose generated catalog is list-shaped while its
  `load_agents` implementation expects an `agents` object key (`KeyError: 'agents'`). The
  upstream compatibility repair is present on contextual-orchestrator PR #901 at head
  `d1bd3626ddb04a7b14e43aebf60827ac50ef8d17`; it is independently protected and not yet merged.
- DiskSage therefore keeps the Noema gate fail-closed and does not bypass it or treat the PR as
  merge-ready. Once the upstream repair is normally merged, the central sidecar pin must be
  updated and the exact DiskSage head re-reviewed. The pin update is tracked in central
  [`.github#1371`](https://github.com/ContextualWisdomLab/.github/pull/1371) at head
  `78f5c5642f5a49da6827f7a786b1ad4e79a6d03a`.

## 2026-08-28 customer-copy boundary

- PR #267 exact implementation head `cdda4f9fdc4001f588d61ef3a152e5f4f418262e` now applies one
  customer-copy contract to cloud transfer, local-copy cleanup, cache and developer-folder cleanup,
  Homebrew, duplicate/orphan cleanup, inventory, and container-resource screens. Native diagnostics,
  identifiers, provider protocol terms, and command output are no longer reflected in visible
  messages; each warning, error, or notice names a bounded next action.
- The contract test covers every existing screen, including the container image/volume/network
  cleanup panel, and rejects implementation terms in visible text and attributes. The local checks
  passed with `npm run check` (0 errors, 0 warnings) and 38 frontend test files / 165 tests. The
  protected PR remains open and blocked until current-head hosted checks and an independent approval
  pass; this UI proof does not authorize a merge or a deletion.

## 2026-08-28 stale anonymous volume follow-up

- A new local Podman inventory found one additional anonymous volume,
  `2b9caeb3e63f84fffcc87c0ad365fed7b9f09812581d7389061488857831ca4c`, created at 12:22 KST.
  It had no Compose labels, no container reference (`docker ps -a --filter volume=...` returned no
  containers), and a runtime-accounted size of 462.9 MB. The volume was removed only after that
  identity and reference check; the seven labeled database/graph volumes and the BuildKit-linked
  volume remain retained.
- Runtime accounting changed from 1.268 GB to 804.9 MB of local volumes and host availability moved
  from about 8.5 GiB to 8.7 GiB. APFS and concurrent hosted builds make the host delta non-authoritative;
  the product records the exact identity and re-audit rather than promising a fixed byte gain. The
  same per-volume evidence and re-audit path is available in PR #267 (head `b4105f8e47f165c702fedd4d05f7d4af6d29b603`).

## 2026-08-28 exact Docker image-size and customer-copy follow-up

- PR #267 head `e24037bb78a66aeed2ae78bb03ff8503904b0902` now obtains Docker dangling-image
  reclaim bytes from one exact-ID `image inspect` response. Docker's human-readable listing size is
  not converted; missing, duplicate, non-numeric, or mismatched identity evidence blocks the plan.
- The same head removes internal engine/model names and raw exception text from customer-facing
  cleanup, organize, inventory, duplicate, and Homebrew messages. The user is given the next
  bounded action while the exact approval, re-audit, and receipt authority remain unchanged.
- Local targeted frontend checks passed (7 tests) and `npm run check` reported zero errors and zero
  warnings. Hosted checks and independent review remain required before protected merge.

## 2026-08-28 runtime-maintenance input boundary follow-up

- PR #267 head `f06d21015a9881fa3090ab1f1106eee8fea5fc20` rejects control characters in a Podman or
  Colima trim rationale before runtime probing or receipt persistence. The same fail-closed input
  boundary is now shared by cache, container, Homebrew, worktree, and VM-maintenance actions.

## 2026-08-28 PR #267 exact-head hosted-gate observation

- PR #267 current head `c09cc30f137680597ceeef9db9d4e5a29206b389` passed the hosted static-analysis,
  dependency, vulnerability, coverage-source, and Windows path checks. The required Strix scan
  retried its contextual-orchestrator provider three times and received HTTP 500 each time without
  producing a vulnerability artifact; the required gate therefore remained fail-closed as provider
  infrastructure unavailable, not as a code finding.
- The required OpenCode gate also remained fail-closed because no authenticated current-head
  `opencode-agent` verdict had been posted. A review-only `@opencode-agent` dispatch request was
  recorded on the PR; no self-approval, bypass, or merge was attempted. The remaining native build
  and test jobs were still running at this observation, so release readiness is not claimed.

## 2026-08-28 measured emergency reclaim and VM recovery

The PR #271 Linux test gate exposed a fixture-boundary regression rather than an editor-cleanup
failure: 13 independent move, eviction, clone, and journal tests created mutation fixtures under
the globally protected shared `/tmp` tree. Production protection remains fail-closed for every
shared-temp child, including current-user-owned children. Hosted mutation fixtures now use the
runner's private workspace temp root instead of weakening the shared production guard.

- VS Code's native `.vscode/extensions/.obsolete` lifecycle document identified 22 still-present
  obsolete extension directories totaling 1,283,664 KiB. DiskSage now treats only those exact real
  child directories as development artifacts; it does not infer obsolescence from directory age or
  version ordering. The filtered headless execution revalidated all 22 identities, moved them to
  Trash, journaled them, and purged only those exact Trash entries; APFS available space increased
  by 1,291,364 KiB in the bounded before/after sample. The same native lifecycle contract found
  and revalidated 15 additional obsolete directories in Cursor, VS Code Insiders, and VS Code
  Server; purging only their journal-matched Trash entries increased APFS availability by another
  692,768 KiB.
- A focused physical-allocation audit found about 7.7 GiB in AppMap downloaded tool binaries and
  1.9 GiB in inactive Superset network diagnostics. AppMap uses the existing regenerable-data
  cleanup contract. Superset diagnostics remain an explicit-review catalog item because historical
  logs cannot be regenerated. Every selected child is still identity-bound, checked for active use
  immediately before mutation, journaled, and moved to OS Trash. No application database or
  cloud-provider state is included.
- The new headless path moved all three inactive AppMap cache children and four inactive Superset
  diagnostic files to Trash. Only those journal-matched regenerable objects were then purged;
  APFS available bytes rose from 56,603,525,120 before execution to 66,297,540,608 after purge.
  Active npm and uv children were blocked and retained.
- A bounded inventory found about 146 GB under `/private/tmp`, dominated by isolated Cargo and
  coverage target roots. DiskSage removed only current-user-owned generated roots after signature,
  open-file, and process-reference checks; `/private/tmp` fell to about 20 GB and APFS available
  space rose from about 3.6 GiB to 53 GiB after additional inactive dependency/build roots were
  removed. Source trees, active worktrees, provider data, and user documents were preserved.
- The running Podman guest initially failed its SSH probe with EOF and its journal reported I/O
  errors. A runtime-native stop/start restored the guest connection. DiskSage then removed only a
  stopped BuildKit container, its dedicated state volume, and its unreferenced image. Same-day
  PostgreSQL containers and every data-bearing named volume were preserved.
- The fixed guest `fstrim` reported 99.5 GiB trimmed. The host sparse image allocation changed from
  43,951,260 KiB to 30,440,160 KiB, while APFS available space rose from about 53 GiB to 65 GiB.
  These are separate before/after observations, not a promise that logical trimmed bytes equal
  host bytes. No raw image rewrite, truncation, or category-wide prune was used.
- The 300 GB objective is not yet satisfied: the latest observation proves about 65 GiB available.
  Cloud eviction remains blocked for items without current provider-upload proof, and non-identical
  photos remain blocked pending measured quality evidence and a selected survivor.

## 2026-08-28 effective macOS cache-root correction

- Native read-only discovery reported `uv cache dir` as `~/.cache/uv`, npm cache as `~/.npm`,
  pnpm store as `~/Library/pnpm/store/v11`, and pip cache as `~/Library/Caches/pip`. The former
  macOS catalog pointed uv, pnpm, Codex runtime, Node, PyTorch, Prisma, and GitHub CLI entries at
  non-effective directories, so DiskSage could report zero bytes while about 2.8 GB remained under
  `~/.cache` alone.
- The catalog now uses an absolute `XDG_CACHE_HOME` when supplied and otherwise `~/.cache`, retains
  explicit UV/Hugging Face overrides, and scopes pnpm to its content-addressed store root plus its
  separate metadata cache. Node, PyTorch, Prisma, GitHub CLI, and Codex runtime caches remain
  manual-review candidates rather than gaining automatic deletion authority from path discovery.
  Headless execution still re-lists exact children, rejects changed
  identities or incomplete/active-use evidence, moves candidates to OS Trash, and journals each
  mutation; the cache root itself is preserved.
- The live Trash contained about 695 MB, including a 368 MB uv `git-v0` cache and collision-renamed
  uv build/wheel/source cache directories. The proven-cache purge now recognizes such macOS
  collision names only when both a known base name and cache-specific structure match; unrelated
  Trash entries and user data remain outside this irreversible path.
- The corrected headless path moved only inactive, identity-matched children from npm, uv, and pnpm
  into Trash; active or incomplete-use entries (`uv/.lock`, npm `_npx`, and pnpm v3) remained in
  place. Structure-bound purge then permanently removed uv git and pnpm metadata caches totaling
  410,257,515 bytes, followed by npm `_cacache`, uv archive/index, and pnpm v10 caches totaling
  1,896,107,386 bytes. The measured APFS available-space increases were 327,860 KiB and
  1,361,876 KiB respectively; logical bytes are not substituted for those host observations.
- A fresh GitHub/current-HEAD audit of 235 `/private/tmp` repositories found only two clean,
  inactive, exact-head candidates: contextual-orchestrator PR #902 (merged) and
  accounting-information-platform PR #30 (closed unmerged). Their linked worktrees were removed
  through `git worktree remove` without force. Dirty, active-evidence-incomplete, open-PR, and
  head-mismatched paths were preserved; the immediate APFS sample fluctuated downward, so no
  positive physical gain is attributed to those 32 MiB of logical worktree data.

## 2026-08-28 exact-head hosted-test repair and additional measured reclaim

- The hosted container-capacity regression failed before exercising its assertion because its fake
  Docker process omitted the mandatory `info` health response. The fixture now implements that
  production precondition; the safety behavior remains unchanged.
- Release builds uploaded `release-disksage-windows-2022-1`, while the verification script expected
  `windows-latest`. The verifier now uses the pinned matrix identity, and a synthetic exact 17-file
  artifact set passes the checksum, path, type, and count contract.
- Three clean inactive temporary Git checkouts were removed only after a fresh fetch proved their
  exact local commits remained reachable from remote branches; generated Rust output was removed
  separately after ignore and active-use checks. APFS available space increased by 1,283,576 KiB
  across those two bounded operations. Provider synchronization paths and locally unique commits
  were preserved.

## 2026-08-28 standalone-clone execution hardening

- Standalone clone cleanup now rejects redirected or symlinked Git administration directories,
  incomplete repository audits, and journals located inside the clone or behind unsafe path types.
  Stale-open PR eligibility still requires an explicit operator cutoff; DiskSage does not invent an
  age threshold. Focused clone and inherited worktree safety tests pass without mutating user data.

## 2026-08-28 Superset partition-cache boundary

- Superset's isolated HTTP cache measured 1,237,960 KiB and its compiled-code cache measured
  48,200 KiB. DiskSage now catalogs only those two regenerable roots; cookies, local/session
  storage, IndexedDB, preferences, and historical network diagnostics remain excluded.
- The live execution moved `No_Vary_Search`, JavaScript, and WebAssembly cache children after
  identity and active-use checks. The large `Cache_Data` child remained fail-closed because the
  recursive native open-file observation exceeded its evidence timeout; process inactivity alone
  was not promoted to deletion authority. The same bounded run and proven-cache purge increased
  APFS availability by 170,104 KiB without claiming the blocked 1.2 GiB.

## 2026-08-28 native Trash and development-artifact execution boundary

- The default macOS Trash backend delegated to Finder and timed out with AppleEvent `-1712` while
  provider work was active. Both ordinary and identity-bound DiskSage Trash mutations now reuse
  the installed trash library's native `NSFileManager` method, avoiding Finder automation without
  killing or pausing Finder or File Provider processes.
- A bounded scan of one development workspace found 78 marker-validated dependency artifacts
  totaling 4,551,103,622 logical bytes. One inactive project execution moved three identity-matched
  `node_modules` roots totaling 189,125,777 logical bytes to Trash. This is reversible logical
  cleanup only: DiskSage does not claim physical recovery until an exact DiskSage-attributed Trash
  entry can be purged without touching unrelated user Trash.
- A later `/private/tmp/opencode` audit found five ignored dependency environments in three dirty
  but inactive BandScope worktrees: two `node_modules` roots and three Python `.venv` roots totaling
  3,378,668 KiB before removal. Git status was preserved, every path was confirmed ignored, and
  recursive `lsof` plus process-command evidence found no active user. The immediate APFS sample
  rose by only 500,316 KiB while background provider/build activity continued, so the larger
  logical total is not reported as physical recovery. DiskSage now exposes this path only through
  the explicit headless `--execute --permanent` disposition: it re-scans the bounded manifest,
  rejects active or changed roots, rechecks filesystem identity, and journals the irreversible
  deletion without emptying unrelated user Trash.
- The same evidence contract was applied to 15 additional ignored Rust `target` trees under
  `/private/tmp`: each root was Git-ignored, had complete recursive open-file evidence, and had no
  process-command reference. The first seven removals increased APFS availability by 10,537,012
  KiB and the next eight by 1,567,780 KiB. Source, dirty changes, Git heads, and cloud paths were
  untouched. Two clean inactive ScopeWeave worktrees whose exact heads matched closed PR #626 and
  merged PR #628 were then removed through ordinary `git worktree remove`; dirty closed PR #622
  was retained.
- Claude Code's native launcher symlink identified `2.1.234` as the installed executable. Three
  non-target version binaries (`2.1.202`, `2.1.201`, and `2.1.177`) had no open-file or process
  references; removing only those binaries increased APFS availability by 683,868 KiB and the
  launcher still reported version `2.1.234`. DiskSage does not yet encode this symlink-target
  lifecycle authority, so stale self-updating tool versions remain a measured product Gap rather
  than a generic age-based cache rule.

## 2026-08-29 exact-head reclaim and stacked-PR baseline

- The session opened with 74,082,400 KiB available on the APFS Data volume. Bounded DiskSage
  executions removed only marker-validated generated artifacts and one clean, inactive worktree
  whose exact head was proven closed and retained. Availability reached 97,770,624 KiB before
  concurrent builds consumed new space; no logical-size total is substituted for that physical
  observation.
- A focused current-HEAD test proved the File Provider Git-metadata blocker, and the test-only
  helper is now excluded from production builds. The resulting 2,386,748,792-byte Rust `target`
  tree was then permanently removed through the same manifest, active-use, identity-recheck, and
  immutable-journal path that DiskSage exposes to operators.
- A fresh audit of 97 BandScope worktrees found no candidate satisfying all containment,
  cleanliness, inactivity, and closed-PR requirements. All 97 remain preserved; worktree names or
  age alone did not grant removal authority.
- VS Code's native obsolete-extension evidence had previously identified 22 directories totaling
  1,314,471,936 allocated bytes. A fresh exact-path recheck now finds none of those directories,
  so DiskSage neither repeats a mutation nor attributes additional physical recovery.
- PRs #273, #275, and #276 are ready for review at exact heads `03585345`, `020b2e19`, and
  `10fcbdeb`. The first two inherited the same Windows release-artifact identity repair and are
  undergoing new hosted checks. Merge remains blocked until every current-head required check is
  terminal-success and repository review policy is satisfied.
- The 300 GB physical-recovery objective remains open. Provider-local eviction still requires
  native uploaded/current evidence, non-identical photo selection still requires measured quality
  evidence and explicit survivor confirmation, and active Podman/Colima resources remain outside
  prune authority.

## 2026-08-29 OneDrive native local-eviction boundary

- Exact release lineage `9c010252fccbf92256ef1d19ffae063ea060becc` produced a macOS artifact ZIP
  with SHA-256 `c6d2125684237adfa00c1ebef63b38179f7d40561c5f38e768526d0208968af8`.
  Redacted receipt `disksage-cloud-live-20260829-9c010252` used a mode-0700 directory and
  mode-0600 files; no private filesystem or cloud path is retained here.
- The complete bounded iCloud inventory at `1787997809914` ms visited 126 entries and 122
  files, emitted 120 candidates totaling 20,860,424,192 allocated bytes, and had no issue or
  truncation. Its plan at `1787997836117` ms admitted zero items and zero allocated bytes,
  fingerprint `5208ab891669fc55ae6be9265614f0e18b722cd67656d3872689081926856336`, with
  `no-planned-items`. Receipt SHA-256 values are
  `5fc603ccc4c0d7de505b38d6bff2262f435eb67ee87c61be6184795b703fce0a` (inventory)
  and `2954393d0cd6f4dbc6b952dd729009fe44067a4371983af0dd96bd2205c578ce` (plan).
- The complete OneDrive traversal at `1787997897288` ms visited 277,410 entries and
  242,891 files. Its allocation-descending top 128 represented 9,792,589,824 allocated
  bytes; result truncation means this is not whole-root authority. The plan at
  `1787997919065` ms admitted all 128 emitted items totaling 5,272,006,656 allocated bytes,
  fingerprint `ad0118c3316579e768df8de2e1942b8109c76e92381b4346b2824e146e01b80a`, with
  the human-approval blocker. Receipt SHA-256 values are
  `46f26c492c190129c7378e61be6aa447441819131ce4022133e03a580f415127` (inventory)
  and `a09ae25b256ac66e0297bf8d9bf81a471857dc8b8116aafc2e20b99d548bf5c1` (plan).
  These are read-only observations; no eviction or other mutation occurred.

- The selected OneDrive root is a registered macOS File Provider domain. A bounded native status
  probe reported the root uploaded, current, unpaused, untrashed, and eligible for the provider's
  unpin action; its allocated subtree remains roughly 32 GiB. Presence in `CloudStorage` alone is
  not used as upload or eviction authority.
- DiskSage now reuses its exact-path, provider-status, active-use, fingerprint approval, immutable
  receipt, and post-allocation verification contract for individual OneDrive files. Execution
  uses exact item evidence and Foundation's local-only ubiquitous-item eviction. Immediately
  before that call, DiskSage re-resolves every mutable sync/policy gate plus exact item/version
  identity. Exact File Provider evidence replaces vendor-command inference. It does
  not require OAuth and never deletes the visible cloud item.
- The same evidence contract now supports bounded OneDrive batches through the provider-neutral
  `disksage-cloud-local-eviction-batch` CLI. Sync-incomplete items are excluded by index, every
  selected item is replanned before the first mutation, and execution stops after the first failed
  or incompletely verified item.
- The live provider-wide probe currently shows active upload/download, indexing, and reconciliation
  work. Local-only eviction is governed by exact item evidence instead, but the desktop app did not
  complete its bounded graceful quit, so no item was evicted. A physical-space receipt from the
  live provider remains open; the 300 GB goal is not
  claimed complete.
- A later live batch found five uploaded, current, idle, provider-evictable Mplus videos totaling
  11,397,992,448 allocated bytes. Their NFC manifest spelling initially failed against the NFD
  File Provider root; the shared containment boundary now accepts only component-wise canonical
  equivalents and still rejects sibling roots. After that fix all five received exact item
  authority, but OneDrive's signed `/unpin` process reported `Failed operation=2` with native
  status `-2` for the first item while returning process status zero. DiskSage detected the failure,
  recorded the attempted batch, halted before item two, and reclaimed zero bytes. Replacing the
  obsolete vendor command motivated the current Foundation boundary; exact-head execution and
  post-allocation verification remain a P0 acceptance gap.
  the approved cloud items remain intact and locally materialized until that native path passes
  post-allocation verification.

## 2026-08-29 temporary-workspace generated-cache recovery

- Project-local Python 3.14 `.venv314` environments now share the same manifest, active-use, journal, and permanent-reclaim checks as `.venv`.

### Podman external-container image authority

- Live deletion exposed Buildah storage containers hidden from ordinary `podman container ps --all`; their images rejected deletion despite appearing dangling.
- Podman image membership now includes native `--external` evidence before issuing an approval phrase. The same live store now produces zero image candidates rather than unsafe partial execution.

- The live `/private/tmp` inventory exposed repeated Rust, Node, Python environment, type-check,
  test, lint, and CodeGraph outputs inside review worktrees. DiskSage's identity-bound permanent
  generated-artifact action removed these outputs without removing a worktree, branch, source
  file, or untracked source change; each mutation was recorded in the private operation journal.
- The reusable artifact catalog now includes `.mypy_cache`, `.pytest_cache`, `.ruff_cache`, `.tox`,
  and `.nox`. Discovery tests prove the new cache names are admitted, while the existing rescan,
  filesystem-object identity, active-use, and bounded traversal gates remain unchanged.
- OneDrive continued a large download while cleanup ran, so physical APFS availability fluctuated
  independently of the bytes removed. Provider transfer cancellation and a fresh capacity snapshot
  remain required before the 300 GB outcome can be claimed.

## 2026-08-29 merged-worktree and isolated-project-cache execution

### Exact PR commit membership boundary

- The worktree authority now verifies each registered HEAD against the exact same-repository
  GitHub pull-request commit list. This closes the squash/rebase ancestry gap and safely recognizes
  detached intermediate commits without inferring identity from a directory or branch name.
- A SHA may occur in more than one PR. Verified membership in any open PR is therefore a mandatory
  preserve veto, even if another PR containing the same SHA is already closed or merged. Search
  caps, pagination/output bounds, repository mismatch, authentication failure, and timeout remain
  evidence gaps rather than cleanup authority.

- A live Naruon audit exposed a second squash-merge boundary: PR #1370's clean, inactive worktree
  has the exact merged pull-request head, but that head is not an ancestor of the retained branch.
  DiskSage now obtains closed-unmerged heads separately and scopes merged queries to branches in
  the registered worktree set, then accepts only an exact same-repository branch-and-head match.
  Repository-wide merged history therefore cannot exhaust the evidence bound. PR #1454 remains preserved because
  its detached intermediate commit is also part of open PR #1466; open work always vetoes reclaim.
  The fingerprint-bound native removal path then re-audited and removed only PR #1370's worktree;
  path and Git registration absence were verified, the branch was retained, and the fresh audit
  reports 28 preserved worktrees, zero candidates, complete evidence, and zero gaps. Its
  253,673,472-byte allocated upper bound is not presented as physical APFS recovery.
- The exact-duplicate collector now prunes `.photoslibrary` and `.photolibrary` packages and rejects
  either package as a scan root. A regression test proves identical bytes inside a Photos package
  cannot form a deletion cluster with an external file. The 44 external Pictures images currently
  have unique exact-content digests; perceptual comparison and measured quality-survivor selection
  remain an open product Gap and no non-identical photo was deleted.
- The managed Apple Photos gap now has a separate macOS-native PhotoKit path. It requests read/write
  access only from the customer's connect action, inventories local identifiers and measured
  resource evidence without allowing network download, groups only exact SHA-256 content matches,
  and requires one explicit keeper per group. iCloud-only originals block deletion planning and
  remain unmaterialized. Execution re-fetches every identifier, metadata fingerprint, and local
  content digest before invoking Photos' own deletion transaction and confirmation; a create-new
  receipt follows success. Near-duplicate managed assets remain unavailable rather than receiving
  an uncalibrated score, so that Gap is explicit and non-destructive.

- A fresh Naruon audit proved exactly one removable worktree: PR #1429 was merged, its detached
  head was retained by current `origin/develop`, the checkout was clean and inactive, and no open
  PR stack retained it. DiskSage removed only `/Users/seonghobae/naruon-wt/pr1429` through its
  fingerprint-bound approval path without force, branch deletion, or Git pruning. Path and Git
  registration absence were both verified; the post-audit reports 29 retained worktrees, zero
  candidates, complete evidence, and zero gaps. Its 253,587,456-byte allocated upper bound is not
  presented as APFS recovery because concurrent provider writes reduced free space during removal.
- DiskSage then permanently removed 543 identity-matched, inactive generated artifacts from
  Superset's isolated project copies: Python environments and caches, `node_modules`, and CodeGraph
  indexes. Both executions completed without a failed candidate, were journaled, and re-audited to
  zero candidates. The second bounded execution increased APFS availability by 820,188 KiB; logical
  candidate totals are kept separate from that physical observation.
- Every `.venv314` discovery path now requires a bounded regular `pyvenv.cfg` whose version is
  Python 3.14, rather than treating a Git or project marker as sufficient deletion evidence. The UI
  names each newly supported Python cache and test environment so the operator can decide what to
  review next without seeing internal implementation labels.
- A subsequent `/private/tmp` execution revalidated 752 generated candidates and permanently
  removed 740. It preserved nine active candidates, two whose manifests changed, and one whose
  active-use evidence was incomplete. APFS availability increased by 4,043,844 KiB between the
  bounded before/after observations; the remaining generated candidates are not counted as
  reclaimable while their safety evidence is incomplete or a process still uses them.
  After the focused Rust verification finished, native `cargo clean` removed its regenerated
  2.3 GiB test target and increased APFS availability by another 2,285,228 KiB.
  A final fresh `/private/tmp` pass removed 29 newly safe candidates, preserved eight active and
  two changed candidates, and increased APFS availability by a further 1,335,132 KiB.
  On the next continuation, 11 more candidates became safe and added 365,312 KiB; eight active
  candidates and one changed candidate again remained untouched.
  A later exact-identity pass removed 66 of 77 candidates representing 3,294,878,422 logical
  bytes. Ten active candidates and one changed or incomplete manifest remained untouched. The
  private journal is `/private/tmp/disksage-dev-permanent-1787954077.jsonl`; concurrent provider
  and build writes mean this logical total is not presented as an APFS free-space increase.
- A later exact-identity `/private/tmp/opencode` pass preserved all 128 registered review
  worktrees, then removed only 146 generated Rust, Python, Node, and analysis-cache roots within
  them. All candidates passed manifest, active-use, and current-object checks; the bounded APFS
  observation increased by 6,487,040 KiB. Source checkouts and Git registrations were untouched.
- The current Podman machine has one running and four stopped PostgreSQL test containers. Its
  native orphan audit proves 74 unreferenced images (about 42.9 GB by record-size sum), but
  `podman system df` and exact stopped-container removal fail because the store contains damaged
  overlay layers, including a missing required lower directory. `podman system check --quick`
  independently confirms the storage inconsistency. DiskSage therefore records no prune or
  physical gain until an explicit native storage-repair plan preserves running-container and
  data-volume dependencies, rechecks integrity, and regenerates the orphan fingerprint.
- That native plan subsequently fingerprinted 129 damaged layers and executed the non-forced
  Podman repair. Fresh postcheck evidence shows 128 repaired and one dependent damaged layer
  retained. Native guest trim then reduced the sparse raw-image allocation from 44,208,422,912
  bytes to about 20.3 GiB; the bounded host observation increased APFS availability by
  22,816,916 KiB. The remaining stopped container and its volumes stay preserved because neither
  exact container removal nor non-forced repair can safely unlink its damaged writable layer.

## 2026-08-29 perceptual photo evidence

- A read-only audit of the user-owned `Pictures` root completed with fingerprint
  `22c1fbfaa1f5bbb06c99ee7d693f40f1c3277b8951be2b3c36aab221daee9518`: 58 entries were
  observed, 45 local photos decoded, one managed Photos Library pruned before descent, zero
  dataless cloud placeholders read, and zero evidence gaps recorded.
- Five distinct-content PNG files formed two exact-aspect-ratio DCT pHash candidate groups. The
  first group contained three 8-bit lossless variants with maximum pairwise Hamming distance 4;
  the 4,408×6,616 member uniquely Pareto-dominated the 2,204×3,308 and 551×827 variants and is shown
  as a review recommendation, not deletion authority. The two lower-resolution encoded files total
  172,074 logical bytes.
- The second group contained two 9,921×14,031, 8-bit lossless variants with maximum pairwise
  Hamming distance 4. Their measured preservation dimensions are equal, so file-size difference is
  not converted into a quality claim and no survivor is recommended. Direct image/metadata review
  and an explicit survivor selection remain required.
- The audit performed no mutation. Quarantine planning cannot begin without one survivor per group;
  execution additionally requires the exact plan phrase, fresh full re-audit, inactive files, and
  unchanged filesystem identities before any non-survivor moves to OS Trash.

## 2026-08-29 Google Drive local-allocation boundary

- A metadata-only audit detected four registered Google Drive File Provider roots: two personal,
  one shared, and one organization root. Two bounded roots completed without evidence gaps; the
  larger personal root remained incomplete after 11,732 entries because 22 entries could not meet
  the metadata policy, and the shared root remained incomplete after 194 entries with eight entry
  errors. No file content was opened or materialized.
- At the 1 MiB allocation floor, all four roots produced zero local-copy candidates and zero
  allocated candidate bytes. At the zero-byte reporting floor, only the incomplete personal scan
  emitted candidates: 59 metadata-sized items totaling 319,488 allocated bytes. This is neither
  meaningful reclaim nor complete-root eviction evidence.
- The provider-generic batch planner rejects the Google Drive root before planning because its
  public Foundation eligibility and postcondition contract has not been verified. DiskSage keeps
  this fail-closed boundary: it emits no eligibility claim or executable fingerprint, and it does
  not broaden the OneDrive/iCloud executor merely because Google Drive uses File Provider. The
  private mode-0700/0600 receipt is identified as
  `disksage-google-drive-live-20260829-e53df609`; no customer path or provider identifier is
  recorded here, and no mutation occurred.
