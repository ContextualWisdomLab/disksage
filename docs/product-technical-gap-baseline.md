# DiskSage product and technical gap baseline

**Snapshot:** 2026-08-30 (Asia/Seoul)

**Protected `main` observed:** `d2eab47dc5a33082938bc269c28faf2b82e326c4`

**Scope:** PRD, accepted ADRs, requested open PRs, and bounded local disk/cloud observations.
**Authority boundary:** this is inventory, not deletion, cloud-write, local-eviction, or merge authority. Mutation still requires fresh item identity, provider evidence, an exact plan fingerprint, required approval, and postconditions. GitHub conclusions apply only to the current PR head.

## Product contract

The contract comes from `docs/product-requirements.md` and ADR-0001 through ADR-0011:

1. Inventory is read-only and metadata-first. Embedded metadata precedes an unambiguous filename token, then filesystem creation/modification metadata. A token such as `2026-04-28` or `251210` is secondary evidence, not ownership, upload, or eviction evidence.
2. Cloud offload follows `copy-verified -> pending-provider-sync -> provider-sync-confirmed -> eviction-ready -> source-evicted`. `local-current` with `is_uploaded=false`, stale/incomplete evidence, active transfer, mismatch, conflict, or unknown state cannot issue an eviction permit.
3. Copy, adoption, and local-copy eviction are bounded and identity-bound. Provider placeholders are not read merely to classify them. Success and failure receipts remain distinct; private path-bearing recovery records are not public exports; mutation is revalidated immediately before execution.
4. Regenerable caches and user data are separate reclaim domains. A cache action requires its implemented identity, active-use, journal, and recovery contract; it is not cloud offload.
5. Deterministic Rust gates own safety. Models may explain or rank already-admissible actions but do not grant mutation authority. OAuth and external services are optional for the standalone desktop product.

## Exact open-PR inventory

`blocked`, `unstable`, `dirty`, and `unknown` are captured GitHub merge states, not source-quality conclusions. A stacked PR is not integrated into `main` merely because its own checks pass.

| PR | Exact head | Base | State | Responsibility still open |
| --- | --- | --- | --- | --- |
| #247 | `629cd15cec2d9c7f154a658b64a86d95b84bdfd5` | `main` | draft, blocked | Explain pending iCloud indexing without granting transfer authority |
| #263 | `0476e434ef63836ab3932b07fc45ec8f66c88265` | `main` | ready, blocked | Fail permanent cache-Trash deletion closed |
| #266 | `6bc2e9ee0f95caf061d987395f46cfe5c8502bd0` | `main` | ready, blocked | Prevent placeholder materialization during ancestor scans |
| #267 | `2bd999aa4cf92f5ee0419cacaafc18ae1b7d1a05` | `main` | ready, blocked | Runtime-agnostic container orphan reclamation |
| #279 | `acf8d948c3eab99c2314f4f53a0ebb664297012e` | `feat/container-orphan-reclaim-runtime-agnostic-v1` | ready, unstable | Reclaim worktrees only with merged/closed exact-head authority |
| #282 | `7a48cc63caab2da7bef16914ec81e578f72e0939` | `feat/merged-worktree-head-authority-v1` | ready, unknown | Repair Podman storage before orphan reclaim |
| #285 | `52acc2c8d707e7a8cb18e85334b3216ad5149136` | `feat/podman-storage-repair-v1` | ready, unstable | Gate native uv-cache pruning |
| #287 | `9894bc732bd0fe701fb509469b07d222ab69f689` | `feat/podman-storage-repair-v1` | ready, dirty | Preserve stopped containers with storage lineage |
| #293 | `395be7143b37330de1c3dd13b0d8928dc651a1da` | `feat/native-uv-cache-reclaim-v1` | ready, unknown | Reclaim inactive Gradle regeneration roots |
| #295 | `c4205504a5d9c0fc937b5e4e5c2cc0912e63b6ce` | `feat/gradle-cache-reclaim-v1` | ready, unstable | Reclaim observed macOS generated caches |
| #298 | `57cf2e868e49f960da70eb14bff4d14e2c5280d7` | `main` | ready, blocked | Bound test-runner disk use |
| #303 | `5af4e9b89f52cc53694e0b203b023c6744004853` | `main` | ready, blocked | Evidence-bound provider-cache reclaim |
| #304 | `f3e46c500f27e8b7728979d676089bd9fdd43a40` | `main` | ready, unknown | Diagnose OneDrive provider-cache pressure without deleting provider state |
| #305 | `b8a0d3a2978325499028e9f1efbaec4576c63a34` | `main` | ready, blocked | Reclaim explicitly identified PostgreSQL test clusters |
| #306 | `2d7baf72a39c1188463438e164c1bfb2696fce2e` | `main` | ready, blocked | Reclaim Python tool caches |
| #308 | `a71d4d78d6969c24aa272ee1371006f92915f67b` | `main` | ready, blocked | Bind release verification to platform artifact namespaces |

