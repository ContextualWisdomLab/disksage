# DiskSage Product Requirements

**Status:** Canonical product requirements

**Audience:** product, design, engineering, security, release, and support

**Scope:** standalone local-first desktop product and its optional ecosystem connectors

This document defines what DiskSage must do and how a person can tell whether it is safe and
useful. Technical mechanisms belong in the linked architecture records and specifications. The
dated [product and technical gap baseline](product-technical-gap-baseline.md) is the authoritative
inventory of current gaps and pull requests; it does not change these requirements or authorize a
file operation.

## User problem and outcome

People under disk pressure need to understand what consumes space, recover meaningful capacity,
and keep irreplaceable data without having to interpret provider internals or trust an unexplained
model verdict. DiskSage must turn evidence into a reviewable next action and must stop when the
evidence is incomplete.

The product outcome target is **300 GB of verified, attributable local capacity recovery in an
eligible real-world workload**. This is a target, not a claim that 300 GB is available or has
been recovered on any device. Every run reports candidate logical bytes separately from the
observed filesystem free-space delta; concurrent writes, snapshots, sparse images, and provider
activity may make those values differ. A run that safely finds less than 300 GB is correct when no
more eligible evidence-backed candidates exist.

## Supported environments

- Desktop platforms: macOS, Windows, and Linux for inventory and supported local cleanup domains.
- Cloud destinations: iCloud Drive, OneDrive, and Google Drive when a local provider root is
  discovered and current provider evidence is available.
- Optional integrations may enrich catalog, ontology, or model advice, but the standalone product
  must retain deterministic local safety decisions and must not require OAuth for the personal
  desktop-client path.

Platform and provider capabilities are not assumed to be equivalent. A capability that cannot be
proved on the current platform is unavailable rather than simulated.

## Jobs to be done

1. Show what occupies the disk, including unknown and physically allocated space.
2. Identify regenerable caches, inactive development artifacts, duplicates, and cloud-offload
   candidates with the evidence needed to review each action.
3. Preserve the best-supported copy and its provenance before suggesting removal of another copy.
4. Move only approved, unchanged candidates through a reversible recovery path.
5. Explain why an action is blocked and tell the person what observable step to take next.
6. Produce receipts that distinguish proposed bytes, processed bytes, and attributable physical
   recovery.

## Functional requirements

### Inventory and classification

- Scans must be bounded, cancellable, and read-only; incomplete or unreadable regions must be
  visible instead of silently omitted.
- Logical size, physical allocation, file identity, materialization state, active use, and
  provider membership must remain distinct evidence.
- Metadata and lineage must preserve source, content identity, production-time source and
  confidence, relationships, and decision evidence. Filename dates are secondary evidence only.
- Ontology and on-device model advice may explain or classify candidates but cannot replace a
  deterministic eligibility gate or grant mutation authority.

### Reclaim domains

- Regenerable caches and temporary artifacts require domain-specific ownership evidence, active-use
  checks, a fresh identity, and a complete scan before they become candidates.
- Git worktrees and standalone clones require authoritative branch/PR ancestry evidence, clean
  tracked and untracked state, no unique commits, and no active handles. Default branches and
  incomplete private-remote evidence are preserved.
- Container and VM reclaim must distinguish stale objects from active workloads and distinguish
  logical deletion from host-filesystem compaction.
- Exact duplicates require exact content evidence. Image-semantic grouping and keeper selection
  must expose separate evidence and must not use arbitrary combined scores; a tie requires a user
  choice.

### Cloud archive and local eviction

The cloud workflow is:

`reviewed candidate → exact copy verified → provider sync pending → provider sync confirmed → eviction review → reversible local action`

- Destination root, provider, account scope, capacity, collision state, source identity, exact
  bytes and content digests, metadata, and lineage must be bound to the copy review and receipt.
- A verified local copy inside a provider root is not proof of remote upload. Provider confirmation
  must be current, item-specific, and content-bound. `local-current` with upload incomplete or
  unknown remains blocked.
- Copy and eviction are separate approvals. Execution rechecks identity, active use, provider
  evidence, collisions, and approval freshness immediately before mutation.
- A timeout, ambiguous account, unreadable root, placeholder, conflict, active provider use,
  insufficient capacity/headroom, or incomplete evidence fails closed and retains the source.

| Capability | iCloud Drive | OneDrive | Google Drive |
| --- | --- | --- | --- |
| Local provider-root discovery | Supported on macOS | Supported where the desktop client exposes a local root | Supported where the desktop client exposes a local root |
| Exact-copy verification and lineage receipt | Supported | Supported | Supported |
| Provider sync confirmation | Native item/File Provider evidence | Native File Provider and/or current read-only provider evidence | Native File Provider and/or current read-only provider evidence |
| Native removal of only the local materialization | Supported when current iCloud evidence permits it | Unavailable; no equivalent authority is inferred | Unavailable; no equivalent authority is inferred |
| Generic exact-copy source workflow | Copy, confirm, then separately review the source action | Copy, confirm, then separately review the source action | Copy, confirm, then separately review the source action |

The detailed state machine and evidence ownership are defined by
[ADR-0001](architecture/adr/0001-cloud-offload-goal-state.md). Failure and placeholder handling
are defined by [ADR-0011](architecture/adr/0011-cloud-transfer-failure-and-materialization.md).

### Review, recovery, and receipts

- Every mutation requires a fresh, exact, human-attributed approval for the displayed candidate
  identities and action. Broad or stale approval is invalid.
- User-file removal must use an OS-managed or product-managed reversible Trash/quarantine path with
  a durable journal and tested undo/restore behavior. Permanent deletion is not a product action.
- A changed, replaced, unreadable, active, or newly ambiguous candidate is skipped or blocks the
  batch without broadening authority to another item.
