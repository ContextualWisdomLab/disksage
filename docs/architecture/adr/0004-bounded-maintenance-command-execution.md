# ADR-0004: Bound fixed maintenance command execution

**Status:** Accepted
**Date:** 2026-08-20

## Context

DiskSage can execute the fixed macOS command `brew cleanup --prune-prefix` only
after a bounded dry-run, local-model judgment, fast-mlsirm calibration, and
separate human confirmation. A timeout in the wrapper must not leave Homebrew
or one of its descendants running after the approval has failed, and command
output must remain bounded.

## Decision

Run the verified Homebrew wrapper in a private Unix process group. On timeout
or wait failure, terminate the entire group, reap the direct child, and retain
only the bounded stdout/stderr evidence. The executable path, arguments, plan
fingerprint, model judgment, calibration result, approval phrase, and audit
record remain independently validated; no model output can provide a command
or path.

## Consequences

- A stalled maintenance command cannot continue after its execution gate fails.
- The source tree and cloud-provider state are unaffected by this local-only
  maintenance action.
- A timeout is recorded as failure; it is never treated as successful cleanup.
- The private process-group setup is macOS-specific and must remain covered by
  the macOS Release build.

## Evidence

- `src-tauri/src/brew_cleanup.rs` uses a fixed executable/argument set,
  bounded output readers, a 120-second deadline, and process-group termination.
- `brew_cleanup::tests` verifies fixed arguments, bounded output, judgment and
  calibration gates, audit-record privacy, and executable identity binding.
