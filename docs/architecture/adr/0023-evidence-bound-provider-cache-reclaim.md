# ADR-0023: Evidence-bound provider cache reclaim

- Status: Proposed
- Date: 2026-08-29
- Last reviewed: 2026-09-05

## Context

DiskSage can identify provider-owned local artifacts that are regenerable, including superseded Microsoft EdgeUpdater installed copies, EdgeUpdater `crx_cache`, and content-addressed Podman AppleHV machine seeds that are not the configured VM image. A provider label, old directory name, stopped process, or matching pathname is not deletion authority. Planning therefore has to establish the cache class, object/content identity, regeneration evidence, active-use evidence, and a fresh plan fingerprint before any mutation is considered.

OS Trash is the supported reversible product action. Permanent provider-cache deletion is materially different. The historical lower-level implementation stages a candidate through pathname rename and then removes it. Its identity/content checks, double fingerprint confirmation, receipt, and journal are useful test evidence, but they do not bind irreversible mutation to the same reviewed filesystem object across ancestor replacement, crash recovery, and Windows/Linux/macOS. Those destructive helpers therefore remain `#[cfg(test)]`; production has no static call edge to them.

Receipt publication is also a security boundary. A receipt that authorizes mutation must not be redirected through parent replacement, permission drift, or same-name substitution, and failure cleanup must not unlink a same-name replacement created by another same-user process. Provider-cache must consume reusable filesystem authority rather than creating or chmodding receipt directories by pathname.

## Decision

DiskSage admits only these independently identified provider-cache classes:

1. A Podman AppleHV content-addressed `*.raw.zst` seed whose 64-hex key matches the full file digest, whose machine/configured-image evidence is known, and whose selected object is not the configured or active VM disk.
2. A Microsoft EdgeUpdater cached installed copy whose bundle version exactly equals its directory name and differs from the exact installed `/Applications` version. The installed version is retained.
3. EdgeUpdater `crx_cache` as a separate explicitly selectable regenerable candidate.

Planning is read-only and fails closed when inventory traversal, recreation evidence, content identity, or active-use evidence is incomplete. Execution re-plans and rechecks the selected (`path`, `evidence_fingerprint`, `object_id`) triplets against both approved fingerprints.

All shipped Rust facade, Tauri, TypeScript, and headless CLI mutation contracts expose Trash only. The external Rust plan contains `trash_approval_phrase` and no `exact_approval_phrase`; the Tauri command accepts no cleanup-mode argument; the TypeScript wrapper sends no mode field; and the CLI rejects `--permanent-purge` before manifest or executor work. The crate-private executor rejects `PermanentPurge` before re-planning or receipt creation. After that guard, production execution writes a Trash receipt, calls only `trash_delete_if_identity`, and returns the Trash mode.

#303 adopts the current #344 private-publication owner exact `f192567dc6f25d1c9ba921346efa18c3c3287dba` through non-force second-parent ancestry. Merge `a20a8c0f30010caa42e7ea159fa0262361c1dd56` adopted the initial staging-identity repair head, and follow-up merge `76df1bfa9f2fe2f2be4478b48131706fab390a60` adopted the final owner contract without force. The inherited owner source/test blob remains canonical; provider-cache does not maintain a copied publication implementation.

For the final receipt record, `write_immutable_receipt` continues to call the crate-private `private_evidence::write_object_bound_bytes_create_new(..., 0o400, None)` contract. #344 owns that contract as a publication facade: when no forbidden-root policy is requested it delegates to the canonical `private_directory_publication::write_private_bytes_create_new_with_parents(..., 0o400, 0o700)` primitive. Existing forbidden-root consumers retain the original parent-must-exist object-bound implementation because directory provisioning does not yet carry a forbidden-root policy.

On Unix, the private-directory primitive discovers the nearest admitted existing anchor, pins its descriptor and device/inode identity, creates each missing descendant with descriptor-relative `mkdirat`, opens it with `O_DIRECTORY|O_NOFOLLOW`, applies exact `0700` only to directories it created, fsyncs child and parent directories, revalidates the descriptor chain, and creates the final receipt with `openat(O_CREAT|O_EXCL|O_NOFOLLOW)` at `0400`. A pre-existing final parent must already be exact `0700`; DiskSage does not chmod it for convenience. Post-create namespace drift invalidates only the exact open record and never unlinks a visible replacement name.