- Receipts must state what was observed, approved, attempted, completed, skipped, and recovered;
  they must never promote logical candidate bytes into measured physical recovery.

## Product states and next actions

Customer-facing text describes the state and a next action, not an internal module or error code.
Engineering terms elsewhere in this document define product constraints; they are not approved
interface copy. A customer message must translate them into an observable condition and a safe next
action without exposing implementation ownership.

| Product state | Meaning | Required next action shown to the person |
| --- | --- | --- |
| Scanning | Evidence is still being collected. | Keep DiskSage open or cancel safely; no cleanup is available yet. |
| Review ready | Exact candidates and consequences are available. | Review the selected items and recovery method before approving. |
| Waiting for provider | A local cloud copy is not yet proven remote-current. | Let the provider finish, then refresh the evidence; keep the local source. |
| Needs attention | Capacity, collision, activity, permission, or identity evidence is incomplete. | Resolve the named condition and scan again; no item was removed. |
| Approved, rechecking | DiskSage is validating the exact reviewed state again. | Avoid editing or using the selected items until the check finishes. |
| Recovered | The reversible action completed and a receipt exists. | Verify the freed-space evidence; use Undo/Restore if the result is unwanted. |
| No eligible candidates | No further safe action is currently supported. | Review preserved blockers or choose another scan domain; do not infer deletion safety. |

## Explicit non-goals

- Reaching 300 GB by weakening evidence, deleting an irreplaceable item, or counting unobserved
  savings.
- Permanent deletion, force-pruning repositories, terminating provider databases/processes, or
  mutating a cloud placeholder to inspect it.
- Treating age, filename, file size, model confidence, or a rule of thumb as deletion authority.
- Treating OAuth, an external LLM, or another ContextualWisdomLab service as a prerequisite for
  standalone personal use.
- Exposing internal implementation boundaries, raw provider diagnostics, secrets, account
  identifiers, or private paths in customer-facing explanations.

## Safety invariants

1. Read-only evidence cannot authorize a mutation by itself.
2. Unknown, stale, incomplete, timed-out, ambiguous, or conflicting evidence fails closed.
3. Planning, copy, provider confirmation, eviction approval, and execution are distinct states.
4. Every execution is bound to fresh content and filesystem identity and rejects replacement races.
5. Provider placeholders, symlinks, active files, and incomplete scans are preserved.
6. Cloud copy completion never means provider sync completion.
7. User-file actions remain reversible and journaled; permanent deletion is unreachable.
8. A model can recommend but cannot override deterministic safety evidence.
9. Private evidence stays local with least-privilege storage; shared exports are bounded and
   path-free.
10. Success claims are evidence-bound: logical, processed, and physical bytes are reported
    separately.

## Acceptance criteria and realistic tests

- A bounded real-volume scan remains responsive, can be cancelled, and accounts for unreadable,
  sparse, hard-linked, dataless, symlinked, and replaced objects without materializing providers.
- A cloud fixture covers exact-copy verification, metadata/lineage receipt, collision, quota,
  active use, source replacement, upload pending, sync confirmed, and stale approval for all three
  providers. Only the fully evidenced path reaches an eviction review.
- A macOS iCloud fixture proves `local-current + is_uploaded=false` remains in “Waiting for
  provider”; a confirmed item can request native local-materialization eviction without deleting
  the cloud object.
- OneDrive and Google Drive fixtures prove that absence of a native local-only eviction capability
  remains unavailable while the generic exact-copy workflow stays reversible.
- Realistic cache, Git, container/VM, temporary-file, and duplicate fixtures cover active use,
  dirty/untracked state, unique commits, provider roots, replacement races, incomplete authority,
  Trash/quarantine receipt, and undo.
- Cross-platform packaging, accessibility, customer-message, privacy, security, and exact-head
  release checks pass. UI flows remain responsive during long scans and actions.
- An end-to-end reclaim report records candidate logical bytes, action results, before/after volume
  evidence, and attribution limits. It may demonstrate the 300 GB target only on an eligible
  workload with reproducible receipts; otherwise it reports the measured result without inflation.

## Telemetry, privacy, and local-only operation

DiskSage is local-first. File content, raw paths, account identifiers, provider database rows,
command lines, and secrets do not leave the device by default. Private receipts are stored locally
with restrictive permissions and bounded retention. Shareable diagnostics use stable redacted
codes, aggregates, timestamps, and fingerprints. Optional network integrations require a stated
purpose, least privilege, explicit configuration, and a documented data boundary. Product
operation and deterministic safety do not depend on telemetry collection.

## Release and operability

- A release must be reproducibly built and tested for its declared platform capability matrix;
  unsupported capabilities are labeled unavailable.
- Version manifests, changelog, provenance, SBOM, packaging, security checks, accessibility checks,
  and release notes must agree on the exact release commit.
- Long work is cancellable, bounded, and observable. Restarts reconcile journals and preserve
  sources rather than guessing completion.
- Support material gives the next safe action and links diagnostic details separately; it does not
  expose internal implementation names as customer explanations.

## Traceability and change control

- [README](../README.md): concise product entry point and current feature surface.
- [Architecture index](../ARCHITECTURE.md): product-to-technical ownership and decision links.
- [ADR index](architecture/adr/README.md): accepted technical decisions.
- [Product and technical gap baseline](product-technical-gap-baseline.md): dated implementation,
  live incident, and open-PR inventory. Exact PR heads belong there, not in this PRD.
- [Changelog](../CHANGELOG.md): integrated user-visible changes; it is not release evidence.

Changes to product outcomes, supported capability, non-goals, or safety invariants update this PRD
and either cite an accepted ADR or introduce one. Implementation progress updates the gap baseline;
it does not silently weaken the product contract.
