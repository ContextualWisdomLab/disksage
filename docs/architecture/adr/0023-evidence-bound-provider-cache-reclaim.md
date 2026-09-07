# ADR-0023: Evidence-bound provider cache reclaim

- Status: Proposed
- Date: 2026-08-29
- Last reviewed: 2026-09-08

## Context

DiskSage identifies provider-owned local artifacts that are regenerable, including superseded Microsoft EdgeUpdater installed copies, EdgeUpdater `crx_cache`, and content-addressed Podman AppleHV machine seeds that are not the configured VM image. Provider labels, old directory names, stopped processes, and matching pathnames are evidence inputs, not mutation authority. Planning therefore establishes cache class, object/content identity, regeneration evidence, active-use evidence, and a fresh plan fingerprint before mutation is considered.

OS Trash is the supported reversible product action. Permanent provider-cache deletion is materially different. Historical pathname-staged purge helpers remain `#[cfg(test)]`; production has no static call edge to them. The crate-private executor rejects `PermanentPurge` before re-planning, receipt creation, or filesystem mutation. Public Rust, Tauri, TypeScript, and headless CLI contracts expose Trash only.

Receipt publication is a separate security boundary. A receipt that authorizes a Trash operation must not be redirected through parent replacement, symlink substitution, permission or special-bit drift, or same-name substitution. Failure cleanup must not unlink a same-name replacement created by another same-user process. Provider-cache consumes the reusable filesystem owner rather than maintaining its own pathname publication implementation.

## Decision

DiskSage admits only independently evidenced provider-cache classes. A Podman AppleHV `*.raw.zst` seed is admissible only when its 64-hex content key matches the full file digest, machine/configured-image evidence is known, and the object is not the configured or active VM disk. A Microsoft EdgeUpdater installed-copy cache is admissible only when the bundle version exactly equals its directory name, differs from the exact installed `/Applications` version, and the installed version is retained. EdgeUpdater `crx_cache` remains a separate selectable class.

Planning is read-only and fails closed when inventory traversal, recreation evidence, content identity, or active-use evidence is incomplete. Execution re-plans and rechecks the selected (`path`, `evidence_fingerprint`, `object_id`) triplets against both approved fingerprints.

The external Rust plan contains `trash_approval_phrase` and no irreversible approval phrase. Tauri accepts no cleanup-mode argument, the TypeScript wrapper sends no mode field, and the CLI rejects `--permanent-purge` before manifest or executor work. Production execution writes a create-new Trash receipt, calls only the Trash mutation boundary, and returns the Trash mode.

### Reusable filesystem owner

#303 consumes the reusable private-publication owner from #344 through ordinary non-force ancestry. The current owner head is `736da6db1fb0918d998b3f4d240c63936c91b11d`; predecessor owner heads remain historical evidence only. Provider-cache does not copy the filesystem implementation or widen it locally.

#344 separates two authorities that must not be conflated:

- **Create-new private publication** is available on Unix only when the no-policy final parent already exists as an exact owner-private `0700` directory. The final record is opened descriptor-relatively with create-new/no-follow semantics, checked at the full `0o7777` permission/special-bit mask, and failure cleanup invalidates only the admitted opened record. The owner intentionally does not claim missing-directory provisioning: POSIX `mkdirat()` reports status but does not return an opened handle for the object it created, so a later `openat()` cannot prove that a same-UID replacement did not win the name between creation and authority acquisition.
- **Existing-record replacement** is unavailable. A pathname-source `renameat()` still re-resolves the source name and cannot prove that final mutation consumes the exact reviewed source object. Current #344 therefore returns `object-bound-replace-source-identity-unavailable` before filesystem lookup or mutation for otherwise valid Unix replacement requests. Non-Unix replacement remains unsupported until a native owner proves equivalent same-object semantics.

This distinction is material to provider-cache. Final Trash receipts are create-new evidence, but create-new authority exists only under a pre-existing exact-private parent. Provider-cache does not infer directory provisioning, mutable-record replacement, or irreversible deletion authority from that capability.

