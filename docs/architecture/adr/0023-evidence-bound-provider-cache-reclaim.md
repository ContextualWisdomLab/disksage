# ADR-0023: Evidence-bound provider cache reclaim

- Status: Proposed
- Date: 2026-08-29
- Last reviewed: 2026-09-05

## Context

DiskSage can identify provider-owned local artifacts that are regenerable, including superseded Microsoft EdgeUpdater installed copies, EdgeUpdater `crx_cache`, and content-addressed Podman AppleHV machine seeds that are not the configured VM image. A provider label, old directory name, stopped process, or matching pathname is not deletion authority. Planning therefore has to establish the cache class, object/content identity, regeneration evidence, active-use evidence, and a fresh plan fingerprint before any mutation is considered.

OS Trash is the supported reversible product action. Permanent provider-cache deletion is materially different. The historical lower-level implementation stages a candidate through pathname rename and then removes it. Its identity/content checks, double fingerprint confirmation, receipt, and journal are useful test evidence, but they do not bind irreversible mutation to the same reviewed filesystem object across ancestor replacement, crash recovery, and Windows/Linux/macOS. Those destructive helpers therefore remain `#[cfg(test)]`; production has no static call edge to them.

Receipt publication is also a security boundary. A receipt that authorizes a later mutation must not be redirected through parent replacement, and failure cleanup must not unlink a same-name replacement created by another same-user process.

## Decision

DiskSage admits only these independently identified provider-cache classes:

1. A Podman AppleHV content-addressed `*.raw.zst` seed whose 64-hex key matches the full file digest, whose machine/configured-image evidence is known, and whose selected object is not the configured or active VM disk.
2. A Microsoft EdgeUpdater cached installed copy whose bundle version exactly equals its directory name and differs from the exact installed `/Applications` version. The installed version is retained.
3. EdgeUpdater `crx_cache` as a separate explicitly selectable regenerable candidate.

Planning is read-only and fails closed when inventory traversal, recreation evidence, content identity, or active-use evidence is incomplete. Execution re-plans and rechecks the selected (`path`, `evidence_fingerprint`, `object_id`) triplets against both approved fingerprints.

All shipped Rust facade, Tauri, TypeScript, and headless CLI mutation contracts expose Trash only. The external Rust plan contains `trash_approval_phrase` and no `exact_approval_phrase`; the Tauri command accepts no cleanup-mode argument; the TypeScript wrapper sends no mode field; and the CLI rejects `--permanent-purge` before manifest or executor work. The crate-private executor rejects `PermanentPurge` before re-planning or receipt creation. After that guard, production execution writes a Trash receipt, calls only `trash_delete_if_identity`, and returns the Trash mode.

#303 adopts #344 exact `2a23a1d7de5b929a76b432c192d2b9e537fbbbdd` through non-force two-parent ancestry (`ff0468dbfa0bbe87d6f0fbd128d85e243ffa0932`). Provider-cache therefore consumes the canonical Unix private-record create-new primitive rather than copying it.

For the final receipt record, `write_immutable_receipt` serializes the receipt and delegates publication to `crate::private_evidence::write_object_bound_bytes_create_new(..., 0o400, None)`. On Unix that owner primitive pins the admitted parent directory, creates with descriptor-relative `openat(O_CREAT|O_EXCL|O_NOFOLLOW)`, writes and syncs the exact open record, syncs the containing directory, verifies the final record identity, and invalidates the exact open record on post-create failure rather than unlinking a pathname. Successful provider-cache receipts are therefore create-new and owner-read-only. Provider-cache does not own a second `OpenOptions`/pathname-open implementation and does not reopen the containing directory by pathname for final-record durability.

On non-Unix targets, provider-cache receipt publication fails closed with `provider-cache-receipt-object-bound-publication-unsupported`; there is no Windows pathname fallback. This is intentionally conservative until #344 gains native Windows handle-relative parity.

This decision does **not** claim that provider-cache receipt-directory provisioning is fully object-bound. The command currently derives an application-data receipt path and `write_immutable_receipt` still provisions the parent with pathname-based `create_dir_all` and Unix permission normalization before delegating the final record to #344. Safe private-directory provisioning/ancestor authority is the next publication gap. It must be repaired in or through the canonical reusable filesystem owner rather than by forking another provider-cache-specific primitive.

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
- `b3fe5adf08685a35c3bfd87fa0539a0599f83e32`: source-contract RED requiring the pathname-staged destructive helpers to be test-only and absent from production call edges; `d1b1df14ecbbe50573716801dfd93e7356f2665d` provides that structural repair.
- `511f373d4282c88410663a924196d074c9f81be8`: source-contract RED forbidding pathname unlink in post-create receipt failure cleanup; `727746b08b6320d44a813dec2b183a9382809130` invalidates only the exact open receipt and adds a real Unix replacement-record fixture.
- `e083c1224db6d531039c8a5f6bb64f10391b6be0`: source-contract RED requiring provider-cache final receipt publication to consume the inherited #344 object-bound create-new primitive and forbidding a duplicate `OpenOptions`/`File::open(receipt_dir)` implementation.
- `a51fef56b79515b48581341f34f4018039475a9f`: production repair routing the final receipt record through #344 at mode `0400`, with non-Unix fail-closed behavior and create-new/read-only acceptance.

Intermediate RED commits are source/test contract evidence only unless an exact-head hosted failure was actually observed. No such hosted failure is claimed for `e083c122...`.

## Consequences

DiskSage can surface exact regenerable provider caches and perform reversible Trash cleanup without exposing unsupported irreversible authority. The final provider-cache receipt record now inherits the same Unix object-bound create-new authority used by the reusable private-publication foundation, so provider-cache no longer carries a second pathname final-record writer.

The remaining publication risk is narrower but real: pathname-based creation/admission and permission normalization of the receipt parent directory must be replaced by or delegated to canonical private-directory provisioning authority, with Windows handle-relative parity before Windows mutation is enabled. Until then the ADR remains Proposed.

Permanent deletion remains unavailable. The historical permanent-purge variant, plan/result evidence, and filesystem fixtures remain crate-private/test-only evidence for future design; they are not a dormant commercial capability.

## Rejected alternatives

- Age-, name-, or process-absence-only deletion authority: it does not establish regeneration or current object identity.
- Treating private publication as irreversible deletion authority: publication and deletion have different rollback and crash-consistency invariants.
- Keeping permanent mode in public DTOs while merely rejecting it at runtime: it advertises an unavailable lifecycle.
- Leaving pathname-staged destructive helpers compiled or statically referenced behind an early return: future control-flow edits could reconnect the unsafe capability.
- Removing a failed create-new receipt by pathname: the visible name can already refer to a different object.
- Maintaining a provider-cache-specific final-record `OpenOptions`/directory-sync writer after inheriting #344: it duplicates canonical filesystem authority and preserves avoidable replacement races.
- Claiming the whole receipt pipeline object-bound after fixing only final-record publication: receipt-parent provisioning is still pathname-based and remains an explicit gap.
- Shipping pathname-authorized permanent deletion with warnings: a warning does not repair mutation authority.
- Blanket Podman prune/repair: ambiguous images, volumes, damaged references, and provider state can contain or protect user data.
