# Cloud metadata review decisions

## Goal

Let an operator inspect the embedded metadata already shown in the cloud archive preview and record
an explicit `approved` or `held` decision for candidates that require review. The workflow remains
copy-only; it does not expose source deletion.

## Safety invariants

- A filename date remains low-confidence context and cannot satisfy the embedded high-confidence
  production-date gate.
- Approval clears only `review-required`. Planner, path, provider, destination-conflict, and
  embedded-date gates remain mandatory.
- A decision is bound to both the candidate metadata fingerprint and a second review fingerprint
  covering the provider, destination, production-time evidence, title/authors/context, dataset
  profile, metadata evidence, and review reasons shown to the operator.
- Any change to that evidence makes the previous decision stale.
- The command rebuilds the plan before accepting a decision, so a stale UI cannot approve changed
  evidence.
- Decision writes and review-gated copies share an application lock. A concurrent `held` decision
  cannot race an already-read approval.
- Decision files are append-only, integrity-bound, read-only JSON records. They contain hashes,
  disposition, and time only; no file path or metadata value is persisted.
- The existing CLI has no review override and therefore continues to reject review-required
  candidates.

## Flow

1. The planner probes bounded embedded metadata and computes `review_fingerprint`.
2. The UI displays the evidence and review reasons.
3. The operator chooses approve or hold.
4. The backend rebuilds the plan and requires both fingerprints to match before appending a record.
5. A copy rebuilds the plan again, loads the latest immutable decision, and runs every blocker.
6. Copy receipts and provider synchronization attestation remain unchanged; the source stays local.

## Verification

- Pure tests prove evidence changes expire an approval.
- Gate tests prove `held`, stale, and invalid decisions block copying.
- Gate tests prove an approval cannot make a filename-derived production date eligible.
- Persistence tests prove immutable round trips, latest-decision selection, and integrity failure.
- Rust all-target checks and frontend type checks cover command/API wiring.
