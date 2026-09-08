# ADR-0022: Keep inconclusive provider evidence and reclaim attribution separate

## Status

Proposed.

## Context

A native cloud-provider probe can time out before producing any output. That absence is not a
successful sync observation and cannot authorize local-copy eviction. Separately, APFS available
space can change because of concurrent applications, snapshots, caches, and provider activity;
the global delta is not the sum of DiskSage actions.

Customer guidance also has a different responsibility from audit evidence. The primary view must
say what to do next, while stable reason codes and evidence kinds remain available on demand.

Earlier active owner lineages already allocate ADR-0012 through ADR-0021. This decision therefore
uses ADR-0022 rather than overwriting an existing architecture identity. It remains Proposed until
its implementation and prerequisite lineages reach protected authority with exact-current gates.

## Decision

- A native probe timeout or empty successful output returns a bounded, path-free `inconclusive`
  receipt with `keep_local=true` and the next action `keep-local-and-rescan`.
- An inconclusive receipt is evidence-incomplete and cannot satisfy copy or eviction admission.
- Reclaim progress exposes the shared-volume available-space change separately from the sum of
  allocation bytes in unique, completed action receipts. Incomplete receipts cannot claim bytes.
- Customer-facing status leads with the next action. Evidence kinds, reason codes, and other
  implementation diagnostics appear only under an explicitly opened audit-detail disclosure.

## Consequences

An empty native response remains useful and durable enough for diagnosis without becoming a
mutation permit. Progress cannot be overstated by assigning concurrent APFS movement to DiskSage,
and customers can act without interpreting internal evidence terminology. The decision must not
be marked Accepted solely because an active branch is locally green.

## Rejected alternatives

- Reusing ADR-0012 was rejected because that immutable identity belongs to an earlier active owner.
- Treating empty output as a clear queue was rejected because absence of evidence cannot prove
  remote completion.
- Assigning the positive APFS delta to the preceding action was rejected because the shared volume
  admits concurrent writers and copy-on-write or snapshot effects.
- Showing reason codes in the primary status was rejected because they do not tell a customer what
  safe action to take.

## Evidence

The decision extends the repository's APFS reclaim evidence design and its existing fail-closed
File Provider admission policy. The observed native probe timeout with zero output and the
concurrently fluctuating APFS sample are operational evidence only; neither is mutation authority.
