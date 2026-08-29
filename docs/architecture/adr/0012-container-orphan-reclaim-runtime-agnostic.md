# ADR-0012: Runtime-agnostic container orphan reclamation is identity-bound and fail-closed

- Status: Accepted
- Date: 2026-08-26
- Scope: `src-tauri/src/container_orphan_reclaim.rs`, Tauri commands
  `inspect_container_orphans` / `execute_container_orphan_prune`,
  CLI `disksage-container-orphan-plan`, Cleanup screen panel.

## Context

DiskSage already audits Podman guest/raw allocation evidence (ADR-0004 lineage of bounded
maintenance execution), but its only executable container boundary prunes dangling Podman
images. Users on Docker Desktop, Colima, and mixed Docker/Podman setups had no equivalent,
and none of the four orphan categories (stopped containers, unreferenced images, dangling
volumes, unused custom networks) was auditable uniformly. Deleting any of these resources is
irreversible — there is no Trash for a container engine store — so the same fail-closed,
identity-bound discipline that governs worktree removal and cache cleanup must apply.

## Decision

1. One engine covers three runtime targets: plain `docker` (native context),
   `docker --context colima` (Colima-managed socket), and
   `podman --connection <machine>` (running machine). Scope names are validated to reject
   option injection (`unsafe-runtime-scope-name`).
2. The audit pass is read-only, wall-clock-bounded, output-capped, and tolerant of both
   NDJSON (Docker) and JSON-array (Podman) envelopes. Any malformed record, unknown
   container state, missing reference count, or oversized listing fails the category closed.
3. Candidates are strictly defined:
   - containers in `exited`/`created`/`dead` states only, and only when a fresh native container
     inspection proves the `Mounts` array exists and is empty. A storage mount preserves the
     container as lifecycle lineage for its data; an inspect failure or malformed/missing mount
     evidence fails the entire container category closed;
   - untagged images with proven zero container references (tagged images are never
     candidates even when unused);
   - dangling volumes reported by the runtime's own `dangling=true` filter;
   - custom networks excluding built-ins (`bridge`, `host`, `none`, `podman`) whose inspect
     proves zero attached endpoints, bounded at 64 probes per audit.
   - Docker image reclaim bytes come only from a numeric `docker image inspect` `Size` for the
     already-authorized full IDs. The human-readable `docker image ls` size is never converted by
     a unit heuristic; missing, duplicate, or mismatched inspect identities fail the category
     closed.
4. Every execution requires a fresh re-audit at execution time; the approval phrase embeds a
   SHA-256 fingerprint of the exact sorted candidate identity set. A stale phrase, empty
   candidate set, incomplete evidence, duplicate identity, or candidate set above the bounded
   exact-delete limit aborts before any mutation.
   A direct Docker host is passed explicitly with `--host`. A named Docker context is instead
   passed explicitly with `--context` so its TLS material remains available, while a fingerprint
   of the complete inspected context definition is bound into the approval without disclosure.
   A context/config change therefore invalidates approval before deletion.
5. Mutation uses only exact identities produced by that fresh audit (`container rm`, `image rm`,
   `volume rm`, or `network rm`). Category-wide `prune --force` is forbidden because a resource
   that becomes orphaned after the audit is not part of the approved fingerprinted set. Candidate
   identities remain private execution state: serialized plans and receipts expose only the
   fingerprint and a redacted `<candidate-set>` command marker.
6. The receipt records bounded command output plus before/after host free-space observation.
   Physical reclaim remains attribution-weak and is never claimed as proof. Once the exact-delete
   subprocess starts, non-zero exits, timeouts, and capture failures return an indeterminate receipt
   because an earlier identity may already have been removed.

## Consequences

- Positive: one mental model and one UI surface cover Docker, Colima, and Podman; evidence
  and receipts are schema-compatible with the existing Podman plan.
- Positive: approval and deletion authority now refer to the same exact resource identities;
  resources that become orphaned after the fresh audit cannot be swept into the mutation.
- Positive: a stopped or broken container cannot be removed before its attached volume's data
  necessity is independently established.
- Negative: exact deletion is capped at 256 candidates per category per execution so command
  length and mutation scope remain bounded. Larger candidate sets fail closed and must be
  reduced before a new audited execution.
- Neutral: no Figma redesign was required; the panel reuses Cleanup-screen patterns. If the
  Cleanup information architecture is redesigned later, record the Figma File ID in the
  superseding ADR first.

## Rejected alternatives

- Whole-category prune (`docker ... prune --force`, Podman equivalent): rejected because the
  runtime can delete a resource that becomes orphaned after the re-audit but was never part of
  the approved candidate fingerprint.
- Per-ID shell loops: rejected because repeated independent process launches enlarge partial-
  success ambiguity. DiskSage instead submits the bounded exact identity set in one runtime
  invocation and records the bounded result.
- Trusting cached UI plans: rejected; stale plans are the primary footgun this design
  eliminates via mandatory re-audit.
- Treating a stopped state as proof of disposability: rejected because stopped containers can
  retain anonymous or named database volumes, and deleting the container can erase the runtime's
  only queryable container-to-volume lineage.
- Auto-detecting Colima by spawning the `colima` binary: rejected to keep the runtime
  surface to two binaries (`docker`, `podman`) with explicit contexts.

## Evidence

- Rust unit tests cover envelope tolerance, ID normalization, classification fail-closed
  branches, network endpoint inspection shapes, fingerprint order-independence, scope
  validation, exact-delete candidate bounds, and redacted command construction.
- Runtime integration tests execute a fake Docker boundary and require an approved container
  execution to invoke `container rm <fingerprinted-id>` while explicitly rejecting any
  category-wide `prune` invocation.
- Frontend contract tests bind visible copy to non-target guarantees, exact phrase + rationale
  gating, confirmation dialog, post-execution state invalidation, and assistive-technology
  announcements with actionable copy only.

## References

Docker, Inc. (2026). *docker image inspect*. Docker Docs. https://docs.docker.com/reference/cli/docker/image/inspect/

Docker, Inc. (2026). *docker image ls*. Docker Docs. https://docs.docker.com/reference/cli/docker/image/ls/
