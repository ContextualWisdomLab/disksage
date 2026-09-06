# Agent session preservation experiment — 2026-09-06

## Objective and scope

Prevent DiskSage cleanup from removing Codex and Claude conversations while retaining useful generated-artifact cleanup. Session age, size, inactivity, successful task completion, and a cache-looking ancestor are not deletion authority. No live conversation was deleted, moved, compressed, or read for this experiment.

The user subsequently required at least **300 GiB of actual reclaimed capacity** with no session false positives. At the start of that capacity phase on September 6, the Data volume reported 9,126,100 KiB available (8.70 GiB), 869,260,276 KiB used, and no APFS snapshots. The acceptance target is a verified increase of at least 314,572,800 KiB, with operation evidence separating reclamation from concurrent host writes. Candidate allocation and moving items into same-volume Trash do not count toward this target. Until a non-overlapping, safely disposable inventory supports it, 300 GiB is a requirement rather than a forecast.

Baseline source: `0e90f9cebadbd7f59606baaec4ca1d2f178c899a` (main). The baseline executable compiled the unchanged `is_protected` function and its two std-only helpers from `safety.rs`, then evaluated the same eight paths used by `session_preservation_metric` under the checkout. All eight passed that old protection gate. This demonstrates a missing final guard; it does **not** establish that a planner selected eight real user sessions or that eight deletions occurred.

## Reproduce

Run the production guard's unit tests without downloading dependencies or building the desktop application:

```sh
rustc --edition=2021 --test src-tauri/src/agent_state_guard.rs -o /tmp/disksage-agent-state-tests
/tmp/disksage-agent-state-tests --nocapture
```

For integration with existing mutation boundaries:

```sh
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib safety::
```

The eight protected cases cover active/archived Codex rollouts, state database, session index, Claude transcripts, file history, prompt history, and spilled tool results. The metric is `unprotected labeled session paths / 8`, lower is better. Four non-session generated-artifact/sibling paths must remain unprotected by this additional policy. These controls establish eligibility, **not** approval to delete them. Other existing safety checks still apply.

| Experiment | Commit | Result | Decision |
| --- | --- | --- | --- |
| Baseline | `0e90f9ce` | 8/8 protected cases admitted by old guard | Record |
| State roots and bounded trees | `afcd93a2` | 0/8 admitted; relocated missing-destination regression failed | Repair; not accepted as passing |
| Resolve existing ancestors; native Git guard | `c5f9a0d1` | 0/8 admitted; 3/3 standalone tests passed | Keep |
| Staged recheck; Trash classifier | `d1f06969` | Adds defense at separate mutation boundaries | Requires integrated verification |
| Shared entry point regression | `eb70e3ea` | Adds preservation assertions for Trash, identity-bound Trash, and moves | Requires integrated verification |

This is a purposive regression set, not a probability sample. No confidence interval or population-wide zero false-positive claim is justified. Host free space was 12 GiB at the initial observation; no recovered bytes are attributed to this change.

