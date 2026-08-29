# DiskSage product and technical gap baseline

**Snapshot:** 2026-08-30 (Asia/Seoul)

**Protected `main` observed:** `d2eab47dc5a33082938bc269c28faf2b82e326c4`

**Scope:** PRD, accepted ADRs, requested open PRs, and bounded local disk/cloud observations.
**Authority boundary:** this is inventory, not deletion, cloud-write, local-eviction, or merge authority. Mutation still requires fresh item identity, provider evidence, an exact plan fingerprint, required approval, and postconditions. GitHub conclusions apply only to the current PR head.

## Product contract

The integrated contract comes from `docs/product-requirements.md` and ADR-0001 through ADR-0011. PR #315 proposes `docs/PRD.md` as the canonical contract; it remains non-authoritative until that PR is integrated:

1. Inventory is read-only and metadata-first. Embedded metadata precedes an unambiguous filename token, then filesystem creation/modification metadata. A token such as `2026-04-28` or `251210` is secondary evidence, not ownership, upload, or eviction evidence.
2. Cloud offload follows `copy-verified -> pending-provider-sync -> provider-sync-confirmed -> eviction-ready -> source-evicted`. `local-current` with `is_uploaded=false`, stale/incomplete evidence, active transfer, mismatch, conflict, or unknown state cannot issue an eviction permit.
3. Copy, adoption, and local-copy eviction are bounded and identity-bound. Provider placeholders are not read merely to classify them. Success and failure receipts remain distinct; private path-bearing recovery records are not public exports; mutation is revalidated immediately before execution.
4. Regenerable caches and user data are separate reclaim domains. A cache action requires its implemented identity, active-use, journal, and recovery contract; it is not cloud offload.
5. Deterministic Rust gates own safety. Models may explain or rank already-admissible actions but do not grant mutation authority. OAuth and external services are optional for the standalone desktop product.

## Exact open-PR inventory

`blocked`, `unstable`, `dirty`, and `unknown` are captured GitHub merge states, not source-quality conclusions. A stacked PR is not integrated into `main` merely because its own checks pass.

