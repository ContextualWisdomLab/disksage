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
| [0012](0012-inconclusive-provider-and-reclaim-presentation.md) | Separate inconclusive provider evidence, reclaim attribution, and customer guidance | Accepted |

New records must state context, decision, consequences, rejected alternatives, and the evidence or
standard that led to the decision. A record never grants cloud-write or source-eviction authority;
those remain bound to current Rust evidence and explicit approvals.
