# Preserve agent conversations at cleanup boundaries

- Status: Proposed
- Date: 2026-09-06
- Decision owner: DiskSage maintainers

## Context

A developer facing disk pressure needs regenerable build and package artifacts removed without losing ongoing or historical agent work. The current main guard protects operating-system roots but admits recognized conversation stores. Classifying JSONL as disposable would also lose transcripts, and protecting only transcript files would miss their indexes, state databases, snapshots, and attachments.

## Decision

In the context of evidence-bound disk reclamation, facing irreversible loss of resumable agent work, we will retain recognized and configured agent-state roots and any selected tree containing them, because path age, cache-like names, and completed tasks do not establish disposability, accepting that a bounded incomplete scan can retain a legitimate cache.

Use a std-only shared policy before ordinary Trash, identity-bound Trash, moves, and native Git worktree removal. Preserve original and resolved path identity, recheck staged trees, and fail closed on incomplete metadata walks. Use native Git moves for staging so registration survives rejection and recovery; never recursively remove a retained staging directory or overwrite a reappeared source. Recursive Git removal remains unavailable: a process retaining its working directory can create ignored session state after the final scan, and native Git removal then deletes it. A real filesystem regression reproduces this race; the retained worktree is restored and no reclaimed bytes are reported. Restore verified regular files after failed cloud eviction with create-only hard links. Disable permanent cache Trash deletion using the existing fail-closed policy proposed in PR #263; preserve that PR's remaining provenance and approval deltas.

## Alternatives considered

- Extension or age rules: rejected; they miss databases and attachments and cannot infer user intent.
- Disable every cleanup route: rejected; it cannot reduce disk-full pressure.
- Read transcripts and ask a model to classify value: rejected; adds private-content exposure and uncertain deletion authority.
- Root and metadata guard: selected as the smallest deterministic defense that preserves existing generated-artifact mechanisms.

## Consequences and verification

Positive: recognized session state survives broad folder selection, custom roots, and aliases without reading its contents. Negative: larger-than-100,000-entry or unreadable trees are retained, and arbitrary exported/renamed transcripts remain outside the recognized-root contract. Environment overrides must be visible to DiskSage. Concurrent namespace mutation remains a separate hardening concern.

Acceptance requires unit and mutation-boundary regressions, exact-head CI, review, and protected merge. Effectiveness additionally requires real candidate and physical-space evidence; a passing regression corpus is not universal proof. [Experiment record](../../doctoring/session-preservation/README.md) contains baseline SHA, results, commands, sources, and follow-up criteria. This proposal does not supersede existing ADRs or allocate an overlapping numeric identifier.