The focused offline harness compiled the actual `safety.rs` plus the unchanged production full-file hashing functions and existing cached dependencies: **47 tests passed, 0 failed** after commit `8bb7e54d`. An intermediate run found a destination-error regression (46 passed, 1 failed); restricting tree inspection to the move source fixed it. The harness does not compile the entire application or native Git/cache modules. PR [#345](https://github.com/ContextualWisdomLab/disksage/pull/345) carries the change; repository CI and independent review are pending.

## Contracts and failure analysis

The shared guard protects complete `.codex`, `.claude`, and `.claude.json` components, their default home roots, and `CODEX_HOME` / `CLAUDE_CONFIG_DIR`. It compares original and canonical roots, including the nearest existing ancestor for a destination that does not yet exist. Parent selection is blocked when it would encompass configured state. A metadata-only tree walk catches nested project state; symlinks are not traversed. Incomplete evidence or more than 10,000 entries retains the tree.

Ordinary and identity-bound Trash operations and moves apply the guard before mutation. Identity-bound Trash repeats the tree check after staging and restores the staged object on rejection. Native Git worktree audit and execution reuse the guard. Review repairs at `6ea7992e` stage worktrees with native `git worktree move`, recheck the same filesystem object, and restore on rejection without overwriting a reappeared source. A failed restoration retains the staged tree and Git registration for recovery. The staging directory never recursively deletes its contents on drop. Native removal remains non-force and never deletes the branch.

The permanent cache Trash entry point now returns an unavailable error without filesystem mutation, reusing the fail-closed policy from PR #263. The CLI reports that items remain in OS Trash and does not create journal directories for this unavailable operation. This is only a narrow policy reuse: PR #263 still owns its other snapshot, approval, and provenance deltas and remains open. Generated-artifact Trash and eligible native worktree cleanup remain available under their existing gates.

Cloud source eviction checks for session state before staging. If the Trash callback fails while the verified regular file remains staged, a create-only hard link restores its original path; an occupied original path is never overwritten, and the staged file remains available if restoration fails. This closes the reviewed failure that could leave a retained source hidden after rejection.

Known limits: arbitrary renamed/exported transcripts outside recognized/configured roots are not identifiable from path metadata; a custom root set only in another process is not discoverable from DiskSage's environment; filesystem path checks do not prove immunity to every concurrent namespace mutation. The 10,000-entry ceiling can retain legitimate large caches. Provider-local eviction and other applications' own retention policies need separate validation. Claude Code documents its own age-based sweep; a DiskSage guard does not disable it.

Next experiment: use a consented, metadata-only real candidate inventory with retained-session labels, report both false-positive count and eligible allocated bytes, then validate restoration and physical free-space change on an explicitly approved generated-artifact cohort. Do not lower session protection merely to increase byte counts.

## Read-only host allocation observation

`du -sk` on four explicit roots completed without opening file contents or deleting data. On this host, Codex sessions occupy 3,064,548 KiB; Claude projects 1,613,792 KiB; npm content cache 1,165,572 KiB; uv cache 16,114,516 KiB. These are filesystem allocation observations, not reclaim approvals or guaranteed physical free-space gains (shared extents, active entries, and cache ownership still matter). Protect the approximately 4.5 GiB of session/project records while evaluating native pruning for the approximately 16.5 GiB of separate package-cache allocation. No raw session names or transcript content are included in this report.

The previously documented central hourly workflow path returned HTTP 404 during this run, and no matching local Codex automation was found. A local hourly Codex heartbeat (`disksage`) was subsequently created successfully for this task, with notifications limited to meaningful changes. This does not validate the historical central workflow.

## Integration verification and owner dependencies

The full application compiled offline and the ignored-session Git audit regression passed at `8a828d35`. The first complete library run after `6ea7992e` reported 757 passed, 4 failed, and 1 ignored. The new staging regression compared a macOS temporary-path alias directly with Git's canonical registration; `95ceeb21` aligns that assertion with the existing canonical-path contract. Recompilation and the staging regression passed, followed by all seven focused session, failed-Trash restoration, and permanent-deletion rejection regressions. Cloud approval and download materialization passed standalone reruns. Automatic cache cleanup failed twice; an idle synthetic-file `lsof` probe took 2.055 seconds, exceeding its unchanged two-second deadline. This identifies a host-dependent observation limit, not a green full-suite result. PR #322 repairs self command-argument matching but does not change that deadline or discard real open-handle blockers.

Hosted Test and Release failures at `8a828d35` reproduce existing concurrency-contract and Windows artifact-name mismatches owned by open PR #264. That PR also has failing central review/security checks and requires independent approval. Its changes must pass the protected merge gates before this proposal can claim integrated CI success; the session fix does not duplicate its workflow implementation or bypass those gates.

The cache CLI's five tests passed at source head `f67ac14e`, including unavailable purge without journal-directory creation; the same head's hosted `windows-home-resolution` job passed. Repeated native-probe experiments did not justify a timeout workaround: numeric-output flags still took 1.80–4.97 seconds for idle files and 1.08–3.50 seconds for held files. The `-b` option missed a held file and was rejected. No timeout or evidence-completeness guard was weakened.

Independent follow-up audit found two sibling cloud rollback branches still using check-then-rename. `f0c7a4e7` shares create-only restoration across verification, active-use rejection, and failed-Trash rollback. Its new regression preserves both files when another writer occupies the original path. The cloud module compiled and reported eight passed and one live-observation failure; that remaining approval test passed its standalone rerun. This does not erase the full-run failure.

## Capacity phase: online free-block return

After a successful native dry run, a single `colima ssh -- sudo fstrim --verbose /var/lib/docker` returned filesystem-unused blocks. This is an external supported guest operation, not a DiskSage execution receipt or a change to PR #310's unavailable stopped-VM execution contract. No files were selected for deletion. The guest reported 38,369,525,760 potential discard bytes, which is **not** the physical recovery measurement.

| Observation | Before (KiB) | After (KiB) | Difference (GiB) |
| --- | ---: | ---: | ---: |
| Default Colima backing allocation | 72,896,548 | 56,033,764 | 16.081604 less allocated |
| Host available space | 7,351,216 | 23,796,864 | 15.683792 net increase |

All 17 previously observed container IDs remained Up afterward; this verifies process continuity, not application health. Relative to the capacity-phase initial available-space observation (9,126,100 KiB), the post-operation host gain was 13.991131 GiB. Background writes and changing swap allocation affect host deltas, so neither the guest discard number nor later unrelated free-space changes are attributed to this operation.

The completed ChatGPT project inventory measured 48.857 GiB, including 22.921 GiB of build/dependency directories; these totals overlap and must not be added. Session stores, project data, initialized indexes, active builds, and unknown VM ownership are retained. The large iCloud metadata traversal remains incomplete and is not a reclaim estimate. The 300 GiB acceptance target remains unmet.

A subsequent dry run and single `podman machine ssh -- sudo fstrim --verbose /` likewise returned only unused filesystem blocks. Podman storage allocation decreased from 30,559,476 to 5,495,828 KiB (23.902557 GiB), while host available space increased from 25,233,564 to 50,130,164 KiB (23.743248 GiB). The guest's 106,836,242,432 reported discard bytes are not physical recovery. Podman still reported three images, zero containers, and zero volumes afterward; no image, container, volume, or session was deleted.

Combined backing-allocation reduction is 39.984161 GiB. At the second post-operation observation, host available space was 47.807850 GiB: 39.104523 GiB above the initial baseline, leaving 260.895477 GiB to the required 300 GiB increase. The operation records are measured live evidence, not proof that the incomplete candidate inventory is disposable or that the product has integrated these native execution paths.

## References

Anthropic. (n.d.). *Explore the .claude directory*. Retrieved September 6, 2026, from https://code.claude.com/docs/en/claude-directory

OpenAI. (n.d.). *Advanced configuration*. Retrieved September 6, 2026, from https://developers.openai.com/codex/config-advanced/

Git contributors. (n.d.). *git-worktree documentation*. Retrieved September 6, 2026, from https://git-scm.com/docs/git-worktree

util-linux contributors. (n.d.). *fstrim(8) manual page*. Retrieved September 6, 2026, from https://github.com/util-linux/util-linux/blob/master/sys-utils/fstrim.8.adoc

Context7 lookup was attempted but returned a monthly quota error. DeepWiki had no indexed DiskSage repository. Current local source and first-party documentation were used instead.
