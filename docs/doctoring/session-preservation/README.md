# Agent session preservation experiment — 2026-09-06

## Objective and scope

Prevent DiskSage cleanup from removing Codex and Claude conversations while retaining useful generated-artifact cleanup. Session age, size, inactivity, successful task completion, and a cache-looking ancestor are not deletion authority. No live conversation was deleted, moved, compressed, or read for this experiment.

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

Ordinary and identity-bound Trash operations and moves apply the guard before mutation. Identity-bound Trash repeats the tree check after staging and restores the staged object on rejection. Native Git worktree audit and execution reuse the guard. The existing permanent-cache classifier now rejects session-bearing trees, but its broader name/signature authority remains insufficient: PR #263 owns disabling permanent cache Trash deletion and is still unmerged. This experiment does not replace that delta.

Known limits: arbitrary renamed/exported transcripts outside recognized/configured roots are not identifiable from path metadata; a custom root set only in another process is not discoverable from DiskSage's environment; filesystem path checks do not prove immunity to every concurrent namespace mutation. The 10,000-entry ceiling can retain legitimate large caches. Provider-local eviction and other applications' own retention policies need separate validation. Claude Code documents its own age-based sweep; a DiskSage guard does not disable it.

Next experiment: use a consented, metadata-only real candidate inventory with retained-session labels, report both false-positive count and eligible allocated bytes, then validate restoration and physical free-space change on an explicitly approved generated-artifact cohort. Do not lower session protection merely to increase byte counts.

## References

Anthropic. (n.d.). *Explore the .claude directory*. Retrieved September 6, 2026, from https://code.claude.com/docs/en/claude-directory

OpenAI. (n.d.). *Advanced configuration*. Retrieved September 6, 2026, from https://developers.openai.com/codex/config-advanced/

Context7 lookup was attempted but returned a monthly quota error. DeepWiki had no indexed DiskSage repository. Current local source and first-party documentation were used instead.
