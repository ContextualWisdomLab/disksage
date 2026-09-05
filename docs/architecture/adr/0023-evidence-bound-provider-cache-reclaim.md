# ADR-0023: Evidence-bound provider cache reclaim

- Status: Proposed
- Date: 2026-08-29

## Context

DiskSage may observe large local artifacts that a provider can regenerate, including superseded
Microsoft EdgeUpdater copies, its separately identified CRX cache, and content-addressed Podman
AppleHV machine seeds. A local process being stopped, an old-looking directory name, or a provider
claim that content is cached is not deletion authority. Planning must therefore establish the exact
cache class, object/content identity, regeneration evidence, active-use evidence, and a fresh plan
fingerprint before any mutation is considered.

OS Trash is a reversible product action. Permanent deletion is materially different: the historical
lower-level implementation in this branch stages an exact Podman seed with a same-parent pathname
rename and then removes it. Its identity/content checks, double fingerprint confirmation, private
receipt, and terminal journal improve evidence but do not bind the irreversible mutation to the same
validated filesystem object/directory authority across ancestor replacement, crash recovery, and all
supported platforms. The pathname-staged implementation is retained only behind `#[cfg(test)]` as
repair and fixture evidence. The lower-level production `execute` boundary rejects
`PermanentPurge` before re-planning or receipt creation and, after that guard, contains only the Trash
execution path. Production code therefore has no static call edge to the historical permanent-purge
helper.

Earlier active DiskSage owner lineages already allocate ADR-0012 through ADR-0022. This decision uses
ADR-0023 rather than reusing an immutable architecture identity. It remains Proposed until its
runtime prerequisites, exact-current tests, review, and protected integration are complete.

## Decision

DiskSage admits only these independently identified cache classes:

1. A Podman AppleHV content-addressed `*.raw.zst` machine seed whose 64-hex key matches the full file
   digest, whose machine and configured image evidence are known, and whose selected object is not
   the configured/active VM disk.
2. A Microsoft EdgeUpdater cached installed copy whose bundle version exactly equals its directory
   name and differs from the exact installed `/Applications` version. The installed version is
   retained.
3. EdgeUpdater `crx_cache` as a separate, explicitly selectable regenerable candidate.

Planning is read-only. It fails closed when inventory traversal, recreation evidence, content
identity, or active-use evidence is incomplete. Execution re-plans and rechecks the selected
candidate triplets (`path`, `evidence_fingerprint`, `object_id`) against the approved plan.

All public Rust, Tauri, and TypeScript mutation surfaces expose **Trash only**. The public Rust facade
owns its `ProviderCacheReclaimPlan` instead of re-exporting the historical lower-level plan; the
commercial plan contains `trash_approval_phrase` and has no `exact_approval_phrase` field. The Tauri
planning command projects the lower-level evidence into that same facade-owned DTO, so unavailable
irreversible approval is omitted from the serialized schema rather than emitted as `null`. Tauri
execution has no caller-selected cleanup-mode argument, delegates every product call as internal
Trash, and projects the result through the facade-owned one-variant
`ProviderCacheCleanupMode::Trash` DTO.

The TypeScript `ProviderCacheReclaimPlan` mirrors that wire schema: it exposes
`trash_approval_phrase` but no `exact_approval_phrase`. Its execution wrapper accepts no cleanup-mode
argument and sends no `mode` field to `execute_provider_cache_reclaim`; the backend therefore remains
the sole authority for the fixed Trash product action rather than accepting a redundant client-side
mode assertion.

The headless CLI imports the public `provider_cache` facade, rejects `--permanent-purge` before
manifest or executor work with `provider-cache-identity-bound-permanent-delete-unavailable`, and calls
only `execute_trash`. The historical `ProviderCacheCleanupMode` and lower-level plan/result remain
crate-private so that prior evidence can be interpreted, while pathname-staged irreversible helpers
and rollback hooks compile only for tests. The crate-private production `execute` boundary rejects
`PermanentPurge` with the same stable error before re-planning or receipt creation. Its remaining
production path then uses the Trash approval phrase, writes a Trash receipt, invokes only
`trash_delete_if_identity`, and returns the Trash mode. Internal call sites therefore cannot revive
pathname-based permanent deletion by bypassing the public facade or by relying on an unreachable
match arm.

