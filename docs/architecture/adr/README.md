# DiskSage Architecture Decision Records

These records document decisions that affect cloud-offload safety, evidence provenance, and
operational automation. Accepted records are append-only in meaning: superseding decisions use a
new numbered record rather than rewriting history.

| ADR | Decision | Status |
| --- | --- | --- |
| [0001](0001-cloud-offload-goal-state.md) | Provider evidence drives the cloud-offload Goal | Accepted |
| [0002](0002-cache-cleanup-is-per-item-evidence-bound.md) | Cache cleanup is per-item evidence-bound | Accepted |
| [0003](0003-zotero-local-api-metadata-handoff.md) | Zotero Local API metadata handoff | Accepted |
| [0004](0004-bounded-maintenance-command-execution.md) | Bounded maintenance command execution | Accepted |
| [0005](0005-hourly-agent-loop-is-advisory.md) | Hourly agent loop is advisory | Superseded by 0008 |
| [0006](0006-redacted-icloud-health-evidence.md) | Persist redacted iCloud health evidence | Accepted |
| [0007](0007-pre-copy-evidence-cohort.md) | Gate iCloud plans on a fresh evidence cohort | Accepted |
| [0008](0008-hourly-loop-foreign-dependencies-read-only.md) | Keep the hourly loop read-only at foreign dependency boundaries | Accepted |
| [0009](0009-path-free-lineage-relation-graph.md) | Export a path-free lineage relation graph | Accepted |
| [0010](0010-rooted-organize-destinations.md) | Require rooted, process-independent organize destinations | Accepted |
| [0011](0011-cloud-transfer-failure-and-materialization.md) | Durable failed-copy evidence and placeholder-safe adoption | Accepted |
| [0012](0012-container-orphan-reclaim-runtime-agnostic.md) | Runtime-agnostic container orphan reclamation is identity-bound and fail-closed | Accepted |
| [0013](0013-closed-pull-request-worktree-authority.md) | Bind closed pull-request worktree cleanup to forge evidence | Accepted |
| [0014](0014-runtime-storage-trim-without-vm-image-rewrite.md) | Trim guest extents without rewriting VM images | Accepted |
| [0015](0015-explicit-cutoff-open-pull-request-worktree-authority.md) | Require an explicit cutoff for stale open pull-request worktrees | Accepted |
| [0016](0016-shared-temporary-storage-ownership-bound.md) | Bound `/tmp` cleanup to current-user-owned trees | Accepted |
| [0017](0017-standalone-stale-pr-clone-authority.md) | Require exact-head authority for standalone stale-PR clones | Accepted |
| [0018](0018-permanent-generated-artifact-failure-safety.md) | Retain failed permanent artifact deletions in private staging | Accepted |
| [0019](0019-macos-file-provider-local-eviction.md) | Use each macOS File Provider domain for local-only eviction | Accepted |
| [0020](0020-podman-native-storage-repair.md) | Use machine-scoped native Podman storage repair without force | Accepted |
| [0021](0021-perceptual-photo-candidates.md) | Require measured evidence and a selected survivor for perceptual photo candidates | Accepted |
| [0022](0022-photo-duplicate-evidence-without-composite-scoring.md) | Separate photo-duplicate and keeper evidence without composite scoring | Accepted |
| [0023](0023-apple-photos-photokit-boundary.md) | Use PhotoKit rather than Photos library package traversal | Accepted |
| [0024](0024-photokit-checkpointed-inventory.md) | Checkpoint PhotoKit inventory at native completion boundaries | Accepted |

New records must state context, decision, consequences, rejected alternatives, and the evidence or
standard that led to the decision. A record never grants cloud-write or source-eviction authority;
those remain bound to current Rust evidence and explicit approvals.