| PR | Exact head | Base | State | Responsibility still open |
| --- | --- | --- | --- | --- |
| #247 | `ff70f8159b82f05593c2ae7611ab3a5229ae886f` | `main` | draft, blocked | Explain pending iCloud indexing without granting transfer authority |
| #263 | `adf22e96837377d279d1bf9900e63ebfebce27ef` | `main` | ready, blocked | Keep permanent cache-Trash deletion unavailable and exclude provider-managed cache roots |
| #266 | `fa463566759ba8943d435b05bbed6aff6b896cc0` | `main` | ready, blocked | Prevent placeholder materialization and withhold rejected provider roots from cleanup flows |
| #267 | `392f07aea31ed4e74e9385497f36565944c0e448` | `main` | ready, blocked | Runtime-agnostic container orphan reclamation |
| #282 | `7be69db1a46c3cd6cc8cd94b5c103f48336e5c7a` | `feat/container-orphan-reclaim-runtime-agnostic-v1` | ready, unstable | Repair Podman storage before orphan reclaim |
| #285 | `e49e07c63d148fa7b96cdb766d14dc05334f94a4` | `feat/podman-storage-repair-v1` | ready, unstable | Gate native uv-cache pruning |
| #287 | `25dbb2095c9a1a30da5dc8fc2ef3e37bb2cdd987` | `feat/podman-storage-repair-v1` | ready, unstable | Preserve stopped containers with storage lineage |
| #293 | `58d7e6e65cf9bcfca58ae9a24b759224ac6a3a34` | `feat/native-uv-cache-reclaim-v1` | ready, dirty | Reclaim inactive Gradle regeneration roots |
| #295 | `9ac7cea769aac0cb19b9e9c7ee299e69385563bf` | `feat/gradle-cache-reclaim-v1` | ready, unstable | Reclaim observed macOS generated caches |
| #298 | `57cf2e868e49f960da70eb14bff4d14e2c5280d7` | `main` | ready, blocked | Bound test-runner disk use |
| #303 | `6593d8ad42483ad69a79ff7c5e735afd1fc33c55` | `main` | ready, blocked | Evidence-bound provider-cache reclaim |
| #304 | `f3e46c500f27e8b7728979d676089bd9fdd43a40` | `main` | ready, blocked | Diagnose OneDrive provider-cache pressure without deleting provider state |
| #305 | `f28d771eeb7879147944e0829bdb1bb2b45e792e` | `main` | ready, blocked | Reclaim explicitly identified PostgreSQL test clusters |
| #306 | `bb22a5b3683227477e4606d2c1a45c129efbe333` | `main` | ready, blocked | Reclaim Python tool caches while excluding provider-managed roots |
| #308 | `67306e9c262d76d721edde8edf2c38a96e125956` | `main` | ready, blocked | Bind release verification to platform artifact namespaces |
| #309 | `30d3996b679cf2c9b4b2be0821f1f6b962e3910a` | `main` | ready, blocked | Reclaim selected development build roots |
| #310 | `9f8db866f617ad85a35640151276af524ccc51dc` | `main` | ready, blocked | Add read-only, fail-closed Colima disk reclaim planning |
| #311 | `9698b4527a7387f362af267a72d660c060eeedf0` | `feat/gradle-cache-reclaim-v1` | ready, unstable | Plan and Trash standalone clones only with closed/merged exact authority |
| #312 | `acde6bdf28524728bd06b1da1b367022187b61c9` | `main` | ready, blocked | Add read-only, fail-closed planning for stopped Parallels VM disk reclaim |
| #313 | `09d97f80d0478164a0b9f23c001be5fda88571b0` | `main` | ready, blocked | Group exact decoded PNG pixels and expose only a uniquely Pareto-dominant keeper; calibrated near-duplicate/IQA evidence and execution remain unavailable |
| #314 | `e9bc0a999ac608822950cb955cc16e47f64b5eea` | `main` | ready, blocked | Reclaim admitted generated build roots from native temporary storage with evidence-bound planning and reversible execution |
| #315 | `ebf0d8caaca0db1c59df676e96f7e00378400855` | `main` | ready, blocked | Establish `docs/PRD.md` as the canonical product contract and link live implementation traceability back to this baseline |

PR #258 itself was observed before this edit at `135139ac73bb0d02468cb02f5baeb735cd08f37c`, open, non-draft, and `BLOCKED`. The live GitHub head after this document changes is authoritative.

## Bounded operational observations

These diagnose pressure; none authorizes mutation. Private evidence and receipt locations stay local with restrictive modes. This public baseline records no path, account identifier, or user content name.

| Observation | Bounded result | Consequence |
| --- | --- | --- |
| APFS global sample, 2026-08-30 05:09:59 KST | Data volume `795,156,896 KiB` used and `111,031,524 KiB` available | This shared-volume sample fluctuates with concurrent activity and is not reclaim attribution. The 300 GB product goal remains unmet; re-sample immediately before and after each approved action. |
| Evidence-gated registered-worktree cleanup, 2026-08-30 | NUL-safe inventory decreased from 45 to 40 after five exact closed/merged-head worktrees passed clean tracked/untracked, inactive, non-default, and no-unique-commit gates; removed worktrees accounted for `45,808 KiB` allocated (`13,820,218 B` tracked logical), with branches preserved | These allocated bytes are action-attributable. The much larger concurrent APFS availability change is not. Productize the same fresh remote-head and filesystem-identity gates before offering this action. |
| Generated build cleanup, 2026-08-30 | A bounded `cargo clean` reported `2.0 GiB` of regenerated outputs removed after validation; ignored generated CodeGraph/UI dependencies and the disposable validation worktree were then removed | Command-reported build output is task evidence; global APFS deltas are only contemporaneous samples and must not be added to it as independent reclaim. |
| OneDrive temporary storage, 2026-08-30 | `17,671,028 KiB` allocated; OneDrive processes present | Classify as provider-cache pressure. Age alone cannot authorize deletion. PR #304 owns diagnosis. |
| Current iCloud native observation, 2026-08-30 | Sync remains `needs-sync`; the latest retained complete redacted audit has 120 items and 0 eligible local-copy evictions | Do not copy or evict. Let iCloud finish, then re-run a complete bounded public-Foundation inventory; `is_uploaded=false` remains a veto. |
| Retained OneDrive redacted bounded audit | top-128 cohort allocated `5,272,006,656 B`; not whole-root authority | Historical inventory is not a current plan. Require fresh native state, fingerprint, bounded approval, and postcondition. Temporary storage is not this authority. |
| Retained Google Drive redacted bounded audit | four root types; meaningful candidates `0 B`; incomplete lower-bound scan 59 entries / `319,488 B` | Keep generic eviction fail-closed. This evidence does not justify provider-generic executor expansion. |

