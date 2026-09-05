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
supported platforms. That lower-level path is retained for repair evidence; it is not shipped product
authority.

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

The shipped Tauri and headless CLI surfaces expose **Trash only** for provider-cache mutation. The
product plan clears the historical `exact_approval_phrase` for irreversible deletion while leaving
read-only evidence, evidence issues, and the Trash approval unchanged. A request for
`PermanentPurge` fails before the lower-level executor with the stable error
`provider-cache-identity-bound-permanent-delete-unavailable`. The CLI likewise advertises only
`--trash`, rejects `--permanent-purge` before manifest/executor work, and does not emit an
irreversible approval phrase from `plan`.

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

These observations are operational evidence, not release or irreversible-deletion authority. The
source-contract RED introduced on this PR requires shipped Tauri/CLI execution to fail permanent
purge closed before reaching the historical lower-level executor. Exact-current hosted tests,
review, ancestry, and protected integration remain required before this ADR may become Accepted.

## Consequences

DiskSage can surface exact regenerable provider caches without TTLs, arbitrary confidence weights, or
provider OAuth. It may intentionally return no candidates when evidence is incomplete. Physical
reclaim is established only by before/after filesystem measurement; nominal cache size and guest TRIM
output are not substituted for that observation.

Operators may save selected candidate triplets as an absolute JSON manifest and invoke
`disksage-provider-cache-reclaim execute` with both fingerprint fields, the Trash approval phrase, a
rationale, and `--trash`. Unknown or duplicate flags fail closed. Irreversible approval is not part of
the shipped CLI contract while the required deletion authority is unavailable.

The historical lower-level `PermanentPurge` enum/helper remains technical debt until it is removed or
moved behind a canonical identity-bound deletion primitive. Typed frontend contracts must not present
that historical mode as a commercially available action; this remains a follow-up before #303 can be
considered product-complete.

## Rejected alternatives

- Reusing ADR-0012: rejected because an earlier active owner already holds that identity.
- Age-, name-, or process-absence-only deletion authority: rejected because it does not establish
  regeneration or current object identity.
- Treating private object-bound publication as deletion authority: rejected because publication and
  irreversible removal have different rollback and crash-consistency invariants.
- Shipping pathname-authorized permanent file deletion while UI/CLI merely warn: rejected because a
  warning does not repair the mutation authority.
- Permanent recursive purge of provider directories: rejected because partial recursive deletion
  cannot be honestly rolled back.
- Blanket Podman prune/repair: rejected because ambiguous volumes, images, and damaged references can
  contain or protect user data.
