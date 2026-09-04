# ADR-0024: Keep Colima disk reclaim behind supported provider boundaries

- Status: Proposed
- Date: 2026-08-30

## Context

Colima sparse backing disks can retain host allocation after container data is removed. Colima's documented recovery path uses a running guest (`colima ssh -- sudo fstrim -a`), while direct Lima `diffdisk` mutation, raw truncation, or inferred `qemu-img` compaction would bypass the provider's supported boundary. A stopped VM can establish workload inactivity, but it does not create a supported compaction command.

Earlier active DiskSage owner lineages already allocate ADR-0012 through ADR-0023. This record therefore uses ADR-0024 instead of colliding with an immutable architecture identity. It remains Proposed until the implementation, prerequisite lineages, exact-current tests, review, and protected integration establish shipped authority.

## Decision

DiskSage provides a read-only Colima backing-disk allocation plan. Planning binds the profile, runtime state, VM/runtime type, configured capacity, logical and allocated bytes, filesystem identity, and bounded Colima-owned configuration evidence. Profile traversal, symlinked or untrusted storage, non-regular backing disks, malformed provider output, and incomplete VM-type evidence fail closed.

Execution remains unavailable while Colima has no documented stopped-VM compaction command. A reviewed plan can produce only an explicitly unavailable receipt after independently validating the reviewed fingerprint, exact approval phrase, attributed human identity, freshness, and live evidence. DiskSage does not stop Colima, enter the guest, invoke `fstrim`, call `qemu-img`, truncate/delete the backing disk, or infer an undocumented Lima operation.

If Colima later publishes a supported native compact operation, a new decision must bind the exact provider version, command, profile, executable identity, backing-disk identity, preconditions, failure semantics, post-operation filesystem evidence, and rollback/recovery boundary before mutation is enabled.

## Consequences

Customers can inspect why host allocation remains high without exposing a destructive action whose safety cannot be defended. The feature may return unavailable even when the VM is stopped and the backing file is large. That is preferable to presenting an unsupported maintenance operation as a reclaim capability.

This branch must not be treated as an independent competing Colima authority if a later canonical Colima reclamation owner incorporates the same contract. In that case, retain this PR until every unique test, fixture, provider-boundary rule, and evidence contract is verified as inherited or deliberately rejected.

## Rejected alternatives

- Reusing ADR-0012: rejected because an earlier active owner already holds that identity.
- Automatically starting/stopping Colima: rejected because it mutates runtime state without operator intent.
- Guest `fstrim` as a stopped-VM operation: rejected because it requires a running guest and does not itself prove host APFS reclaim.
- Direct `qemu-img`, raw truncation/deletion, or Lima implementation-detail mutation: rejected because these are not supported Colima product boundaries and can corrupt the VM.
- Reporting nominal sparse-file size reduction as reclaimed host space: rejected; physical reclaim requires before/after filesystem evidence.

## Evidence

The current implementation's regression suite covers stable reviewed-plan handoff, current Colima list/config evidence, custom Lima storage, symlink rejection, attributed approvals, and self-validation of unavailable receipts. Historical review findings remain evidence only and must be revalidated against the exact current head.

Primary provider references are the Colima FAQ and current Colima command/config sources; Lima implementation discussions are supporting evidence, not permission to invoke undocumented maintenance commands.