## Customer-observable gaps

| Priority | Observable gap | Current evidence | Acceptance proof |
| --- | --- | --- | --- |
| P0 | DiskSage cannot yet demonstrate the requested 300 GB reclaim outcome. | APFS and the open cache/container/worktree/provider queue show independent domains, but no approved aggregate plan exists. | Fresh path-free inventories partition bytes by authority; bounded plans avoid double counting; APFS postchecks report actual reclaimed bytes. |
| P0 | Provider pressure can be visible while safe local-copy eviction is zero or stale. | iCloud retained evidence has 0 eligible items; OneDrive inventory is historical and temporary storage lacks user-file authority; Google Drive has 0 meaningful candidates. | UI distinguishes sync incomplete, provider-cache pressure, local-copy eligibility, and no meaningful reclaim; it shows evidence time and only the next admissible action. |
| P0 | Scanning an ancestor of a provider root may materialize placeholders. | PR #266 remains open. | A fixture proves managed descendants are pruned or queried through a non-materializing capability before regular-file reads. |
| P0 | Container and generated-cache reclaim is fragmented across stacks. | #267/#282/#285/#287/#293/#295 and #303/#305/#306 are not integrated into `main`. | Each domain reports identity, active-use/lineage blockers, dry-run bytes, bounded recovery semantics, journal, and APFS postcheck; bases merge in order. |
| P0 | Virtual-machine disk reclaim is incomplete. | Podman work is split across #267/#282/#287; #310 provides read-only Colima planning and #312 provides read-only, fail-closed stopped-Parallels-VM planning. Neither open PR is integrated, and compaction execution plus recovery/post-action receipts remain unavailable. | Each runtime first proves ownership, stopped/idle state, sparse-image identity and supported native compaction semantics; dry-run and post-action physical-byte evidence remain runtime-specific. |
| P0 | Photo quality-aware duplicate cleanup is incomplete. | #313 implements exact decoded-pixel PNG grouping and displays a keeper only when one member uniquely Pareto-dominates on separate losslessness, source bit-depth, metadata-completeness, and lineage evidence. Perceptual near-duplicate grouping and no-reference IQA remain unavailable without calibrated, checksummed artifacts; cleanup execution is unavailable. | A calibrated descriptor/IQA artifact records provenance and checksum; ambiguous Pareto groups require customer selection; an exact group/keeper identity, fresh approval, reversible Trash journal, and undo proof precede any execution. |
| P1 | Merged/closed worktree and standalone-clone cleanup are not shipped end to end. | A bounded operational run safely removed 5 of 45 registered worktrees (`45,808 KiB` allocated), but the capability is not on protected `main`; standalone clone lifecycle #311 is stacked and unstable. | Re-resolve remote PR state, default branch, commit reachability, dirty/untracked state, active use and filesystem identity immediately before Trash staging; uncertainty fails closed. |
| P1 | `/tmp` and tool-cache reclaim is only partially integrated. | #314 implements evidence-bound reclaim for admitted generated build roots in native temporary storage. Clone/worktree, nonce-scoped, and general temporary-file adapters remain unavailable; #263/#293/#295/#306/#309/#314 are open and therefore not integrated into protected `main`. | Each adapter identifies its owning producer and regeneration contract; current identity, provider exclusion, active-use evidence, exact approval, Trash journal/undo, and APFS postcheck are revalidated on the final integrated head. Unknown general temporary content remains review-only. |
| P1 | Release evidence is not uniformly namespace-bound. | #308 remains blocked. | Verify every platform artifact, checksum, attestation, and help contract against its exact namespace and source head. |
| P1 | Repository-wide 100% coverage/docstring/edge claims are not evidenced. | No complete measured report is present. | Publish per-language executable measurements and claim only measured scopes. |

