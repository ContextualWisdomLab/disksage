# ADR-0022: Keep uv cache mutation behind uv-native prune authority

## Status

Proposed — 2026-09-22

## Problem

DiskSage's generic cache cleaner can enumerate and Trash-delete children of catalogued cache roots. That is not a valid mutation authority for uv. uv documents its cache as disposable but private, explicitly warns that direct cache modification is unsafe, and supplies `uv cache prune` as the supported operation for removing unused entries. The same storage model distinguishes cache-backed `uvx` environments from persistent `uv tool install` environments.

The incident that motivated this ADR also exposes a second boundary: an idle persistent user service can point directly, or through a symlink, at a cache-backed executable. Open-file/process evidence alone does not detect that dependency. uv also warns that symlink link mode creates tight coupling between installed environments and cache contents.

## Constraints

- DiskSage must not infer uv domain semantics from private bucket names such as `archive-v0`.
- A native prune must use one canonical executable identity and uv's configured cache directory.
- A plan is not mutation authority when inventory, process evidence, persistent-service evidence, or tool/cache symlink evidence is incomplete.
- A process can appear after DiskSage's read-only probe, so uv's own cache lock remains the final concurrency authority.
- Cache allocation is not recovered-capacity proof; capacity is measured before and after the native command.
- Approval and result records remain immutable local evidence. They are not sent to a remote service.

## Decision

Generic uv Trash cleanup fails closed with `uv-cache-native-owner-required`; `uv-cache` is not an automatic generic cleanup target.

The native owner resolves a fixed uv executable, binds its local filesystem identity, reads `uv cache dir --no-config` and `uv tool dir --no-config`, performs bounded cache inventory and active-use evidence, and evaluates persistent dependencies before approval. On macOS it reads the user's `~/Library/LaunchAgents` plist files. On Linux it reads the user's `~/.config/systemd/user` service files. A direct or resolved service path into the uv cache blocks pruning. A symlink in the persistent uv tools directory that resolves into the cache also blocks pruning. Incomplete service or symlink traversal fails closed.

Execution recreates the plan immediately, requires the exact fingerprint and human-attributed approval, revalidates executable identity, writes an immutable approval record, and invokes only:

`uv cache prune --no-config --offline --no-progress --color never --cache-dir <resolved-cache>`

`UV_LOCK_TIMEOUT=0` delegates the post-probe race to uv's cache lock rather than waiting or bypassing it. `--force`, `uv cache clean`, generic Trash deletion of private uv buckets, and direct filesystem removal are not part of this owner.

Windows remains fail-closed in this native owner until a Windows active-use/service adapter is implemented and proven with current-head filesystem/service fixtures.

## Rejected alternatives

- Protect every path named `archive-v0`: a private cache bucket name is not stable product ontology and unrelated directories can share the basename.
- Directly delete known uv cache buckets: uv states direct cache modification is unsafe.
- Treat all cache-backed `uvx` environments as persistent installed tools: uv documents them as disposable and recreatable.
- Use `--force`: it bypasses uv's in-use protection.
- Rely only on `lsof`: stopped/on-demand persistent services and cache-coupled symlinked tool environments can remain dependent without an open file descriptor.

## Risks and follow-up

The current service adapters intentionally cover user-level launchd/systemd definitions because the affected cache is user-scoped. Complex generated systemd command expressions are treated as incomplete evidence rather than guessed. Additional platform/service managers require explicit adapters and real fixtures before widening authority. ADR status must remain Proposed until same-exact Linux/macOS acceptance, current-head review, and protected-line adoption are complete.

## Traceability

Astral Software Inc. (2026). *Storage*. uv documentation. https://docs.astral.sh/uv/reference/storage/

Astral Software Inc. (2026). *Tools*. uv documentation. https://docs.astral.sh/uv/concepts/tools/

Astral Software Inc. (2026). *Caching*. uv documentation. https://docs.astral.sh/uv/concepts/cache/

Astral Software Inc. (2026). *Commands*. uv documentation. https://docs.astral.sh/uv/reference/cli/

Historical DiskSage Draft #285 is the predecessor evidence source for native prune planning/approval/receipt semantics. Its stale/diverged branch is not integrated by source-copy; this ADR reconstructs the decision on the current protected-main successor and adds persistent-service/symlink evidence required by the incident.