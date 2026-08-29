# ADR-0023: Preserve OpenCode history and reclaim only unreferenced tool outputs

- Status: Accepted
- Date: 2026-08-29
- Scope: `opencode_artifact_reclaim`, headless planner/executor

## Context

OpenCode 1.18.23 stores sessions in a shared SQLite database and externalizes truncated tool
results under `tool-output`. The database, session diffs, and snapshots are user history rather
than generic caches. File age does not prove that any of them is disposable.

## Decision

DiskSage reads OpenCode's native `part.data.state.metadata.outputPath` references through the
root-owned macOS SQLite executable in read-only mode. It preserves every referenced output and all
database, WAL, snapshot, session-diff, authentication, and unknown objects. An absent reference
can authorize Trash movement only when the database/WAL identity remains stable, active-use
evidence is complete and idle, the exact regular-file identity and SHA-256 remain unchanged, and
the fresh candidate fingerprint receives an attributed exact-phrase approval. Create-only private
approval/result records and the existing append-only Trash journal preserve the mutation record.
Trash movement and permanent removal are disabled. A path-based Trash API cannot atomically prove
that the validated inode is the object moved, and a caller-selected shared journal cannot
authenticate quarantine provenance or one exact batch. Re-enabling mutation requires an
OS-enforced identity-bound move, current-user ownership, a create-only batch manifest bound to the
original approval, restart-safe immutable per-item before/after outcomes, and shared-journal batch
selection. Planning remains read-only and available.

## Consequences

- OpenCode history is not reduced merely to reach a disk target.
- Orphan sidecars left by completed native lifecycle operations become safely reclaimable.
- A large referenced output or database remains visible as a product gap, not misclassified cache.

## Rejected alternatives

- Age retention and size thresholds: rejected because they do not establish session lineage.
- Deleting sessions or running `VACUUM`: rejected without an explicit session-selection decision;
  the current database has no free-list pages and therefore offers no proven physical reclaim.
- Deleting snapshots or session diffs by filename: rejected until OpenCode exposes or DiskSage can
  prove their exact live-session/project reference graph.

## Evidence

The live OpenCode store contained 161 sessions. All 20 session-diff files matched database session
IDs. Of 63 tool-output files, 59 had exact native metadata references, four did not. The 8.4 GiB
database reported zero free-list pages. OpenCode's native `session delete` command is the supported
session lifecycle, but this decision grants no authority to choose or delete a session.

Before this fail-closed correction, the four exact outputs were moved to Trash and permanently
purged. They represented 260,637 logical and 270,336 allocated bytes. Concurrent filesystem writes
reduced APFS availability across both bounded observations, so DiskSage records no attributable
physical-space gain. Review then found that the mutation protocol did not authenticate the shared
journal or close replacement and partial-outcome races; both mutation surfaces are now disabled.

## References

SQLite Consortium. (2025). *Pragma statements*. https://www.sqlite.org/pragma.html

OpenCode. (2026). *OpenCode source repository*. https://github.com/anomalyco/opencode