## Technical and operational gaps

| Priority | Gap | Smallest next proof |
| --- | --- | --- |
| P0 | No fresh aggregate reclaim plan is bound to current disk/provider observations. | Produce complete independent inventories; deduplicate extents/candidates; fingerprint bounded plans; revalidate at mutation. |
| P0 | Provider temporary storage has no safe generic deletion authority. | Use provider-supported ownership/status evidence. Never infer staleness from age; add an action only with a proven provider contract and postcondition. |
| P0 | Stacked branches can contradict one another until bases integrate. | Process base-first: current-head review, verified fixes, terminal required checks, qualifying review, normal merge, then refresh the next head. |
| P1 | APFS observations are not automatically comparable pre/post receipts. | Record path-free volume snapshots with timestamp, filesystem identity, allocated-byte method, plan fingerprint, and post-action delta. |
| P1 | Runtime UI evidence is weaker than fixture coverage for provider edge states. | Capture bounded signed-artifact E2E receipts without hydrating placeholders or exposing paths. |
| P2 | Optional ontology/agent/catalog integrations can drift beyond measured need. | Keep adapters over path-free receipts and relations; add a dependency only with a consumer, failure contract, and standalone-offline test. |

## PRD, ADR, and research consistency

- [`docs/product-requirements.md`](product-requirements.md) remains the integrated PRD. [PR #315](https://github.com/ContextualWisdomLab/disksage/pull/315) proposes canonical `docs/PRD.md`; until integration, its exact head is inventory evidence rather than mutation authority.
- [ADR-0001](architecture/adr/0001-cloud-offload-goal-state.md) defines exact-copy evidence and sync-gated eviction; [ADR-0002](architecture/adr/0002-cache-cleanup-is-per-item-evidence-bound.md) separates cache reclaim; [ADR-0004](architecture/adr/0004-bounded-maintenance-command-execution.md) bounds maintenance commands; [ADR-0006](architecture/adr/0006-redacted-icloud-health-evidence.md) and [ADR-0007](architecture/adr/0007-pre-copy-evidence-cohort.md) govern redacted, temporally coherent iCloud evidence.
- [ADR-0008](architecture/adr/0008-hourly-loop-foreign-dependencies-read-only.md) keeps the hourly agent loop read-only across foreign dependencies. [ADR-0009](architecture/adr/0009-path-free-lineage-relation-graph.md) defines path-free lineage exports. [ADR-0011](architecture/adr/0011-cloud-transfer-failure-and-materialization.md) requires durable failed-copy evidence and placeholder-safe adoption.
- Accepted ADR meaning is append-only. Contradictory implementation evidence requires a superseding ADR, not silent rewriting.
- Standards and APA 7th research citations remain in owning ADR/specification documents. PR #313's ADR separates exact decoded-pixel identity and Pareto evidence from unavailable calibrated perceptual/IQA artifacts; this baseline introduces no threshold, composite weight, provider guarantee, or speculative execution authority.

## Loop completion rule

For every row: refresh exact remote head/base; inspect all current threads and required checks; fix verified source defects at the owning boundary; run focused tests; push normally; re-query the new exact head. Merge only when rules, terminal required checks, and qualifying review are satisfied. Waiting, cancelled, stale, or provider-failed checks are evidence to classify, not bypass permission. After merge, refresh dependent stacks and this baseline.