Owner review found that object identity alone was not sufficient at finalization. Before #344 `b400437d...`, the exact file mode was checked before a post-write race window, but the final visible record was revalidated only for type/device/inode. A same-UID process could therefore widen the admitted `0400` or `0600` record before success. Test-only real-filesystem RED `41759c1d2531392d07263236f7eed1d58f2dce47` chmods the exact record to `0644` in that window and requires `private-directory-publication-file-mode-drift` plus exact-record invalidation. Production `b400437d5024504cb0e4156b2d940a905df5fdbc` revalidates final visible mode after the race window and evaluates exact file/directory modes with `0o7777`, so unexpected setuid/setgid/sticky bits also fail exact private-mode admission. The RED-head hosted Test `33970599396` remained queued when the production head advanced; no hosted RED conclusion is claimed.

A later review found the analogous final-parent gap when the final parent itself is the already-existing anchor. With no missing descendant names, post-write `revalidate_chain()` reasserted anchor identity and basic privacy but not the exact `0700` final-parent contract. Test-first real-filesystem commit `abbf1d2fe7758bfb6d51f23ea87a3c8c165fe5da` widens that exact parent to `0755` during the post-write race window and requires `private-directory-publication-directory-mode-drift`, no convenience chmod, and invalidation of only the admitted record. It is source/test RED; no hosted RED conclusion is claimed. Production owner repair `457515961fa1abaabc768061ce78d38c47dba911` revalidates the admitted final-parent descriptor at exact `0700` after the race-window chain check and before success.

Fresh review of the reusable atomic-replacement primitive then found a distinct source-name substitution window. The staging file was written and synced through its exact opened descriptor, but after the deterministic post-sync hook the implementation revalidated only the parent and destination before calling `renameat` with the staging pathname. A same-user namespace replacement could therefore cause different bytes to be renamed into the final record. Test-first commit `471b1525511f47f5529c8e3a30ac8d3198452bf6` removes the admitted staging name after sync, installs different bytes under the same name, and requires fail-closed behavior with the original destination preserved. Its hosted Test remained queued when production advanced, so it is source/test RED only. Production `4d8f6cc5cbe8bba2c51a46b925ea41abf24dd909` revalidates staging type/device/inode/mode against the exact opened file immediately before `renameat`, verifies the final name against that same file after publication, and replaces pathname unlink cleanup with exact-descriptor invalidation so a replacement staging name is never deleted. Follow-up `f192567dc6f25d1c9ba921346efa18c3c3287dba` preserves the existing domain error contract while keeping those checks.

That repair closes the deterministic post-sync substitution fixture but does not convert POSIX `renameat` into a source-handle-conditioned rename: the syscall still identifies its source by directory-relative name. The final pre-rename identity check therefore leaves a smaller check-to-rename interval that this ADR does not misrepresent as eliminated. This residual is a reason to keep ADR-0023 Proposed and to keep irreversible deletion unavailable; #342 likewise cannot claim full same-object replacement immunity solely from this primitive.

The consumer regression `1dc3c3e259f5fb31ab4d847b9f26c2156803f1ac` requires first-use Trash cleanup to create a missing receipt hierarchy safely rather than fail only because the directory is absent. After adopting #344, the same real-filesystem path must provision the receipt parent at exact `0700`, publish the receipt without owner-write bits, and only then allow the approved cache to move to Trash. The intermediate RED was queued when the production lineage advanced, so no hosted RED conclusion is claimed for that exact commit.

On non-Unix targets, provider-cache receipt publication still fails closed with `provider-cache-receipt-object-bound-publication-unsupported`; there is no Windows pathname fallback. Windows first-use parity therefore remains a release gap until the reusable owner provides native handle-relative directory creation and final-record publication with equivalent replacement-race, permission, and durability evidence.

Permanent provider-cache deletion may be reconsidered only after the canonical deletion-safety owner provides one implementation with stable object/directory authority through final mutation, ancestor/symlink/reparse/hardlink resistance, permission-drift resistance, durable pre-mutation recovery evidence, partial-failure handling, crash/power-loss recovery, and platform-specific acceptance for every platform where the mode is exposed. Publication authority is not deletion authority.

