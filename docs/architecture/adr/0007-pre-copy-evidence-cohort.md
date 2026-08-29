# ADR-0007: Gate iCloud copy plans on a fresh evidence cohort

**Status:** Accepted
**Date:** 2026-08-20

## Context

ADR-0006 made local-volume, provider-runtime, and iCloud-health observations
durable, but separate records could still be mistaken for one current
observation. A plan assembled from a stale or incomplete stream must not look
ready merely because the other streams are fresh.

## Decision

For iCloud plans, DiskSage creates one path-free `pre_copy_evidence` cohort
from the three streams. Every observation must have a known stream name, a
non-zero Unix timestamp, a 64-character hexadecimal fingerprint, and
`evidence_complete=true`. The cohort is complete only when the observation
timestamps are within five minutes of one another; otherwise the plan records
stable blockers and remains fail-closed. The cohort fingerprint covers the
canonical stream order, observations, freshness result, and blockers.

This is a freshness and integrity gate, not a cloud receipt or per-item upload
attestation. It never grants cloud-write, source-eviction, or remote-capacity
authority. Provider-native per-item evidence and the existing human approval
remain mandatory.

The path-free Naruon readiness envelope carries this cohort and an explicit
`pre_copy_evidence_met` binding (schema version 8). A Naruon consumer therefore
cannot treat a quiet provider queue or a missing cohort as copy readiness.

## Consequences

### Positive

- A restart or delayed planning loop cannot silently combine unrelated
  observations.
- The UI can explain whether copy admission is blocked by missing, incomplete,
  malformed, or skewed evidence without exposing paths or provider internals.
- The cohort is deterministic and cheap to test in Rust.

### Negative

- A temporarily unavailable provider stream blocks a new iCloud copy until a
  fresh bounded probe succeeds.
- Five minutes is a conservative freshness ceiling; a future deployment may
  tune it only with new evidence and an ADR revision.

## Rejected alternatives

- **Use the newest stream only:** rejected because it hides gaps in the other
  required observations.
- **Treat timestamps as attestation:** rejected because timestamps do not prove
  a cloud object exists or that its bytes are remotely durable.
- **Persist raw provider output for correlation:** rejected by ADR-0006's
  privacy and disk-pressure boundary.

## Evidence basis

The fail-closed, least-privilege boundary follows Saltzer and Schroeder's
secure-design principles and NIST SP 800-53 Rev. 5 controls. The citations are
maintained in the cloud-offload design note and are reproduced here for the
decision record:

- Saltzer, J. H., & Schroeder, M. D. (1975). The protection of information in
  computer systems. *Proceedings of the IEEE, 63*(9), 1278–1308.
  https://doi.org/10.1109/PROC.1975.9939
- Joint Task Force. (2020). *Security and privacy controls for information
  systems and organizations* (NIST SP 800-53 Rev. 5, Release 5.2.0, 2025).
  https://doi.org/10.6028/NIST.SP.800-53r5

## Related decisions

- [ADR-0001](0001-cloud-offload-goal-state.md) — provider evidence and
  fail-closed eviction gates.
- [ADR-0005](0005-hourly-agent-loop-is-advisory.md) — advisory agent loops.
- [ADR-0006](0006-redacted-icloud-health-evidence.md) — bounded redacted
  iCloud evidence streams.
