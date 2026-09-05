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
supported platforms. That lower-level implementation is retained inside the application crate for
repair evidence; it is not commercial mutation authority.

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

All public Rust and Tauri mutation surfaces expose **Trash only**. The public Rust facade owns its
`ProviderCacheReclaimPlan` instead of re-exporting the historical lower-level plan; the commercial
plan contains `trash_approval_phrase` and has no `exact_approval_phrase` field. The Tauri planning
command projects the lower-level evidence into that same facade-owned DTO, so unavailable
irreversible approval is omitted from the serialized schema rather than emitted as `null`. Tauri
execution has no caller-selected cleanup-mode argument, delegates every product call as internal
Trash, and projects the result through the facade-owned one-variant `ProviderCacheCleanupMode::Trash`
DTO.

The headless CLI imports the public `provider_cache` facade, rejects `--permanent-purge` before
manifest or executor work with `provider-cache-identity-bound-permanent-delete-unavailable`, and calls
only `execute_trash`. The historical `ProviderCacheCleanupMode`, lower-level plan/result, and
`provider_cache_reclaim` pathname-staged irreversible executor remain crate-private implementation
and repair evidence.

The TypeScript wrapper does not accept a cleanup-mode argument and its cleanup result is typed as
Trash-only, but its current plan interface still declares `exact_approval_phrase: string | null` and
its invoke payload still sends `mode: "trash"` even though the Tauri command no longer accepts a mode.
Those stale client-schema remnants are a code-current follow-up and must be removed before this ADR
can become Accepted; they do not restore irreversible backend authority.

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

The RED commits above are source-contract evidence only; no hosted failing run for their intermediate
heads is claimed. Exact-current hosted tests, review, ancestry, and protected integration remain
required before this ADR may become Accepted.

## Consequences

DiskSage can surface exact regenerable provider caches without TTLs, arbitrary confidence weights, or
provider OAuth. It may intentionally return no candidates when evidence is incomplete. Physical
reclaim is established only by before/after filesystem measurement; nominal cache size and guest TRIM
output are not substituted for that observation.

Operators may save selected candidate triplets as an absolute JSON manifest and invoke
`disksage-provider-cache-reclaim execute` with both fingerprint fields, the Trash approval phrase, a
rationale, and `--trash`. Unknown or duplicate flags fail closed. Irreversible approval is not part of
the shipped CLI, Rust, or Tauri plan/execution contract while the required deletion authority is
unavailable.

The historical `PermanentPurge` variant and pathname-staged purge helpers remain crate-private repair
evidence with unit coverage. They no longer form an external Rust capability. They must not become
public again unless they are replaced by the canonical identity-bound deletion/recovery primitive and
its cross-platform destructive acceptance evidence. External Rust callers can name only the
facade-owned Trash plan/mode/result contracts; internal irreversible plan/mode/result types are not
part of the commercial API.

The TypeScript provider-cache plan and invoke payload still need a schema-cleanup follow-up so the
client contract exactly matches the stronger Tauri/Rust boundary. Until that lands and exact-head
checks are terminal green, this ADR remains Proposed.

## Rejected alternatives

- Reusing ADR-0012: rejected because an earlier active owner already holds that identity.
- Age-, name-, or process-absence-only deletion authority: rejected because it does not establish
  regeneration or current object identity.
- Treating private object-bound publication as deletion authority: rejected because publication and
  irreversible removal have different rollback and crash-consistency invariants.
- Keeping the historical executor public while UI/CLI merely reject permanent mode: rejected because
  a direct Rust caller would still retain irreversible pathname-authorized capability.
- Re-exporting the historical cleanup mode/result while hiding only the executor: rejected because it
  advertises an unavailable irreversible lifecycle in the external Rust contract and leaves the
  commercial result coupled to an internal mode type.
- Re-exporting the historical plan while clearing `exact_approval_phrase` at runtime: rejected because
  the public type still names and serializes unsupported irreversible authority.
- Shipping pathname-authorized permanent file deletion while UI/CLI merely warn: rejected because a
  warning does not repair the mutation authority.
- Permanent recursive purge of provider directories: rejected because partial recursive deletion
  cannot be honestly rolled back.
- Blanket Podman prune/repair: rejected because ambiguous volumes, images, and damaged references can
  contain or protect user data.