The owner lineage contains real-filesystem contracts for missing-parent fail-closed behavior, existing-parent exact `0700`, record mode drift, setuid/setgid/sticky drift through full `0o7777`, staging-name substitution, bounded byte verification, and exact-record invalidation. Current #344 Test `34050767861` is terminal success on exact `736da6db...`; that evidence establishes the owner behavior on that head but does not transfer as #303 merge or release evidence.

### Provider-cache receipt and deletion boundary

`write_immutable_receipt` uses `private_evidence::write_object_bound_bytes_create_new(..., 0o400, None)`. On Unix, the no-policy path therefore requires the receipt directory itself to pre-exist at exact `0700`; it does not create missing `receipts/provider-cache` descendants and does not chmod an existing wider directory. Missing or mode-drifted parent authority fails receipt publication before provider-cache mutation. On non-Unix targets provider-cache receipt publication remains fail closed; there is no Windows pathname fallback.

The shipped Tauri path currently derives `app_data_dir()/receipts/provider-cache` but has no accepted object-bound bootstrap authority for that nested directory. This is a buyer-visible prerequisite gap, not permission to reintroduce pathname provisioning in #303. Until the canonical filesystem owner supplies a cross-platform accepted bootstrap/handle authority or installation lifecycle pre-provisions and proves the exact-private parent, first-use cleanup must fail closed rather than mutate without a receipt.

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
- `21d9444701bd5c52b0e63be2377bbe957a5e2444`, `64d68db08c3109799f8fe4d7b3a7291d9e5e3025`, `2ff22a9a4902c3cb87eab45f53d0466a1e1c3d9d`, `644de3439a9b5e02c591b4bf0ef305f7387074b5`: historical directory-publication exploration; current owner supersedes the earlier missing-parent provisioning claim and requires a pre-existing exact-private parent.
- `41759c1d2531392d07263236f7eed1d58f2dce47`, `b400437d5024504cb0e4156b2d940a905df5fdbc`, `abbf1d2fe7758bfb6d51f23ea87a3c8c165fe5da`, `457515961fa1abaabc768061ce78d38c47dba911`: final-record and final-parent mode-drift fixtures/repairs.
- `471b1525511f47f5529c8e3a30ac8d3198452bf6`, `4d8f6cc5cbe8bba2c51a46b925ea41abf24dd909`, `f192567dc6f25d1c9ba921346efa18c3c3287dba`, `8c9c2f4793f20d8ca01662d8c53239a415108b04`, `431b192f1630aaf34b4c09dd72c3ff4897fd5789`: staging substitution and full special-bit revalidation lineage before the remaining source-name semantic gap was made explicit.
- `182cbdc4430757676737d7e804059203da4a201a`: executable contract that rejects raw pathname-source replacement as same-object authority.
- `280a0059e14374d6bbee667fb899de511c5bb311`: historical #344 production head that removed existing-record replacement authority.
- `736da6db1fb0918d998b3f4d240c63936c91b11d`: current #344 owner head; create-new publication requires the pre-existing exact-private parent and existing-record replacement remains unavailable.
- `073408eb870d760bb5846a45e9fb692492bdd5bc`: #303 regression that proves missing receipt-parent authority fails before cache mutation, then succeeds only after an exact `0700` parent is explicitly present.
- `cd8ad472f94650162f070cf04868b969ee7c09e3`: #303 unit-fixture repair binding receipt publication to the same exact-private parent contract rather than relying on runner umask.

Intermediate RED commits are source/test contract evidence only unless a hosted failing result was actually observed.

## Consequences

DiskSage can surface exact regenerable provider caches and perform reversible Trash cleanup only where receipt/publication and Trash mutation boundaries are actually supported. On Unix, create-new receipt publication is available under a pre-existing exact `0700` parent; first-use creation of that parent is not currently authorized by the reusable owner. Existing-record replacement is deliberately unavailable rather than simulated with a pathname rename. Windows native-handle publication/replacement parity remains a release gap.

ADR-0023 stays Proposed until the applicable exact head has terminal passing required checks and the deletion/recovery prerequisites in Issue #170 are satisfied. No predecessor check, Draft Release skip, or mechanically mergeable state is release evidence.