Provider-cache now inherits the reusable Unix private-publication foundation from #344 through an
actual non-force two-parent ancestry adoption rather than source copying. Receipt finalization has an
additional invariant: once the receipt file has been opened create-new, an error path must never
unlink the visible pathname. A same-user actor can replace that name while the original file remains
open. Failure therefore invalidates only the exact open receipt by truncating and syncing that file
descriptor. A zero-length private tombstone may remain when the original name still resolves to the
opened object. Deleting such a tombstone is an explicit later operation, not error cleanup.

This repair does not yet claim that provider-cache receipt creation itself is fully object-bound. The
current receipt directory creation, permission update, record open, and containing-directory sync are
still pathname-based in the provider-cache writer. The adopted #344 foundation supplies the canonical
Unix descriptor-relative publication primitive and must be consumed rather than copied when that
remaining receipt-publication gap is repaired. Windows remains fail closed wherever native handle
parity is absent.

Permanent provider-cache deletion may be reconsidered only after the canonical deletion-safety owner
provides one implementation with stable object/directory authority through staging and deletion,
ancestor/symlink/reparse/hardlink resistance, durable pre-mutation recovery evidence, partial-failure
recovery, and platform-specific acceptance for every platform where the mode is exposed. Reusable
private-publication evidence does not by itself authorize deletion semantics.

When `podman system df` fails, the plan is `repair-required` and offers only the read-only
`podman system check --quick` diagnostic. DiskSage does not claim `--repair` succeeds and does not
automatically run repair, blanket image/volume prune, or remove a referenced container or layer.

## Evidence

A redacted production observation admitted one superseded Edge installed-copy cache, EdgeUpdater's
separately selected CRX cache, and one AppleHV seed cache. Their measured allocation was 1,797,688
KiB; an historical exact removal observation increased APFS availability by 1,792,412 KiB. No active
raw VM or user-data path was selected. A separate Podman observation found that `system df` failed on
a damaged layer referenced by an exited container; ordinary and forced container removal failed,
and `system check --repair` also failed because the layer remained in use. A guest TRIM reported
99.5 GiB while host APFS increased only 87,708 KiB, so guest-reported trim is not host reclaim proof.

These observations are operational evidence, not release or irreversible-deletion authority.
Source-contract RED `2207ca3121cb5fc29f2cbe56748abf50fe097fd0` first removed caller-selected
mode from the shipped Tauri schema. Follow-up source-contract RED
`36850bf30e41e1fdb716ce576479ad3b3dc86e4e` requires the historical lower-level executor to be
crate-private, requires one public Trash-only Rust facade, and requires the CLI to consume that
facade. The first facade implementation is `febaf0ee5e58a2b647030ec4c025563b078faffb`;
`7b3fc0c1196a36ae298603e6be1b8837727f46d0` removes public module reachability and
`c6ed71d0e932c92cc635205436deef57d70b40c4` routes the CLI through `execute_trash`.

Fresh review then found that `provider_cache.rs` still publicly re-exported the historical cleanup
mode, including the `PermanentPurge` variant, and re-exported the lower-level result whose public
`mode` field used that historical type. Source-contract REDs
`00b4f4f0ab6a6153f82fa17fcb128cdf985ebab6` and
`1b877bd33ac3c757a55ce72f22fd9b36f6f202d6` require the external Rust facade to stop re-exporting
those internal mode/result contracts. Production repairs
`216533bda068f5ea15ce28455ed5458e03819faa` and
`f2ac8e3157bb03721fd4b37ca393db5ab108a938` keep the historical mode internal and project the
commercial result through a Trash-only public DTO. A defensive projection check fails closed if an
internal non-Trash result ever crosses `execute_trash`.

