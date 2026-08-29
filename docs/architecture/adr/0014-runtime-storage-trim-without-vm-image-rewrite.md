# ADR-0014: Trim guest extents without rewriting Podman or Colima VM images

**Status**: Accepted
**Date**: 2026-08-28
**Scope**: `src-tauri/src/runtime_storage.rs`, Tauri commands, Cleanup screen

## Context

The host can be full even when Podman or Colima reports reclaimable logical bytes. A VM-backed
runtime keeps its own filesystem and may use a sparse disk image, so logical `system df` values are
not proof of host allocation. Rewriting or compacting a raw VM image while the runtime is active
could corrupt running workloads and data-bearing volumes.

## Decision

1. DiskSage exposes a read-only plan for the Podman machine and Colima independently. The plan
   records executable availability, running state, guest reachability, and a deterministic
   approval phrase.
2. After a fresh plan and explicit rationale, DiskSage may run only the fixed guest command
   `sudo fstrim -av` through `podman machine ssh` or `colima ssh`. The command is bounded and its
   output is returned as a receipt; no user path or image bytes are accepted as input.
3. If a running guest is unreachable, trim remains blocked. A separate, explicitly approved
   recovery action may run only the runtime-native stop/start sequence, warns that running work
   can be interrupted, and must prove guest reachability again before trim becomes available.
4. Host-image compaction is reported as unsupported unless a future runtime-native, integrity-
   checked API is added. DiskSage never invokes `qemu-img`, deletes a VM image, stops a runtime,
   or removes a volume as part of trim.
5. Trim captures host-volume observations immediately before and after execution. The UI reports
   only the measured available-space change and does not infer that all of the change came from
   trim.

## Consequences

- Users can reclaim guest filesystem extents without risking active VM images.
- The UI distinguishes “게스트 정리 완료” from host-image compression and never promises 300 GB
  when current measurements do not support it.
- Raw-image compaction remains an explicit external maintenance task until a provider-supported
  operation can be verified and bound to an approval record.

## Rejected alternatives

- Rewriting or truncating Podman/Colima raw images: rejected because active stores and sparse
  extents cannot be proven safe from a path or file-size snapshot.
- `system prune --volumes` and category-wide deletion: rejected; named volumes may contain
  databases and are handled only by the identity-bound orphan planner.

## Evidence

- Rust tests verify fixed command construction and fail-closed unavailable-runtime plans.
- Recovery and trim use distinct approvals; reachability is included in the plan fingerprint so a
  stale recovery or trim plan cannot authorize execution after guest state changes.
- The existing container-orphan and Podman planners remain the authority for exact image,
  volume, network, and container candidates; this ADR adds no alternate deletion path.
