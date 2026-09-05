# ADR-0023: Evidence-bound provider cache reclaim

- Status: Proposed
- Date: 2026-08-29
- Last reviewed: 2026-09-06

## Context

DiskSage identifies provider-owned local artifacts that are regenerable, including superseded Microsoft EdgeUpdater installed copies, EdgeUpdater `crx_cache`, and content-addressed Podman AppleHV machine seeds that are not the configured VM image. Provider labels, old directory names, stopped processes, and matching pathnames are evidence inputs, not mutation authority. Planning therefore establishes cache class, object/content identity, regeneration evidence, active-use evidence, and a fresh plan fingerprint before mutation is considered.

OS Trash is the supported reversible product action. Permanent provider-cache deletion is materially different. Historical pathname-staged purge helpers remain `#[cfg(test)]`; production has no static call edge to them. The crate-private executor rejects `PermanentPurge` before re-planning, receipt creation, or filesystem mutation. Public Rust, Tauri, TypeScript, and headless CLI contracts expose Trash only.

Receipt publication is a separate security boundary. A receipt that authorizes a Trash operation must not be redirected through parent replacement, symlink substitution, permission or special-bit drift, or same-name substitution. Failure cleanup must not unlink a same-name replacement created by another same-user process. Provider-cache consumes the reusable filesystem owner rather than maintaining its own pathname publication implementation.

## Decision

DiskSage admits only independently evidenced provider-cache classes. A Podman AppleHV `*.raw.zst` seed is admissible only when its 64-hex content key matches the full file digest, machine/configured-image evidence is known, and the object is not the configured or active VM disk. A Microsoft EdgeUpdater installed-copy cache is admissible only when the bundle version exactly equals its directory name, differs from the exact installed `/Applications` version, and the installed version is retained. EdgeUpdater `crx_cache` remains a separate selectable class.

Planning is read-only and fails closed when inventory traversal, recreation evidence, content identity, or active-use evidence is incomplete. Execution re-plans and rechecks the selected (`path`, `evidence_fingerprint`, `object_id`) triplets against both approved fingerprints.

The external Rust plan contains `trash_approval_phrase` and no irreversible approval phrase. Tauri accepts no cleanup-mode argument, the TypeScript wrapper sends no mode field, and the CLI rejects `--permanent-purge` before manifest or executor work. Production execution writes a create-new Trash receipt, calls only the Trash mutation boundary, and returns the Trash mode.

### Reusable filesystem owner

#303 now adopts exact filesystem owner #344 `280a0059e14374d6bbee667fb899de511c5bb311` by non-force second-parent merge `0c2587570eed5c029104f2ce55961d193462083a`. Compare ancestry must keep #344 as merge base with `behind_by=0`; predecessor owner heads are historical evidence only. The inherited filesystem source and tests remain canonical; provider-cache does not copy them.

#344 separates two authorities that must not be conflated:

- **Create-new private publication** remains available on Unix through descriptor-relative private-directory/private-evidence primitives. Missing private descendants are created with `mkdirat`, opened with `O_DIRECTORY|O_NOFOLLOW`, set to exact private modes, fsynced, and revalidated by descriptor identity. The final receipt is created with `openat(O_CREAT|O_EXCL|O_NOFOLLOW)` and exact mode checks include the full `0o7777` permission/special-bit mask. Post-create failure invalidates only the admitted opened record rather than unlinking an untrusted replacement name.
- **Existing-record replacement** is unavailable. The final POSIX replacement primitive formerly used `renameat` with a directory-relative source name after revalidation. That syscall still re-resolves the source name, leaving a check-to-mutation interval in which another same-UID process could substitute a different object. Another pathname check does not remove that semantic gap. Current #344 therefore returns `object-bound-replace-source-identity-unavailable` before filesystem lookup or mutation for otherwise valid Unix replacement requests. Non-Unix replacement remains unsupported until a native owner proves equivalent same-object semantics.

This distinction is material to provider-cache. Final Trash receipts are create-new evidence and continue to use the proven create-new path. Provider-cache does not infer mutable-record replacement or irreversible deletion authority from that create-new capability.

The owner repair lineage includes real-filesystem and source-contract tests for missing-parent provisioning, existing-parent exact `0700`, record mode drift, setuid/setgid/sticky drift through full `0o7777`, staging-name substitution, and exact-record invalidation. The final source-object replacement finding is represented by `src-tauri/tests/object_bound_publication_source_identity_contract.rs`; production #344 `280a0059...` resolves it by failing closed rather than by claiming `renameat` is source-handle-conditioned.

Exact-head Test run `33983161163` completed successfully for #344 `280a0059...`. The Draft Release run is skipped and is not release evidence. This Test result permits consumer adoption of that exact owner head; it does not by itself authorize #344 or its consumers to merge to protected `main` or publish a release.

### Provider-cache receipt and deletion boundary

`write_immutable_receipt` uses `private_evidence::write_object_bound_bytes_create_new(..., 0o400, None)`. On Unix, the no-forbidden-root path delegates to canonical private-directory create-new publication with a `0700` parent chain and `0400` final receipt. Existing forbidden-root consumers retain their stricter parent-must-exist contract until directory provisioning carries the same forbidden-root policy. On non-Unix targets provider-cache receipt publication remains fail closed; there is no Windows pathname fallback.

