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

The shared guard protects complete `.codex`, `.claude`, and `.claude.json` components, their default home roots, and `CODEX_HOME` / `CLAUDE_CONFIG_DIR`. It compares original and canonical roots, including the nearest existing ancestor for a destination that does not yet exist. Parent selection is blocked when it would encompass configured state. A metadata-only tree walk catches nested project state; symlinks are not traversed. Incomplete evidence or more than 100,000 entries retains the tree.

Ordinary and identity-bound Trash operations and moves apply the guard before mutation. Identity-bound Trash repeats the tree check after staging and restores the staged object on rejection. Native Git worktree audit and execution reuse the guard. Review repairs at `6ea7992e` stage worktrees with native `git worktree move`, recheck the same filesystem object, and restore on rejection without overwriting a reappeared source. A failed restoration retains the staged tree and Git registration for recovery. The staging directory never recursively deletes its contents on drop. Native removal remains non-force and never deletes the branch.

The permanent cache Trash entry point now returns an unavailable error without filesystem mutation, reusing the fail-closed policy from PR #263. The CLI reports that items remain in OS Trash and does not create journal directories for this unavailable operation. This is only a narrow policy reuse: PR #263 still owns its other snapshot, approval, and provenance deltas and remains open. Generated-artifact Trash and eligible native worktree cleanup remain available under their existing gates.

Cloud source eviction checks for session state before staging. If the Trash callback fails while the verified regular file remains staged, a create-only hard link restores its original path; an occupied original path is never overwritten, and the staged file remains available if restoration fails. This closes the reviewed failure that could leave a retained source hidden after rejection.

Known limits: arbitrary renamed/exported transcripts outside recognized/configured roots are not identifiable from path metadata; a custom root set only in another process is not discoverable from DiskSage's environment; filesystem path checks do not prove immunity to every concurrent namespace mutation. The 100,000-entry ceiling can retain legitimate large caches. Provider-local eviction and other applications' own retention policies need separate validation. Claude Code documents its own age-based sweep; a DiskSage guard does not disable it.

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

The subsequent temporary-directory allocation scan completed with 1,370 directory entries totaling 449.740082 GiB of reported allocation and no reported errors. This is not exclusive physical storage: APFS/shared extents and links can overstate recoverable blocks. It includes 29.390583 GiB under a Claude temporary root, which remains preserved, plus source checkouts and running Python environments. A bounded follow-up separates build/dependency artifacts from those retained contents before any cleanup decision. Spotlight returned no large-file results while reporting an unknown indexing state, so it supplies no absence or capacity evidence. The iCloud allocation query remains incomplete. Later host free-space increases coincide with declining swap use and are not credited as additional cleanup.

The first bounded artifact inventory covered the 100 largest eligible temporary roots and found 112 directories: 67 Cargo targets (137.080 GiB), 32 dependency directories (32.779 GiB), and 13 Python environments (2.923 GiB), totaling 172.783 GiB reported allocation. The largest 20 Cargo targets total 54.899 GiB and have adjacent retained manifests and lockfiles. Executable-path observations positively identified running Python and build-tool environments, which are preserved. An absent executable match does not prove inactivity; complete session exclusion, source linkage, filesystem identity, and open-handle evidence remain required. No candidate was deleted. This inventory excludes recognized agent storage rather than reclassifying it as disposable temporary output.

Applying the unchanged production session-tree guard to the 20 largest Cargo candidates completed 14 observations, all returning retain, before the 120-second observation limit. Six candidates remain unobserved. The boolean guard does not distinguish detected state from incomplete/over-limit walks, so none of these results proves session presence or safe disposability. No cleanup followed. The separate inventory extension continues incrementally without remeasuring the original 112 entries.