A subsequent contract review found the same class of leakage in the read-only plan: the public facade
still re-exported the historical `ProviderCacheReclaimPlan`, whose `exact_approval_phrase` field names
an unavailable irreversible lifecycle. Source-contract RED
`7db6dc77f0e57c60096cb7d20771f2eb39d0cd3c` requires a facade-owned plan with Trash approval only.
Production repair `86d1eeb3e35616c0b95d15b67dfb3ceb26b2574d` introduces that DTO and explicit
projection; `defac8ff38c84fa2d08efd1b9f9abdcf897a7799` routes the shipped Tauri plan/result through
the facade-owned schemas. Contract update `e125f256f6f26972fa9a3fbb1cf9dd181f01736e` verifies that
the Tauri boundary no longer serializes `exact_approval_phrase` as a nullable field.

The remaining client mismatch was made executable in test-only commit
`44107488869850df6b5d67810182618216ad961a`: the Vitest wrapper contract requires the provider-cache
execution payload to omit the obsolete `mode` key. At that head the production wrapper still sent
`mode: "trash"`, so the test and implementation intentionally disagreed; no hosted failing result is
claimed unless that exact intermediate head is observed failing. Production commit
`112988abfc9bbcd8ccc7e5945cbdea636146392f` removes both the stale TypeScript
`exact_approval_phrase` declaration and the redundant invoke `mode` payload, aligning the client with
the already Trash-only Rust/Tauri schema.

An internal-authority review then found that crate privacy alone still left the historical
`execute(..., PermanentPurge, ...)` branch capable of reaching pathname-staged permanent deletion if
a future in-crate caller bypassed the safe facade. Source-contract RED
`80499b7a70ce4c1e86125fc308da7a21b6d1b9cd` requires the irreversible-mode rejection to occur before
receipt creation or the historical mutation helper. Production repair
`717926e2a7744e3c45fadde6384aa1ac4f5e4698` enforces
`provider-cache-identity-bound-permanent-delete-unavailable` at the lower-level execution boundary and
updates the lower-level acceptance so even an internally well-formed permanent-purge request leaves
the candidate intact and creates no receipt.

Fresh review of that repair found a stronger residual capability: despite the early return,
production `execute` still contained a `PermanentPurge` match arm statically calling
`permanently_purge_exact`, and the pathname-staged deletion helpers still compiled in normal builds.
Source-contract RED `b3fe5adf08685a35c3bfd87fa0539a0599f83e32` requires the helper/rollback hooks to
be `#[cfg(test)]`, requires no production call edge to `permanently_purge_exact`, and requires the
post-guard production execution path to contain no irreversible match arm. Its hosted failure is not
claimed unless that exact intermediate head is observed failing. Production repair
`d1b1df14ecbbe50573716801dfd93e7356f2665d` gates the historical destructive implementation and
its audit helper to tests, removes the permanent branch and permanent-only directory condition from
production `execute`, fixes receipt/result mode to Trash after the fail-closed guard, and leaves the
real replacement-race fixtures available under the unit-test configuration.

The next review moved from deletion to its audit artifact. Provider-cache receipt finalization used to
drop the opened receipt and call `fs::remove_file(&path)` when sealing or directory sync failed. That
cleanup could delete an unrelated replacement record if the visible name changed after create-new.
Source-contract RED `511f373d4282c88410663a924196d074c9f81be8` forbids pathname unlink in this
post-create failure path. Production repair `727746b08b6320d44a813dec2b183a9382809130`
truncates and syncs the already-open receipt handle instead. Its Unix real-filesystem test removes the
original visible name, creates replacement bytes at the same pathname from the finalization hook, and
verifies that the replacement remains byte-identical after the failure. The preceding ancestry commit
`ff0468dbfa0bbe87d6f0fbd128d85e243ffa0932` adopts #344 exact
`2a23a1d7de5b929a76b432c192d2b9e537fbbbdd` as a second parent and semantically retains both owner
deltas. The source-contract RED's hosted failure is not claimed unless that exact intermediate head is
observed failing.

