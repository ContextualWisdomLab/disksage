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
   - containers in `exited`/`created`/`dead` states only;
   - untagged images with proven zero container references (tagged images are never
     candidates even when unused);
   - dangling volumes reported by the runtime's own `dangling=true` filter;
   - custom networks excluding built-ins (`bridge`, `host`, `none`, `podman`) whose inspect
     proves zero attached endpoints, bounded at 64 probes per audit.
4. Every execution requires a fresh re-audit at execution time; the approval phrase embeds a
   SHA-256 fingerprint of the exact sorted candidate identity set. A stale phrase, empty
   candidate set, or incomplete evidence aborts before any mutation.
5. The known TOCTOU window between pre-execution audit and prune is minimized by the fresh
   re-audit and disclosed in the module documentation; the receipt records command output
   verbatim plus before/after host free-space observation. Physical reclaim remains
   attribution-weak and is never claimed as proof.

## Consequences

- Positive: one mental model and one UI surface cover Docker, Colima, and Podman; evidence
  and receipts are schema-compatible with the existing Podman plan.
- Negative: per-category pruning uses the runtime's own prune command, so it removes all
  currently-orphaned resources of that category rather than an exact subset. This matches
  the existing dangling-image boundary and is why the approval phrase binds to the full
  candidate set instead of individual IDs.
- Neutral: no Figma redesign was required; the panel reuses Cleanup-screen patterns. If the
  Cleanup information architecture is redesigned later, record the Figma File ID in the
  superseding ADR first.

## Rejected alternatives

- Per-ID selective deletion (`docker rm <id>` loops): rejected because partial success
  mid-loop leaves ambiguous state without improving safety over a verified whole-category
  prune with a bound candidate set.
- Trusting cached UI plans: rejected; stale plans are the primary footgun this design
  eliminates via mandatory re-audit.
- Auto-detecting Colima by spawning the `colima` binary: rejected to keep the runtime
  surface to two binaries (`docker`, `podman`) with explicit contexts.

## Evidence

- Rust unit tests: 31 focused tests covering envelope tolerance, ID normalization,
  classification fail-closed branches, network endpoint inspection shapes, fingerprint
  order-independence, scope validation, and prefix construction.
- Frontend contract tests: 6 tests binding visible copy to non-target guarantees, exact
  phrase + rationale gating, confirmation dialog, post-execution state invalidation, and
  assistive-technology announcements with actionable copy only.