When `podman system df` fails, the plan is repair-required and offers only the read-only `podman system check --quick` diagnostic. DiskSage does not claim `--repair` succeeds and does not automatically run repair, blanket image/volume prune, or remove a referenced container or layer.

## Evidence and traceability

A redacted production observation admitted one superseded Edge installed-copy cache, EdgeUpdater's separately selected CRX cache, and one AppleHV seed cache. Their measured allocation was 1,797,688 KiB; an historical exact removal observation increased APFS availability by 1,792,412 KiB. A separate Podman observation showed damaged-layer references where normal/forced container removal and `system check --repair` did not establish safe host reclaim. A guest TRIM reported 99.5 GiB while host APFS increased only 87,708 KiB. These are operational observations, not release or irreversible-deletion authority.

The contract-repair lineage includes:

- `2207ca3121cb5fc29f2cbe56748abf50fe097fd0`: source-contract RED removing caller-selected Tauri cleanup mode.
- `00b4f4f0ab6a6153f82fa17fcb128cdf985ebab6` and `1b877bd33ac3c757a55ce72f22fd9b36f6f202d6`: source-contract REDs removing historical irreversible mode/result types from the public Rust facade; `216533bda068f5ea15ce28455ed5458e03819faa` and `f2ac8e3157bb03721fd4b37ca393db5ab108a938` provide the Trash-only DTO repair.
- `7db6dc77f0e57c60096cb7d20771f2eb39d0cd3c`: source-contract RED removing irreversible approval from the public plan; `86d1eeb3e35616c0b95d15b67dfb3ceb26b2574d` and `defac8ff38c84fa2d08efd1b9f9abdcf897a7799` provide the projected public plan/Tauri repair.
- `44107488869850df6b5d67810182618216ad961a`: test-only TypeScript payload RED; `112988abfc9bbcd8ccc7e5945cbdea636146392f` removes stale `exact_approval_phrase` and redundant `mode` payload.
- `80499b7a70ce4c1e86125fc308da7a21b6d1b9cd`: internal-authority RED; `717926e2a7744e3c45fadde6384aa1ac4f5e4698` rejects permanent mode before planning/receipt work.
- `b3fe5adf08685a35c3bfd87fa0539a0599f83e32`: source-contract RED requiring pathname-staged destructive helpers to be test-only and absent from production call edges; `d1b1df14ecbbe50573716801dfd93e7356f2665d` provides that structural repair.
- `511f373d4282c88410663a924196d074c9f81be8`: source-contract RED forbidding pathname unlink in post-create receipt failure cleanup; `727746b08b6320d44a813dec2b183a9382809130` invalidates only the exact open receipt and adds a real Unix replacement-record fixture.
- `e083c1224db6d531039c8a5f6bb64f10391b6be0`: source-contract RED requiring provider-cache final receipt publication to consume inherited object-bound create-new authority and forbidding a duplicate `OpenOptions`/pathname-open implementation.
- `a51fef56b79515b48581341f34f4018039475a9f`: production repair routing the final receipt record through the private-evidence publication contract at mode `0400`, with non-Unix fail-closed behavior and create-new/read-only acceptance.
- `53c1b68fc1bf1ae864a4af0f2a65dddfa0932709`: source-contract RED forbidding provider-cache-local `create_dir_all(receipt_dir)` and pathname permission normalization; `eb7a52bddb8fd73bb732c32e9b9f68777c42cb25` removes those mutations.
- `76ee3482c5b2ac9ab310383e3c221c8c459be26e`: prior real-filesystem acceptance proving the missing-parent fail-closed state before a safe owner primitive was available.
- `21d9444701bd5c52b0e63be2377bbe957a5e2444` / `64d68db08c3109799f8fe4d7b3a7291d9e5e3025`: #344 RED/production lineage for descriptor-relative missing-parent provisioning.
- `2ff22a9a4902c3cb87eab45f53d0466a1e1c3d9d` / `644de3439a9b5e02c591b4bf0ef305f7387074b5`: #344 RED/production lineage requiring an existing final private parent to be exact `0700` without chmod.
- `7f71a4cf12374c9f9db3789933c9915fd0346c6d` / `8c5fde535fabe405e38d1ac3d61a2b1e0e4a8f98`: #344 compatibility facade that preserves forbidden-root behavior while routing no-policy private records through the canonical private-directory owner.
- `1dc3c3e259f5fb31ab4d847b9f26c2156803f1ac`: consumer real-filesystem RED requiring safe first-use receipt-parent provisioning.
- `41759c1d2531392d07263236f7eed1d58f2dce47` / `b400437d5024504cb0e4156b2d940a905df5fdbc`: #344 real-filesystem final-record mode-drift RED and production repair.
- `abbf1d2fe7758bfb6d51f23ea87a3c8c165fe5da` / `457515961fa1abaabc768061ce78d38c47dba911`: #344 real-filesystem existing-final-parent mode-drift RED and production repair.
- `471b1525511f47f5529c8e3a30ac8d3198452bf6` / `4d8f6cc5cbe8bba2c51a46b925ea41abf24dd909` / `f192567dc6f25d1c9ba921346efa18c3c3287dba`: #344 deterministic staging-name substitution RED, descriptor-identity/invalidation repair, and final stable-error-contract owner head; #303 merge `76df1bfa9f2fe2f2be4478b48131706fab390a60` inherits that owner blob non-force.

