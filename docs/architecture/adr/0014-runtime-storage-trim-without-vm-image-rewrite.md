# ADR-0014: Trim guest extents without rewriting Podman or Colima VM images

**Status**: Accepted  
**Date**: 2026-08-28  
**Scope**: `src-tauri/src/runtime_storage.rs`, Tauri commands, headless CLI, Cleanup screen

The desktop dispatches bounded trim and recovery subprocess waits through Tauri's blocking-task
pool so a slow guest operation cannot occupy an asynchronous command worker.

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
6. The standalone `disksage-runtime-storage` CLI calls the same Rust planner and executor as the
   desktop app. It does not add a second mutation implementation: inspection is the default, and
   execution requires the current exact phrase plus a rationale. It probes only the selected
   runtime, and its bounded stdout and stderr are part of the versioned headless JSON receipt.
   The desktop serialization omits those diagnostic streams so local paths and runtime details do
   not cross into the frontend.
7. The Podman reclaim plan binds the exact machine and backing-file identity, active-container
   count, rollback policy, and restart policy before considering host compaction. Podman 5.8 has no
   runtime-native compact or shrink command, so the current plan is read-only, publishes no stop or
   compaction command, and issues no approval phrase. A future executor requires zero active
   containers plus a runtime-native operation with rollback; raw-image tools remain prohibited.
8. A backing file's stable identity (device and inode) is distinct from mutable freshness evidence
   (length, allocation, and modification time). Future execution must bind and revalidate both;
   ordinary guest writes must not be misrepresented as a different backing file.

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
- CLI process tests verify that runtime selection does not invoke the unselected runtime, command
  streams survive bounded receipt serialization, and execution authority is all-or-nothing.
- Podman plan tests keep host compaction blocked without a native operation, even when no container
  is active, and bind backing-file identity without relying on a pathname alone.
- Recovery and trim use distinct approvals; reachability is included in the plan fingerprint so a
  stale recovery or trim plan cannot authorize execution after guest state changes.
- The existing container-orphan and Podman planners remain the authority for exact image,
  volume, network, and container candidates; this ADR adds no alternate deletion path.
