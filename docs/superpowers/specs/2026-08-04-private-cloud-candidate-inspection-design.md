# Private cloud candidate inspection dossier

## Problem

The exact-reason private review dossier intentionally accepts only `review-required` candidates.
That is correct for attributed review decisions, but it leaves `blocked` candidates without a safe
inspection path. Printing the full plan exposes local paths and raw embedded metadata to terminal
logs, while excluding blocked candidates prevents a metadata-first operator from understanding why
large multipart archives, incomplete downloads, or confidential containers are held.

## Interface

`disksage-cloud-plan --decision-summary --private-candidate-inspection-output
/absolute/new-file.json` performs one fresh, single-destination plan.

- Standard output remains the redacted decision summary.
- The private output includes every current plan candidate, including `blocked`,
  `review-required`, and `ready-for-copy-review` states.
- Each candidate preserves its full path, raw embedded metadata evidence, production-time source and
  confidence, fingerprints, destination, blockers, and review reasons.
- Production-time precedence remains embedded metadata, explicit filename date, filesystem creation
  time, then modification time. Filename dates remain auxiliary evidence.
- The output uses create-new semantics and mode `0600` on Unix. Unsupported permission enforcement
  fails closed.
- The flag is rejected for multicloud summaries, relative output paths, exact review subsets, and
  every mutation or export action.

## Safety boundary

The dossier is inspection evidence only. It cannot create an attributed review decision, prepare a
copy receipt, write to a cloud provider, or authorize source eviction. The public summary reports
only its hash, byte count, mode, create-new status, inclusion of blocked candidates, and explicit
non-approval/non-mutation claims.

## Verification

Tests must prove parser and action-gate exclusivity, absolute create-new private output, inclusion of
both blocked and review-required candidates, production-time precedence disclosure, sensitive path
retention only inside the private dossier, and explicit false cloud-write/source-eviction claims.
