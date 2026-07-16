# Reviewed fallback production-date evidence

## Problem

DiskSage already ranks production-time evidence as embedded metadata, explicit filename date,
filesystem creation time, then modification time. The copy gate nevertheless rejected every
candidate that lacked an embedded, high-confidence production date, even after an operator had
approved the exact evidence shown in the review UI. That made the fallback ranking unusable for
opaque archives and older files while adding no extra protection after an evidence-bound review.

## Decision

Embedded, high-confidence production time remains the only path that can pass the date gate
without a review decision. A lower-ranked date may pass only when all of the following are true:

1. The planner marks the candidate as requiring review.
2. The append-only decision is valid and has disposition `approved`.
3. The decision candidate fingerprint and review fingerprint match the rebuilt candidate.
4. The normal planner, path, provider, destination, metadata-fingerprint, and copy gates pass.

Held, absent, invalid, mismatched, or stale decisions do not waive the embedded-date gate. The
headless CLI does not accept or load review decisions, so it cannot use fallback dates to copy.

Embedded dates below high confidence receive an explicit
`embedded-production-date-confidence-not-high` review reason. Filename and filesystem fallbacks
already receive `production-date-not-from-embedded-metadata`.

## Safety invariants

- A filename date is never trusted automatically.
- An approval is bound to the source, destination, provider, file identity, selected date,
  confidence, review reasons, and all displayed metadata evidence.
- Replanning occurs before the decision is stored and again before copying.
- Approval changes only review/date eligibility. It cannot bypass a planner block, unsafe path,
  provider mismatch, changed fingerprint, destination collision, copy verification, or provider
  synchronization proof.
- The source deletion API remains absent. Local eviction still requires an immutable copy receipt
  and provider-native synchronization evidence.

## Verification

- Rust unit tests cover approved, absent, and held fallback-date decisions.
- Planner tests cover medium-confidence embedded dates becoming review-required.
- Svelte type checking verifies that the UI mirrors the backend gate.
- Existing stale/tampered decision and provider synchronization tests remain authoritative.
