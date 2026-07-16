# Stale Git worktree inventory

## Goal

Identify linked Git worktrees that consume local disk but can be considered for removal without treating age alone as proof of safety.

## Evidence and classification

- Discover repositories under a user-selected root and deduplicate linked worktrees by parsing small `.git` gitfiles with a 250 ms read bound. Standard `.git/worktrees/<id>` layouts derive their common directory from the path rather than reading `commondir`, which avoids blocking on pathological macOS metadata reads.
- Stop descending at each repository boundary; `git worktree list` supplies its linked paths, so scanning every source file merely to discover nested markers is unnecessary.
- Bound repository discovery to eight directory levels, 100,000 directories, and five seconds from the selected root. Deeper or slower layouts are inspected by selecting a nearer root, and truncation is visible in `scan_issues`.
- Skip Git subprocesses for repositories without a `.git/worktrees` registration directory because they cannot contain linked worktrees to reclaim.
- Parse `git worktree list --porcelain` and preserve worktree path, HEAD, branch, detached state, lock reason, and prunable reason.
- Measure estimated allocated bytes in Rust without following symlinks or counting shared `.git` object storage as reclaimable worktree data. Each linked tree has a two-second/200,000-entry bound and the whole sizing phase has a ten-second bound.
- Skip expensive filesystem sizing for the primary worktree because it is categorically protected and cannot contribute to reclaimable linked-worktree bytes.
- Define last activity conservatively as the later of local HEAD commit time and the latest file modification time in the worktree.
- Resolve the local default ref from `origin/HEAD`, then conventional local/remote main or master refs. Do not fetch.
- Record dirty status, ahead/behind counts, and whether HEAD is already an ancestor of the local default ref.
- A linked worktree is removal-eligible only when it exists, is old enough, is clean, unlocked, attached to a branch, has no commits ahead of the default ref, and is merged into that ref.
- Primary, dirty, locked, detached, recent, unmerged, ahead, and indeterminate worktrees fail closed with explicit reasons.
- Incomplete filesystem evidence is shown as a partial estimate and always blocks removal eligibility.
- Missing paths that Git marks prunable are reported separately and never counted as disk-reclaim candidates.
- Bound every Git subprocess to one second and the repository evidence phase to 30 seconds. Timed-out children are killed and reaped off the inventory path. A broken or slow repository is reported as a scan issue and cannot become removal-eligible.

## Local validation snapshot

On 2026-07-16, a read-only scan of the local Codex development root completed in 25.5 seconds. It found 11 repositories with linked-worktree registration metadata and inspected 14 linked worktrees. Six filesystem measurements completed and eight were explicitly partial. All 14 failed at least one conservative gate (recent activity, dirty state, unmerged/ahead commits, unresolved default ref, or incomplete filesystem evidence), so the removal-eligible count was zero. No worktree was changed.

## Safety boundary

This slice is read-only. It never invokes `git fetch`, `git worktree remove`, `git worktree prune`, branch deletion, trash, or permanent deletion. Local default refs may be stale, so a later execution flow must refresh remote evidence where permitted, re-run all gates, show the exact command and paths, obtain explicit confirmation, and journal the result.

No LLM or LLM-as-a-Judge is needed for these deterministic Git predicates. Ollama is not used.
