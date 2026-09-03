# ADR-0023: Evidence-bound provider cache reclaim

- Status: Proposed
- Date: 2026-08-29

## Context

macOS may retain large, regenerable provider artifacts even when the active application or virtual
machine uses a different object. Moving such a cache to Trash is reversible but does not immediately
increase free space. Permanent deletion does, but must not be inferred from age, name alone, or an
LLM recommendation. A damaged Podman overlay store can also make `podman system df` fail; broad
prune or repair commands are unsafe because unused-looking volumes may contain databases and native
repair can itself fail while a damaged layer remains referenced by a stopped container.

Earlier active DiskSage owner lineages already allocate ADR-0012 through ADR-0022. This decision
therefore uses ADR-0023 rather than reusing an immutable architecture identity. It remains Proposed
until its container/runtime prerequisites and this implementation reach protected authority with
exact-current verification.

## Decision

DiskSage admits only these independently identified cache classes:

1. A Podman AppleHV content-addressed `*.raw.zst` machine seed whose 64-hex key and full content
   digest are recorded, whose machine and active raw disk both exist, whose recreation source is
   observed from Podman, and for which no open handle is found. The active raw VM is never a candidate.
2. A Microsoft EdgeUpdater cached installed copy whose bundle version exactly equals its directory
   name and differs from the exact installed `/Applications` version. The installed version is retained.
3. EdgeUpdater `crx_cache` as a separate, explicitly selectable regenerable candidate.

Planning is read-only and the default CLI operation. Execution re-plans and rechecks object identity,
content evidence, and open handles. OS Trash and permanent purge are distinct modes. Permanent purge
is limited to the evidence-backed file class for which recovery and integrity boundaries are proven;
provider directory candidates remain Trash-only. Permanent purge requires the same fresh plan
fingerprint in two confirmation fields, the backend-authored exact phrase, an explicit candidate
manifest and rationale, and a create-only private receipt. It stages the exact object by an atomic
same-parent rename before deletion and appends the safety journal.

When `podman system df` fails, the plan is `repair-required` and offers only the read-only
`podman system check --quick` diagnostic. DiskSage does not claim `--repair` succeeds and does not
automatically run repair, blanket image/volume prune, or remove a referenced container or layer.

## Evidence

A redacted production observation admitted one superseded Edge installed-copy cache, EdgeUpdater's
separately selected CRX cache, and one AppleHV seed cache. Their measured allocation was 1,797,688
KiB; exact permanent removal increased APFS availability by 1,792,412 KiB. No active raw VM or user
data path was selected. A separate Podman observation found that `system df` failed on a damaged
layer referenced by an exited container; both ordinary and forced container removal failed, and
`system check --repair` also failed because the layer remained in use. A guest TRIM reported 99.5
GiB while host APFS increased only 87,708 KiB, so guest-reported trim is not host reclaim proof.

These observations are operational evidence, not release authority. Exact current-head tests,
review, ancestry, and protected integration remain required before this record may become Accepted.

## Consequences

The feature can reclaim exact regenerable artifacts without TTLs, arbitrary weights, or provider
OAuth. It may intentionally return no candidates when any identity, recreation, version, content,
or handle evidence is incomplete. Physical reclaim remains established only by before/after
filesystem measurement; nominal cache size and guest trim output are not substituted for that
observation.

The packaged CLI first emits a plan with `disksage-provider-cache-reclaim plan`. The operator saves
only selected candidate triplets (`path`, `evidence_fingerprint`, `object_id`) as an absolute JSON
manifest, then invokes `execute` with both fingerprint flags, the plan's exact phrase, a rationale,
and exactly one of `--trash` or `--permanent-purge`. Unknown or duplicate flags fail closed.

## Rejected alternatives

- Reusing ADR-0012: rejected because an earlier active owner already holds that identity.
- Age-, name-, or process-absence-only deletion authority: rejected because it does not establish
  regeneration or current object identity.
- Blanket Podman prune/repair: rejected because ambiguous volumes, images, and damaged references
  can contain or protect user data.
- Permanent recursive purge of provider directories: rejected because partial recursive deletion
  cannot be honestly rolled back.
