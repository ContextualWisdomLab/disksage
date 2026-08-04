# Podman reclaim evidence design

## Problem

A Podman machine can report a fixed 100 GiB virtual disk while the host raw image is either sparse
or almost fully allocated. Guest deletion is not host reclaim proof: discarded guest blocks remain
allocated on the host until the storage stack propagates TRIM/discard. A full guest can also prevent
Podman's API socket and diagnostic log from starting, so a failed API probe must not be mistaken for
an absent machine.

## Safety contract

- The feature is read-only. It never prunes images, removes containers or volumes, vacuums logs,
  runs `fstrim`, stops a machine, or changes its configured disk size.
- External commands are argv-based (never shell strings), capture bounded output, and have a bounded
  timeout. Standard output and standard error are drained concurrently so a full operating-system
  pipe cannot deadlock the child process. At most 1 MiB from each stream is retained; excess bytes
  are discarded while draining and make the probe fail closed with a stable output-too-large code.
  Timeout and wait-error paths kill and reap the child, then join both reader threads before return.
- The raw image must be an absolute regular file and must not be a symbolic link.
- Podman-reported guest or store reclaimable bytes are not host physical reclaim proof.
  `physically_reclaimable_bytes` remains `null` until a separate before/after filesystem observation
  proves released host blocks.
- Suggested actions are advisory and explicitly require human approval.

## Evidence model

`schema_kind: disksage.podman-reclaim-plan`, schema version 1, records:

1. machine state and configured logical capacity from `podman machine inspect`;
2. the configured raw image's logical and observed allocated bytes;
3. guest total, used, and available bytes from a read-only `df` probe over `podman machine ssh`;
4. API/store counts and graph-root accounting from `podman info --format json`;
5. stable issue and reason codes, elapsed time, and whether the evidence is complete.

The raw-allocation-minus-guest-used gap is an observation only. It can justify recommending a TRIM
review but never becomes `physically_reclaimable_bytes`.

## Verification contract

The subprocess boundary has deterministic regressions for simultaneous stdout/stderr output larger
than a typical pipe buffer, output one byte beyond the retained limit, and timeout cleanup. These
checks complement the full Rust, security, SAST, and cross-platform release gates on the exact PR
head.

## Integration boundary

The computation and parsing stay in Rust and are exposed through a standalone JSON CLI and a Tauri
command rendered in Cleanup. This is deterministic local evidence collection, so Noema,
contextual-orchestrator, semantic-data-portal, pg-erd-cloud, and fast-mlsirm are not required for
this slice.