The RED commits above are contract evidence; hosted status is reported only from the exact head on
which it actually ran. Exact-current hosted tests, review, ancestry, and protected integration remain
required before this ADR may become Accepted.

## Consequences

DiskSage can surface exact regenerable provider caches without TTLs, arbitrary confidence weights, or
provider OAuth. It may intentionally return no candidates when evidence is incomplete. Physical
reclaim is established only by before/after filesystem measurement; nominal cache size and guest TRIM
output are not substituted for that observation.

Operators may save selected candidate triplets as an absolute JSON manifest and invoke
`disksage-provider-cache-reclaim execute` with both fingerprint fields, the Trash approval phrase, a
rationale, and `--trash`. Unknown or duplicate flags fail closed. Irreversible approval is not part of
the shipped CLI, Rust, Tauri, or TypeScript plan/execution contract while the required deletion
authority is unavailable.

A failed provider-cache receipt finalization may leave a zero-length private create-new tombstone
rather than unlinking a pathname that another same-user actor could have replaced. This is an
intentional safety trade: preserving an unrelated replacement object is more important than silently
removing an incomplete receipt name. Successful receipts remain create-new and read-only.

The historical `PermanentPurge` variant and lower-level planning/result evidence remain crate-private,
but pathname-staged purge/rollback helpers are test-only and production `execute` has no call edge to
them. The lower-level executor rejects the unavailable mode before re-planning or receipt creation;
all code after that guard is structurally Trash-only. Historical filesystem fixtures remain useful for
showing why pathname staging is insufficient, but they are not production capability. They must not be
re-enabled unless replaced by the canonical identity-bound deletion/recovery primitive and its
cross-platform destructive acceptance evidence. External Rust callers can name only the facade-owned
Trash plan/mode/result contracts; internal irreversible plan/mode/result types are not part of the
commercial API.

Client-schema parity and internal fail-closed parity remove contract/capability leakage but do not make
permanent deletion safe or complete. Until same-object deletion/recovery, cross-platform destructive
acceptance, exact-head checks, review, and protected integration are complete, this ADR remains
Proposed.

## Rejected alternatives

- Reusing ADR-0012: rejected because an earlier active owner already holds that identity.
- Age-, name-, or process-absence-only deletion authority: rejected because it does not establish
  regeneration or current object identity.
- Treating private object-bound publication as deletion authority: rejected because publication and
  irreversible removal have different rollback and crash-consistency invariants.
- Keeping the historical executor public while UI/CLI merely reject permanent mode: rejected because
  a direct Rust caller would still retain irreversible pathname-authorized capability.
- Treating crate privacy alone as sufficient isolation for the historical permanent-purge executor:
  rejected because a future in-crate caller could accidentally route to irreversible pathname-based
  mutation without crossing the commercial facade.
- Leaving the pathname purge helper compiled and statically referenced behind an earlier runtime
  rejection: rejected because an unreachable branch is weaker than removing the production call edge,
  and future control-flow edits could silently reconnect irreversible mutation.
- Removing a failed create-new receipt by pathname: rejected because the visible name can refer to a
  different object by cleanup time; failure invalidation acts only on the exact open handle.
- Re-exporting the historical cleanup mode/result while hiding only the executor: rejected because it
  advertises an unavailable irreversible lifecycle in the external Rust contract and leaves the
  commercial result coupled to an internal mode type.
- Re-exporting the historical plan while clearing `exact_approval_phrase` at runtime: rejected because
  the public type still names and serializes unsupported irreversible authority.
- Keeping a TypeScript-only `mode: "trash"` field after the Tauri command removed mode selection:
  rejected because duplicated authority drifts from the canonical command schema and can falsely
  imply that the client selects mutation mode.
- Shipping pathname-authorized permanent file deletion while UI/CLI merely warn: rejected because a
  warning does not repair the mutation authority.
- Permanent recursive purge of provider directories: rejected because partial recursive deletion
  cannot be honestly rolled back.
- Blanket Podman prune/repair: rejected because ambiguous volumes, images, and damaged references can
  contain or protect user data.
