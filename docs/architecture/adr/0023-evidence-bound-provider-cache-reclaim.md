# ADR-0023: Evidence-bound provider cache reclaim

- Status: Proposed
- Date: 2026-08-29
- Last reviewed: 2026-09-05

## Context

DiskSage can identify provider-owned local artifacts that are regenerable, including superseded Microsoft EdgeUpdater installed copies, EdgeUpdater `crx_cache`, and content-addressed Podman AppleHV machine seeds that are not the configured VM image. A provider label, old directory name, stopped process, or matching pathname is not deletion authority. Planning therefore has to establish the cache class, object/content identity, regeneration evidence, active-use evidence, and a fresh plan fingerprint before any mutation is considered.

OS Trash is the supported reversible product action. Permanent provider-cache deletion is materially different. The historical lower-level implementation stages a candidate through pathname rename and then removes it. Its identity/content checks, double fingerprint confirmation, receipt, and journal are useful test evidence, but they do not bind irreversible mutation to the same reviewed filesystem object across ancestor replacement, crash recovery, and Windows/Linux/macOS. Those destructive helpers therefore remain `#[cfg(test)]`; production has no static call edge to them.

Receipt publication is also a security boundary. A receipt that authorizes mutation must not be redirected through parent replacement, and failure cleanup must not unlink a same-name replacement created by another same-user process. Provider-cache must consume reusable filesystem authority rather than creating or chmodding receipt directories by pathname.

## Decision

DiskSage admits only these independently identified provider-cache classes:

1. A Podman AppleHV content-addressed `*.raw.zst` seed whose 64-hex key matches the full file digest, whose machine/configured-image evidence is known, and whose selected object is not the configured or active VM disk.
2. A Microsoft EdgeUpdater cached installed copy whose bundle version exactly equals its directory name and differs from the exact installed `/Applications` version. The installed version is retained.
3. EdgeUpdater `crx_cache` as a separate explicitly selectable regenerable candidate.

Planning is read-only and fails closed when inventory traversal, recreation evidence, content identity, or active-use evidence is incomplete. Execution re-plans and rechecks the selected (`path`, `evidence_fingerprint`, `object_id`) triplets against both approved fingerprints.

All shipped Rust facade, Tauri, TypeScript, and headless CLI mutation contracts expose Trash only. The external Rust plan contains `trash_approval_phrase` and no `exact_approval_phrase`; the Tauri command accepts no cleanup-mode argument; the TypeScript wrapper sends no mode field; and the CLI rejects `--permanent-purge` before manifest or executor work. The crate-private executor rejects `PermanentPurge` before re-planning or receipt creation. After that guard, production execution writes a Trash receipt, calls only `trash_delete_if_identity`, and returns the Trash mode.

#303 now adopts the current #344 private-publication owner exact `8c5fde535fabe405e38d1ac3d61a2b1e0e4a8f98` through non-force second-parent ancestry at `7b3b67e74e34ca3df8f06c9358819833f02fa27b`. The inherited owner blobs remain canonical; provider-cache does not maintain a copied private-directory implementation.

For the final receipt record, `write_immutable_receipt` continues to call the crate-private `private_evidence::write_object_bound_bytes_create_new(..., 0o400, None)` contract. #344 now owns that contract as a publication facade: when no forbidden-root policy is requested it delegates to the canonical `private_directory_publication::write_private_bytes_create_new_with_parents(..., 0o400, 0o700)` primitive. Existing forbidden-root consumers retain the original parent-must-exist object-bound implementation because directory provisioning does not yet carry a forbidden-root policy.

On Unix, the private-directory primitive discovers the nearest admitted existing anchor, pins its descriptor and device/inode identity, creates each missing descendant with descriptor-relative `mkdirat`, opens it with `O_DIRECTORY|O_NOFOLLOW`, applies exact `0700` only to directories it created, fsyncs child and parent directories, revalidates the descriptor chain, and creates the final receipt with `openat(O_CREAT|O_EXCL|O_NOFOLLOW)` at `0400`. A pre-existing final parent must already be exact `0700`; DiskSage does not chmod it for convenience. Post-create namespace drift invalidates only the exact open record and never unlinks a visible replacement name.

The consumer regression `1dc3c3e259f5fb31ab4d847b9f26c2156803f1ac` requires first-use Trash cleanup to create a missing receipt hierarchy safely rather than fail only because the directory is absent. After adopting #344, the same real-filesystem path must provision the receipt parent at exact `0700`, publish the receipt without owner-write bits, and only then allow the approved cache to move to Trash. The intermediate RED was queued when the production lineage advanced, so no hosted RED conclusion is claimed for that exact commit.

On non-Unix targets, provider-cache receipt publication still fails closed with `provider-cache-receipt-object-bound-publication-unsupported`; there is no Windows pathname fallback. Windows first-use parity therefore remains a release gap until the reusable owner provides native handle-relative directory creation and final-record publication with equivalent replacement-race and durability evidence.

Permanent provider-cache deletion may be reconsidered only after the canonical deletion-safety owner provides one implementation with stable object/directory authority through final mutation, ancestor/symlink/reparse/hardlink resistance, durable pre-mutation recovery evidence, partial-failure handling, crash/power-loss recovery, and platform-specific acceptance for every platform where the mode is exposed. Publication authority is not deletion authority.

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
- `1dc3c3e259f5fb31ab4d847b9f26c2156803f1ac`: consumer real-filesystem RED requiring safe first-use receipt-parent provisioning; `7b3b67e74e34ca3df8f06c9358819833f02fa27b` adopts the current owner non-force.

Intermediate RED commits are source/test contract evidence only unless an exact-head hosted failure was actually observed.

## Consequences

DiskSage can surface exact regenerable provider caches and perform reversible Trash cleanup without exposing unsupported irreversible authority. On Unix, first-use provider-cache receipt publication no longer requires a caller to pre-create the receipt hierarchy: the canonical filesystem owner provisions missing private ancestors and publishes the final read-only create-new receipt through one descriptor-bound chain before Trash mutation proceeds.

This closes the Unix first-use availability gap without reintroducing consumer-owned pathname mutation. The cross-platform publication gap remains: Windows must gain native handle-relative parity before the same claim applies there. ADR-0023 remains Proposed until applicable platform and deletion/recovery prerequisites are satisfied.

Permanent deletion remains unavailable. The historical permanent-purge variant, plan/result evidence, and filesystem fixtures remain crate-private/test-only evidence for future design; they are not a dormant commercial capability.

## Rejected alternatives

- Age-, name-, or process-absence-only deletion authority: it does not establish regeneration or current object identity.
- Treating private publication as irreversible deletion authority: publication and deletion have different rollback and crash-consistency invariants.
- Keeping permanent mode in public DTOs while merely rejecting it at runtime: it advertises an unavailable lifecycle.
- Leaving pathname-staged destructive helpers compiled or statically referenced behind an early return: future control-flow edits could reconnect the unsafe capability.
- Removing a failed create-new receipt by pathname: the visible name can already refer to a different object.
- Maintaining a provider-cache-specific final-record writer after inheriting #344: it duplicates canonical filesystem authority and preserves avoidable replacement races.
- Creating or chmodding the receipt hierarchy by pathname inside provider-cache: it splits authority between an unsafe parent setup step and an object-bound record writer.
- Requiring a human or caller to pre-create the receipt hierarchy after the canonical owner can provision it safely: it leaves a buyer-visible first-use failure without adding safety.
- Shipping pathname-authorized permanent deletion with warnings: a warning does not repair mutation authority.
- Blanket Podman prune/repair: ambiguous images, volumes, damaged references, and provider state can contain or protect user data.
