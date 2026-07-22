# Stale Git worktree inventory

## Goal

Identify linked Git worktrees that consume local disk but can be considered for removal without treating age alone as proof of safety.

## Evidence and classification

- Discover repositories under a user-selected root and deduplicate linked worktrees by parsing small `.git` gitfiles with a two-second read bound. Standard `.git/worktrees/<id>` layouts derive their common directory from the path rather than reading `commondir`, which avoids an additional blocking read on pathological macOS metadata paths.
- Stop descending at each repository boundary; `git worktree list` supplies its linked paths, so scanning every source file merely to discover nested markers is unnecessary.
- Bound repository discovery to eight directory levels, 500,000 directories, and 60 seconds from the selected root. Deeper or slower layouts are inspected by selecting a nearer root, and truncation is visible in `scan_issues`.
- Skip Git subprocesses for repositories without a `.git/worktrees` registration directory because they cannot contain linked worktrees to reclaim.
- Parse `git worktree list --porcelain` and preserve worktree path, HEAD, branch, detached state, lock reason, and prunable reason.
- Measure estimated allocated bytes in Rust without following symlinks or counting shared `.git` object storage as reclaimable worktree data. Each linked tree has a 15-second/1,000,000-entry bound and the whole sizing phase has a 120-second bound.
- Skip expensive filesystem sizing for the primary worktree because it is categorically protected and cannot contribute to reclaimable linked-worktree bytes.
- Define last activity conservatively as the later of local HEAD commit time and the latest file modification time in the worktree.
- Resolve the local default ref from `origin/HEAD`, then conventional local/remote main or master refs. Do not fetch.
- Record dirty status, ahead/behind counts, and whether HEAD is already an ancestor of the local default ref.
- A linked worktree is removal-eligible only when it exists, is old enough, is clean, unlocked, attached to a branch, has no commits ahead of the default ref, and is merged into that ref.
- Primary, dirty, locked, detached, recent, unmerged, ahead, and indeterminate worktrees fail closed with explicit reasons.
- Incomplete filesystem evidence is shown as a partial estimate and always blocks removal eligibility.
- Missing paths that Git marks prunable are reported separately and never counted as disk-reclaim candidates.
- Run the bounded two-second `git worktree list --porcelain` preflight before resolving default
  refs or collecting secondary Git evidence. A repository that cannot enumerate its registered
  worktrees is already fail-closed and must not consume a second timeout window. Other Git
  subprocesses retain their five-second bound, and the repository evidence phase retains its
  180-second global bound. Timed-out children are killed and reaped off the inventory path.
- Report elapsed milliseconds and an explicit evidence-completeness bit. Completeness is true only
  when repository discovery and Git probes have no recorded issue and every existing non-primary
  or orphan filesystem measurement finishes within its bounds.

## Orphaned worktree and generated-artifact evidence

A worktree directory may remain after its `.git/worktrees/<id>` registration has disappeared. In
that state Git cannot prove the original HEAD, branch, dirty state, ahead count, or merge state.
DiskSage therefore reports the directory as an orphaned worktree, records the missing gitdir, and
categorically sets source-tree removal eligibility to false.

The same bounded Rust filesystem walk separately attributes allocated bytes under exact,
regenerable directory names: `node_modules`, Rust `target`, Python virtual environments and cache
directories, `.next`, and `.turbo`. Ambiguous names such as `build` and `dist` are excluded. These
paths are review evidence only; this slice exposes no deletion command. Partial walks are labeled
and never upgraded to complete evidence.

For registered linked worktrees, DiskSage batches those exact artifact roots through bounded
`git check-ignore -v -z --stdin` probes. Each artifact records a tri-state result plus the matching
rule source, line, and pattern: `true` means a positive ignore rule matched, `false` means Git
completed the probe but did not report the path as ignored, and `null` means the evidence could not
be collected. Output, input, subprocess, and total-inventory budgets are bounded; malformed,
unexpected, duplicate, oversized, failed, or timed-out evidence stays unknown and adds a scan
issue. Orphaned worktrees cannot resolve this evidence because their Git metadata is missing.

Ignore confirmation does not authorize deletion. A cleanup executor must still re-check active
processes, open files, and process working directories immediately before removing a cache. The
inventory remains read-only and reports ignore-confirmed bytes separately from all name-based
generated-artifact bytes.

On 2026-07-16 this policy was derived from a real 1.5 GiB orphaned Naruon worktree whose gitfile
pointed to a missing registration. The source tree was preserved while only its 1.4 GiB
`frontend/node_modules` directory was removed through the existing operator workflow.

## Local validation snapshot

On 2026-07-16, a read-only scan of the local Codex development root found 15 repositories with linked-worktree registration metadata and inspected 23 worktrees. All 17 non-primary worktree filesystem measurements completed. One clean, merged, zero-ahead worktree accounted for 1,345,085,440 allocated bytes, but its last activity was only two days old, so the 30-day policy blocked removal. The removal-eligible count remained zero. Slow or unavailable repository metadata still produced 40 explicit scan issues (30 bounded gitfile reads, nine bounded Git commands, and one bounded discovery pass); none were silently treated as safe. No worktree was changed.

On 2026-07-22, the ignore-evidence build scanned all 11 DiskSage worktrees in 3.031 seconds with
no scan issues and complete evidence. The current linked worktree's 98,017,280 allocated bytes of
`node_modules` were attributed to `.gitignore` line 2, pattern `node_modules`; all reported
generated-artifact bytes in that bounded scan were ignore-confirmed. Unit coverage also proved
that a tracked directory with a generated-artifact name remains `false` rather than being promoted
to ignore-confirmed evidence.

## Safety boundary

This slice is read-only. It never invokes `git fetch`, `git worktree remove`, `git worktree prune`, branch deletion, trash, or permanent deletion. Local default refs may be stale, so a later execution flow must refresh remote evidence where permitted, re-run all gates, show the exact command and paths, obtain explicit confirmation, and journal the result.

No LLM or LLM-as-a-Judge is needed for these deterministic Git predicates. Ollama is not used.