Intermediate RED commits are source/test contract evidence only unless an exact-head hosted failure was actually observed.

## Consequences

DiskSage can surface exact regenerable provider caches and perform reversible Trash cleanup without exposing unsupported irreversible authority. On Unix, first-use provider-cache receipt publication no longer requires a caller to pre-create the receipt hierarchy: the canonical filesystem owner provisions missing private ancestors and publishes the final read-only create-new receipt through one descriptor-bound chain before Trash mutation proceeds.

Final success now requires the admitted record identity, exact record mode, and exact final-parent private mode to survive the final race window. The reusable atomic-replacement primitive additionally rejects the demonstrated post-sync staging-name substitution and invalidates only its exact opened staging object on failure. It still does not claim that POSIX `renameat` is source-handle-conditioned, so the narrower final check-to-rename race remains tracked instead of being hidden by an “object-bound” label.

The cross-platform publication gap remains: Windows must gain native handle-relative parity before the same claim applies there. ADR-0023 remains Proposed until applicable platform and deletion/recovery prerequisites are satisfied.

Permanent deletion remains unavailable. The historical permanent-purge variant, plan/result evidence, and filesystem fixtures remain crate-private/test-only evidence for future design; they are not a dormant commercial capability.

## Rejected alternatives

- Age-, name-, or process-absence-only deletion authority: it does not establish regeneration or current object identity.
- Treating private publication as irreversible deletion authority: publication and deletion have different rollback and crash-consistency invariants.
- Keeping permanent mode in public DTOs while merely rejecting it at runtime: it advertises an unavailable lifecycle.
- Leaving pathname-staged destructive helpers compiled or statically referenced behind an early return: future control-flow edits could reconnect the unsafe capability.
- Removing a failed create-new receipt or replacement staging file by pathname: the visible name can already refer to a different object.
- Treating a pre-`renameat` device/inode check as equivalent to a source-handle-conditioned rename: POSIX still resolves the source name at the mutation syscall.
- Treating device/inode equality alone as sufficient final publication evidence: the same admitted object can have its permission mode widened before success.
- Treating an existing final parent as exact-private only before record creation: same-user permission drift can widen that same directory before success.
- Masking only `0o777` while claiming an exact private mode: unexpected setuid/setgid/sticky bits are still mode drift.
- Maintaining a provider-cache-specific final-record writer after inheriting #344: it duplicates canonical filesystem authority and preserves avoidable replacement races.
- Creating or chmodding the receipt hierarchy by pathname inside provider-cache: it splits authority between an unsafe parent setup step and an object-bound record writer.
- Requiring a human or caller to pre-create the receipt hierarchy after the canonical owner can provision it safely: it leaves a buyer-visible first-use failure without adding safety.
- Shipping pathname-authorized permanent deletion with warnings: a warning does not repair mutation authority.
- Blanket Podman prune/repair: ambiguous images, volumes, damaged references, and provider state can contain or protect user data.
