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
| [0005](0005-hourly-agent-loop-is-advisory.md) | Hourly agent loop is advisory | Accepted |
| [0006](0006-redacted-icloud-health-evidence.md) | Persist redacted iCloud health evidence | Accepted |

New records must state context, decision, consequences, rejected alternatives, and the evidence or
standard that led to the decision. A record never grants cloud-write or source-eviction authority;
those remain bound to current Rust evidence and explicit approvals.