Permanent provider-cache deletion remains unavailable. Publication authority is not deletion authority. Reconsidering irreversible deletion requires the canonical deletion-safety owner to prove stable object/directory authority through final mutation, ancestor/symlink/reparse/hardlink resistance, permission drift resistance, durable pre-mutation journal/receipt evidence, partial-failure handling, crash or power-loss recovery, undo/recovery semantics, and platform-specific acceptance for Windows, Linux, and macOS wherever the capability is exposed.

When `podman system df` fails, planning remains repair-required and exposes only the bounded read-only `podman system check --quick` diagnostic. DiskSage does not claim `--repair` succeeds and does not automatically execute repair, blanket image/volume prune, or removal of referenced containers or layers.

## Evidence and traceability

Historical RED/repair pairs remain useful evidence but do not supersede the current owner contract. Key lineage includes:

- `2207ca3121cb5fc29f2cbe56748abf50fe097fd0`: source-contract RED removing caller-selected Tauri cleanup mode.
- `00b4f4f0ab6a6153f82fa17fcb128cdf985ebab6`, `1b877bd33ac3c757a55ce72f22fd9b36f6f202d6`, `216533bda068f5ea15ce28455ed5458e03819faa`, `f2ac8e3157bb03721fd4b37ca393db5ab108a938`: Trash-only Rust facade repair.
- `7db6dc77f0e57c60096cb7d20771f2eb39d0cd3c`, `86d1eeb3e35616c0b95d15b67dfb3ceb26b2574d`, `defac8ff38c84fa2d08efd1b9f9abdcf897a7799`: irreversible approval removal from public plan/Tauri wire schema.
- `44107488869850df6b5d67810182618216ad961a`, `112988abfc9bbcd8ccc7e5945cbdea636146392f`: TypeScript plan/payload alignment.
- `80499b7a70ce4c1e86125fc308da7a21b6d1b9cd`, `717926e2a7744e3c45fadde6384aa1ac4f5e4698`, `b3fe5adf08685a35c3bfd87fa0539a0599f83e32`, `d1b1df14ecbbe50573716801dfd93e7356f2665d`: internal permanent-mode admission and production-call-edge removal.
- `511f373d4282c88410663a924196d074c9f81be8`, `727746b08b6320d44a813dec2b183a9382809130`: exact-record cleanup instead of pathname unlink.
- `e083c1224db6d531039c8a5f6bb64f10391b6be0`, `a51fef56b79515b48581341f34f4018039475a9f`, `53c1b68fc1bf1ae864a4af0f2a65dddfa0932709`, `eb7a52bddb8fd73bb732c32e9b9f68777c42cb25`: provider-cache consumption of reusable create-new receipt authority and removal of local pathname directory mutation.
- `21d9444701bd5c52b0e63be2377bbe957a5e2444`, `64d68db08c3109799f8fe4d7b3a7291d9e5e3025`, `2ff22a9a4902c3cb87eab45f53d0466a1e1c3d9d`, `644de3439a9b5e02c591b4bf0ef305f7387074b5`: descriptor-relative missing-parent and exact-existing-parent create-new owner lineage.
- `41759c1d2531392d07263236f7eed1d58f2dce47`, `b400437d5024504cb0e4156b2d940a905df5fdbc`, `abbf1d2fe7758bfb6d51f23ea87a3c8c165fe5da`, `457515961fa1abaabc768061ce78d38c47dba911`: final-record and final-parent mode-drift fixtures/repairs.
- `471b1525511f47f5529c8e3a30ac8d3198452bf6`, `4d8f6cc5cbe8bba2c51a46b925ea41abf24dd909`, `f192567dc6f25d1c9ba921346efa18c3c3287dba`, `8c9c2f4793f20d8ca01662d8c53239a415108b04`, `431b192f1630aaf34b4c09dd72c3ff4897fd5789`: staging substitution and full special-bit revalidation lineage before the remaining source-name semantic gap was made explicit.
- `182cbdc4430757676737d7e804059203da4a201a`: executable contract that rejects raw pathname-source replacement as same-object authority.
- `280a0059e14374d6bbee667fb899de511c5bb311`: current #344 production owner head; existing-record replacement fails before filesystem mutation while proven create-new publication remains available.
- `0c2587570eed5c029104f2ce55961d193462083a`: #303 non-force adoption of exact current #344.

Intermediate RED commits are source/test contract evidence only unless a hosted failing result was actually observed.

## Consequences

DiskSage can surface exact regenerable provider caches and perform reversible Trash cleanup only where the receipt/publication and Trash mutation boundaries are actually supported. On Unix, first-use create-new receipt publication remains available. Existing-record replacement is deliberately unavailable rather than simulated with a pathname rename. Windows native-handle publication/replacement parity remains a release gap.

ADR-0023 stays Proposed until the applicable exact head has terminal passing required checks and the deletion/recovery prerequisites in Issue #170 are satisfied. No predecessor check, Draft Release skip, or mechanically mergeable state is release evidence.