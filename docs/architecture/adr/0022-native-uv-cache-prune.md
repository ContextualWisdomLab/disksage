# ADR-0022: Use uv's native prune only after active-use evidence closes

## Status

Accepted — 2026-08-29

## Context

The live uv cache occupies several GiB, but multiple long-running local tools currently hold uv's
cache lock and load code from cached environments. Direct directory deletion is outside uv's cache
contract and could disrupt those processes. The existing per-child Trash path is useful for
independent inactive objects, but it does not express uv's own reachability model.

## Decision

DiskSage adds one narrow native action: `uv cache prune`. Its read-only plan resolves `uv cache
dir`, binds the canonical executable identity and current cache allocation, and collects a bounded
recursive active-use observation. Any active process, incomplete inventory, or incomplete active-use
observation blocks approval.

Execution never uses `--force`. It regenerates the plan immediately, requires its exact fingerprint
and phrase, revalidates executable identity, writes an immutable attributed approval, and runs the
fixed prune command with uv's lock timeout set to zero. uv's own lock therefore closes the race if a
new user appears after DiskSage's probe. The result record retains bounded command output and
filesystem available bytes before and after; estimated cache allocation is not called recovered
capacity.

## Consequences

- Active uv/MCP tools keep running and the current cache remains untouched.
- A later inactive observation can remove only entries uv itself declares unreachable.
- Full `uv cache clean`, direct cache deletion, arbitrary age rules, and `--force` remain excluded.
- This first native package-manager contract covers uv only; other managers require their own
  authoritative lifecycle command and evidence.

## Rejected alternatives

- Deleting cache subdirectories directly: uv documents direct cache modification as unsafe.
- `uv cache clean`: it clears all entries rather than only unreachable objects.
- `uv cache prune --force`: it bypasses uv's in-use protection.
- Waiting an arbitrary number of days: age does not prove reachability or inactivity.

## Evidence

Astral Software Inc. (2026). *Caching*. uv documentation. https://docs.astral.sh/uv/concepts/cache/

The documentation states that `uv cache prune` removes unused entries, direct cache modification is
unsafe, cache mutations are lock-protected, and `--force` ignores the in-use check. The live plan is
therefore blocked while local tools hold the cache.