PR #258 itself was observed at predecessor `1ee148690aeb56dffe6ef3de972f55b05e33f5d2`, open, non-draft, `BLOCKED`, and `REVIEW_REQUIRED`. Its 2026-08-26 checks are predecessor evidence only. The live GitHub head after this document changes is authoritative.

## Bounded operational observations

These diagnose pressure; none authorizes mutation. Private evidence and receipt locations stay local with restrictive modes. This public baseline records no path, account identifier, or user content name.

| Observation | Bounded result | Consequence |
| --- | --- | --- |
| APFS availability, 2026-08-30 | `110,427,796 KiB` available | The 300 GB product goal is not met. Re-sample before and after any approved action. |
| OneDrive temporary storage, 2026-08-30 | `17,671,028 KiB` allocated; OneDrive processes present | Classify as provider-cache pressure. Age alone cannot authorize deletion. PR #304 owns diagnosis. |
| Latest retained complete iCloud redacted audit | 120 items, 0 eligible local-copy evictions | There is no retained iCloud eviction cohort. Re-run a complete bounded public-Foundation inventory before planning; `is_uploaded=false` remains a veto. |
| Retained OneDrive redacted bounded audit | top-128 cohort allocated `5,272,006,656 B`; not whole-root authority | Historical inventory is not a current plan. Require fresh native state, fingerprint, bounded approval, and postcondition. Temporary storage is not this authority. |
| Retained Google Drive redacted bounded audit | four root types; meaningful candidates `0 B`; incomplete lower-bound scan 59 entries / `319,488 B` | Keep generic eviction fail-closed. This evidence does not justify provider-generic executor expansion. |

## Customer-observable gaps

| Priority | Observable gap | Current evidence | Acceptance proof |
| --- | --- | --- | --- |
| P0 | DiskSage cannot yet demonstrate the requested 300 GB reclaim outcome. | APFS and the open cache/container/worktree/provider queue show independent domains, but no approved aggregate plan exists. | Fresh path-free inventories partition bytes by authority; bounded plans avoid double counting; APFS postchecks report actual reclaimed bytes. |
| P0 | Provider pressure can be visible while safe local-copy eviction is zero or stale. | iCloud retained evidence has 0 eligible items; OneDrive inventory is historical and temporary storage lacks user-file authority; Google Drive has 0 meaningful candidates. | UI distinguishes sync incomplete, provider-cache pressure, local-copy eligibility, and no meaningful reclaim; it shows evidence time and only the next admissible action. |
| P0 | Scanning an ancestor of a provider root may materialize placeholders. | PR #266 remains open. | A fixture proves managed descendants are pruned or queried through a non-materializing capability before regular-file reads. |
| P0 | Container and generated-cache reclaim is fragmented across stacks. | #267/#282/#285/#287/#293/#295 and #303/#305/#306 are not integrated into `main`. | Each domain reports identity, active-use/lineage blockers, dry-run bytes, bounded recovery semantics, journal, and APFS postcheck; bases merge in order. |
| P1 | Merged/closed worktree cleanup is not shipped end to end. | #279 is stacked and unstable. | Re-resolve remote PR state, commit reachability, dirty/untracked state, and worktree identity immediately before bounded removal; uncertainty fails closed. |
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

- `docs/product-requirements.md` is the PRD. Its fail-closed state machine governs this baseline; an open PR title or historical observation does not override it.
- ADR-0001 defines provider evidence and eviction gates; ADR-0002 separates cache reclaim; ADR-0004 bounds maintenance commands; ADR-0006/0007 govern redacted, temporally coherent iCloud evidence.
- ADR-0008 keeps the hourly agent loop read-only across foreign dependencies. ADR-0009 defines path-free lineage exports. ADR-0011 requires durable failed-copy evidence and placeholder-safe adoption.
- Accepted ADR meaning is append-only. Contradictory implementation evidence requires a superseding ADR, not silent rewriting.
- Standards and research citations remain in owning ADR/specification documents. This baseline adds no algorithm, weight, provider guarantee, or speculative architecture, so it makes no new research-adoption claim.

## Loop completion rule

For every row: refresh exact remote head/base; inspect all current threads and required checks; fix verified source defects at the owning boundary; run focused tests; push normally; re-query the new exact head. Merge only when rules, terminal required checks, and qualifying review are satisfied. Waiting, cancelled, stale, or provider-failed checks are evidence to classify, not bypass permission. After merge, refresh dependent stacks and this baseline.