Further owner review found check-then-rename restoration in the shared safety path. Commit `85c87879` uses native atomic no-replace rename on macOS, Linux, and Windows, failing closed on unsupported platforms. The regression set covers an occupied empty directory, dangling symlink, and successful identity-preserving directory restoration. This shared repair is a prerequisite for the generated-cache owner stack (#295, #320, #322, #325); its implementation is not copied into those branches and their remaining deltas are preserved.

The actual safety-module harness passed all 50 tests at `85c87879`, including the three new exclusive-restore cases and the previous 47 tests. Linux and Windows native execution still requires CI validation. The iCloud allocation query subsequently completed without reported errors at 324 KiB; it provides no material capacity toward the target and is excluded from reclamation.

The completed extension produces 444 unique artifact paths totaling 321.083229 GiB reported allocation: Cargo targets 234.879414 GiB, dependency directories 53.094307 GiB, and Python environments 33.109509 GiB. Three candidates have positively observed active executables and remain preserved. All remaining entries still lack complete inactivity and retained-source evidence; nominal allocation plus prior VM recovery is not proof of the 300 GiB physical target.

An independent metadata count found all 20 largest Cargo targets exceed 10,000 entries, with no observed metadata errors before reaching 10,001. Their earlier retain decisions therefore cannot be treated as session-positive labels or converted to deletion approval. Larger generated-tree evidence needs an explicit complete-scan contract; bypassing the incomplete-walk gate is not a remedy.

Performance experiment `aa756124` prepares configured root identities once per tree walk and compares a fresh root snapshot before allowing cleanup. It retains live candidate canonicalization and the existing entry/error limits. The same private 8,001-entry fixture measured median 423,248 microseconds before and 211,097 after (approximately 2.00 times faster), with identical allow decisions for the six synthetic runs. The standalone three-test suite and the actual 50-test safety harness passed after the change. Snapshot comparison does not prove immunity to transient root retargeting that returns to the original state.

Measured complete Cargo trees contained 25,630, 70,344, and 65,895 entries; metadata enumeration completed in 0.9–3.2 seconds. Experiment `abae2b65` raises the bounded full-walk allowance to 100,000 entries without sampling or permitting incomplete evidence. A new 10,001-file fixture remains blocked under the old budget, passes a complete new-budget walk, and is retained after nested Claude state is added. All four standalone tests and 51 actual safety-module tests passed. The 20 largest previously blocked targets completed production guard checks in 55.147 seconds, all without protected-state or incomplete-walk rejection. Fresh native open-file checks completed for all 20 with no observed holders; Cargo metadata bound each target directory to retained source files outside it.

## Native Cargo reclamation pilot

One source-linked temporary Cargo target was revalidated, previewed with `cargo clean --dry-run --frozen`, moved into a private sibling directory using the actual shared atomic no-replace helper, identity-checked, and scanned again for agent state and open files. Native `cargo clean --frozen --manifest-path … --target-dir <private staged target>` completed successfully. All 36 Cargo-reported source/manifest files remained present; the staged target and empty private parent were absent afterward. No session file or repository source was selected for removal.

The pilot removed 4,164,724 KiB (3.971790 GiB) of reported target allocation. Host available space changed from 60,895,088 to 64,857,912 KiB, a net 3.779243 GiB increase during the operation; concurrent writes still limit causal attribution. The operation journal is separate from a product execution receipt, and does not claim PR #295 or #345 has merged or shipped. Remaining source-linked targets use the same per-operation revalidation, private staging, and no-overwrite failure restoration. An unknown, active, changed, oversized, or unreadable candidate is retained.

The pilot and first sequential cohort have now completed all 20 operations. Every receipt records successful native cleanup, retained Cargo-reported source files, and no restore error. Their combined pre-clean allocation was 55.547333 GiB; the sum of signed per-operation available-space changes was 53.826694 GiB. These are separate measurements and must not be added together or interpreted as an exclusive APFS extent measurement. Later background free-space changes remain excluded. The remaining 147 source-linked temporary candidates are being revalidated individually; active-use rejection and incomplete observations retain the original directories.

A separate read-only local-project inventory verified 17 Cargo target/source relationships, totaling 22.19 GiB of reported allocation. No executable match was observed, but inactivity is still unverified. These candidates are not included in the completed recovery figures. The 300 GiB actual-recovery acceptance criterion remains unmet.

## Late-arrival Git session preservation

During operational review, Cargo 1.97.1 was shown to clean an independently configured `build.build-dir` even when `--target-dir` selected a private stage. The running cohort was paused; its current pre-mutation guard was cancelled, leaving that candidate untouched. A fresh metadata audit of all 69 completed operations reported coincident target/build directories. This is current configuration evidence, not a historical configuration snapshot.

The operational scripts now reject an independent natural build directory, explicitly bind `build.build-dir` during preview and cleanup, and verify both configured directories against staging before native cleanup. A real Cargo fixture verified rejection of an independent directory and successful scoped cleanup with sources and the unrelated directory retained. Cargo's own missing-cache-tag rejection also remained intact; no marker was fabricated to bypass it. The sequential cohort then resumed. These scripts are operational evidence, not a shipped implementation or authorization to broaden the generated-cache owner's allowlist.

An actual Git fixture reproduced a remaining destructive race: a child retained its working directory across staging, waited until the final session scan completed, created an ignored conversation file, and native non-force worktree removal deleted it. A private staging name and another metadata scan cannot exclude that existing-directory access.

Commit `06b55bce` therefore makes recursive Git worktree removal unavailable and uses existing restoration to retain the worktree and its registration. All 18 actual Git worktree tests passed, including late-arrival Codex and Claude state, restoration collisions, and zero removal/zero reclaimed bytes for ordinary candidates. The UI now presents the read-only audit without asking for a deletion approval; the CLI help states the same availability limit. Re-enabling removal requires a verified contract that preserves late-arriving state, not another scan alone.

## Larger observed trees and native cache capacity

The temporary cohort completed 147 observations: 125 native cleanups and 22 retained candidates. The local cohort completed 17 observations: 16 cleanups and one retained large tree. Two temporary candidates previously interrupted by observation timeout or the scope audit subsequently passed fresh complete validation and cleanup. These counts exclude synthetic fixtures; per-operation records remain authoritative for bytes and retained sources.

Experiment `e14d1bfb` raises the complete-walk allowance to 500,000 entries after metadata-only measurements found 460,192 entries in the UV cache and 186,178 in a retained local build tree. Four standalone and 51 actual safety tests passed. The production UV guard completed without rejection in 34.82 seconds, using 5,193,728 bytes maximum RSS. No scan was sampled and no canonical-path, symlink, error, or pending-work check was relaxed. Cache pruning was not authorized by that result alone: external environments may refer to cache files using symlinks, which requires separate consumer evidence.

Native BuildKit cleanup was limited to 106 freshly revalidated, reclaimable, non-shared regular cache records using an exact ID selector and explicit Colima builder/context. After cleanup and guest free-block return, backing allocation decreased by 9.008080 GiB and host available space increased by 9.970119 GiB during the operation. Every previously observed running container, image, and volume remained present. The difference between these measurements reflects other activity; neither is added to guest-reported discard bytes.

## Completed native cleanup and remaining capacity target

The receipt audit now contains 165 successful operations on 165 distinct real Cargo target roots: the pilot, 19 first-cohort targets, 125 remaining-cohort targets, 17 local targets, two successful retries, and this task's own completed validation target. Synthetic build-directory fixtures and unsuccessful attempts are excluded. Every included receipt records native exit zero, retained reported source files, and no restoration error. Combined pre-clean allocation was 244.085270 GiB; the sum of signed per-operation available-space changes was 237.657967 GiB. These measurements overlap with the earlier cohort totals and are not additional amounts to add to them.

The local total includes the previously retained ConceptWeave target, which passed fresh complete validation before removing 7.599163 GiB of reported allocation; its observed available-space increase was 7.554836 GiB and all 41 reported source files remained. This task's completed validation target contributed 3.688370 GiB of allocation and a 3.492432 GiB observed increase; all 83 reported source files and the validation logs outside the target remained. Both receipts bind the natural build directory and staged target/build directories. These operations do not establish that historical configurations of earlier operations were unchanged.

Three unused Podman base images were removed by exact image identity after their remote manifest indexes and platform digests were verified. The stored records retain immutable Rust 1.98, Rust 1.97.1, and Python image references for retrieval. Following native removal and guest free-block return, backing allocation decreased by 2.359924 GiB and host available space increased by 2.380783 GiB during the operation. No containers or volumes remained before/after this image cleanup; the guest's discard count is not added to reclaimed capacity.

Native rustup removed only the installed documentation components from five exact toolchains. Each component manifest contained only its documentation directory; the production agent-state guard completed without rejection and a fresh open-file observation found no holders before removal. All native commands returned zero, all five documentation roots were absent afterward, and compiler paths, compiler versions, and every other installed component remained unchanged. Removed allocation totaled 4.384247 GiB; the batch's observed available-space increase was 4.375164 GiB.

The UV cache remains retained. Native observation found 50 distinct holders, including cache-lock users and Python processes loading extension modules from its archive directories. A successful agent-state scan or an inbound-symlink inventory does not override this direct active-use evidence. No force cleanup or whole-cache removal was performed.

At 2026-09-06T14:52:06.370856+00:00, `df -k` reported 309,154,404 KiB available, compared with the capacity-phase baseline of 9,126,100 KiB. The observed increase was **286.129288 GiB**, leaving **13.870712 GiB** to the required additional 300 GiB. This current host snapshot is affected by background writes and swap; it is not the sum of exclusive reclaimed extents. The acceptance target remains unmet.

Local evidence for this update is stored under `/tmp/disksage-session-baseline/`: `cargo-reclaim-receipts/`, `podman-recoverable-image-cleanup.json`, the three remote manifest records, `rust-docs-cleanup.json`, `uv-cache-native-active-use.json`, `uv-cache-held-paths.json`, and `readme-capacity-snapshot.json`. These are operational records, not shipped-product or protected-merge evidence.

## Native dependency cleanup limits and late parent-directory access

Trivy 0.74.0's database-only command completed against a privately staged directory containing only its database and metadata files. A current remote database manifest was available; the existing policy directory was retained. The operation removed 1,319,332 KiB of database allocation and observed a 1,318,472 KiB increase in available space. The regenerated database may be a newer revision. No other Trivy cache category was selected.

Thirty temporary pnpm projects had retained tracked sources, manifest/lock hashes, a cached `pnpm@11.5.3` pin, and one verified project scope. Native cleanup was invoked through that exact version's built-in `pm clean` command in a private minimal execution project, with workspace discovery and hooks disabled, module/store paths confined, private XDG configuration, and original HOME unchanged. A 12-check fixture verified script/hook suppression, preserved source/lockfiles, retained custom hidden entries, and non-traversal of external symlink targets.

An additional actual-process fixture exposed a parent-directory gap: target-only open-file checks reject a held working directory or direct target descriptor, but miss an owning-project descriptor. Such a writer discovered staging and created a nested Claude session after the final check; native pnpm cleanup deleted the synthetic file. The subsequent protocol checks the entire owning Git root and original command-path association before staging and immediately before cleanup. A new fixture rejects both an existing parent descriptor and one acquired after staging, retains the new session, and restores without overwrite. These observations do not exclude arbitrary actors arriving after the final probe. The original RED report was accidentally overwritten during the GREEN experiment; original tool output remains, and separately named RED rerun and GREEN source/report/log artifacts now preserve independently measured evidence.

Five actual dependency cleanups completed, with original manifest/lock hashes and reported sources retained; four used full source-content hashes and the expanded owning-root checks. The first pilot used source metadata identity outside its hashed manifest/lock. All five restored the custom `.vite` leftovers rather than deleting them. Their signed available-space changes sum to **-1.447853 GiB** despite approximately 1 GiB of reported allocation removed per project. Further cleanup was stopped after the current operation completed. A complete remaining-tree metadata audit found 32,049 regular files, all with one hardlink. Installed pnpm prefers reflink cloning on macOS; shared APFS extents and concurrent writes are plausible explanations, but exclusive extent identity was not measured. Directory allocation is therefore not credited as reclaimed physical space, and the remaining dependency cohort is retained.

The 15 remaining Cargo preview failures total 10.254 GiB reported allocation. Fresh full, package-specific, and documentation-only dry runs still reject missing `CACHEDIR.TAG`; no marker was created or refusal bypassed. UV remains actively used. Global pnpm pruning has broader effects than its store directory and observed process activity, so it was not executed. All installed Playwright revisions remain referenced and were retained. Native browser-cache inspection could not proceed because the Mac was locked; no browser settings or cache contents were changed.

Canonical generated-cache PR #295 at `a4a897e3` has the same structural parent-handle scope gap and drops original command-path matching during staged probes. Its temporary-workspace contract remains unavailable and does not authorize these external pnpm operations. Owning-scope evidence, original-path association, parent-descriptor regression coverage, and provider-native locking where available remain owner acceptance work; this runtime lane does not duplicate that implementation or claim it has shipped.

At 2026-09-06T15:20:09.211Z, available space was 310,653,924 KiB (296.262669 GiB), 287.559341 GiB above the 9,126,100 KiB baseline. The required additional 300 GiB remained 12.440659 GiB short. No native-cleanup runner remained active. Background changes remain separate from operation attribution.

## Original command-path association in the generated-cache owner

Canonical PR #295 subsequently advanced to `0b30fa20d31051745506994043186a331c2821a2`. Both staged probes now reuse `active_use_evidence_with_command_path` with the original cache path. Controlled-boundary tests include the actual generated-cache module: the two new cases fail against the previous source, while all 12 focused module tests pass after the change. They verify rejection and restoration at the first staged probe and after hashing, rather than asserting only which helper name appears in source.

A separate live-process experiment included the unchanged production `git_worktree.rs` (blob `cf08607ee859bb4d98184a1bbff275d4f8d32c7f`) and used native `/usr/sbin/lsof` and `/bin/ps`. A live shell carried the original path in its arguments, had its working directory outside the cache, and opened no cache files. Following a synthetic rename, the staged-path-only helper returned complete inactive evidence; the split object/command-path helper returned complete active evidence with that exact live child PID. Both results had no error or truncation. The child was reaped and the empty fixture removed. This verifies native command-path association, not arbitrary concurrent-writer exclusion or the whole destructive execution path. Parent-scope binding and atomic no-replace restoration remain separate owner requirements.

## References

Anthropic. (n.d.). *Explore the .claude directory*. Retrieved September 6, 2026, from https://code.claude.com/docs/en/claude-directory

OpenAI. (n.d.). *Advanced configuration*. Retrieved September 6, 2026, from https://developers.openai.com/codex/config-advanced/

Git contributors. (n.d.). *git-worktree documentation*. Retrieved September 6, 2026, from https://git-scm.com/docs/git-worktree

util-linux contributors. (n.d.). *fstrim(8) manual page*. Retrieved September 6, 2026, from https://github.com/util-linux/util-linux/blob/master/sys-utils/fstrim.8.adoc

Context7 lookup was attempted but returned a monthly quota error. DeepWiki had no indexed DiskSage repository. Current local source and first-party documentation were used instead.
