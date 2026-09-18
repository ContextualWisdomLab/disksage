# Corrected reclaim estimate (vs ~41.5 GiB claim) — 2026-09-18

## Verdict

The user-facing **~41.5 GiB** figure is **already-reclaimed morning work**, not remaining
actionable inventory under DiskSage Orca protections (`#440`).

| Metric | GiB (logical) |
| --- | ---: |
| Claim (user-facing) | 41.5 |
| Reclaimable-now **with** 7d recent-write gate | **0.0** |
| Reclaimable-now **excluding** recent-write only (still live Orca / lead / editable / data protections) | **11.4** |

Evidence: Studio criteria review JSON/MD under operator bin notes
(`orca_reclaim_criteria_review_20260917.*`) and the follow-up `#440` dry-run
(`orca_reclaim_dryrun_440_20260918.md`): **436** worktrees audited → **0** removal
candidates with the 7d window.

### Gate statement

When Orca protections are enabled, DiskSage requires an explicit
`--recent-write-window-secs` (commonly **604800** = 7 days). That gate blocks
**destructive** worktree / rebuildable-artifact reclaim. It does **not** apply to:

1. **Lossless Codex session zstd** (`disksage-log-archive`, `--older-than-days 0`) — verify-then-remove compression; reversible; not a deletion policy.
2. **CloudKit / generated caches** under documented cache reclaim paths — rebuildable OS/app caches, not Orca worktree deletes.

## Related Studio outcomes (operator notes)

- Codex sessions (prior >30d pass): ~5.86 GiB logical reclaim; tree ~42 G→~37 G. Remaining shrink needs `--older-than-days 0` under host RAM headroom.
- CloudKit `Library/Caches/CloudKit`: ~12.5 GiB then ~0.75 GiB residue cleared.
- Parallels VMs: **untouched** by policy.

## Swap / RAM pressure (separate from DiskSage)

Host swap saturation is **RAM pressure**, not free-disk inventory. DiskSage reclaim and
log-archive change **disk** footprint; they do not evacuate anonymous memory or reduce
swap used by live `cargo`/`maturin`/solver processes. Response under resource guard:
shed Studio builds, prefer MacBookAir for compile, keep session caps low.

## PR note on #437

`ContextualWisdomLab/disksage#437` (homebrew audit subprocess bounds) is **already
MERGED** — not draft. No undraft action remains for that lane.
