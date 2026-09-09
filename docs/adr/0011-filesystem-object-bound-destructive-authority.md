# ADR-0011: Filesystem-object-bound destructive authority

- **Status:** Proposed
- **Date:** 2026-08-11
- **Decision owners:** DiskSage runtime/security architecture

## Context

DiskSage discovers filesystem candidates and may later offer reversible cleanup through the desktop recycle bin. A pathname is not a durable filesystem identity. Another same-user process can rename a validated directory or replace a child path after a check and before a path-based recycle operation. Re-checking the pathname immediately before mutation narrows the window but does not remove the check/use race.

The active cache-root hardening line demonstrated this distinction: no-follow handles can safely bind read-only enumeration to the intended directory, while the existing cross-platform recycle API still accepts a pathname for the destructive operation. Treating those two boundaries as equivalent would overstate mutation authority.

## Decision

A DiskSage destructive filesystem operation MUST satisfy one of these conditions before it is enabled:

1. the mutation primitive itself is bound to the exact filesystem object identity that was authorized, with platform semantics that prevent pathname substitution through the destructive boundary; or
2. DiskSage fails closed and exposes only read-only discovery/evidence for that operation.

Path equality, canonicalization, `lstat` followed by a later path open, a pre-delete revalidation, UI confirmation, or model/reviewer evidence alone MUST NOT grant destructive authority when a pathname can still be rebound before the mutation primitive consumes it.

Read-only discovery may use no-follow directory handles or equivalent object-bound primitives. A future recycle implementation may be platform-specific, but it must preserve the product requirement that cleanup is reversible; replacing recycle-bin semantics with permanent deletion is not an acceptable security workaround.

## Alternatives considered

### Revalidate the root immediately before `trash::delete(path)`

Rejected as the final authority boundary. It reduces exposure but leaves a race between the last validation and the path-consuming recycle call.

### Hold an advisory filesystem lock

Rejected. Advisory locks do not prevent an uncooperative same-user process from renaming or substituting pathname components.

### Permanently delete by descriptor-relative unlink

Rejected. Descriptor-relative unlinking can preserve object identity on supported platforms, but it violates DiskSage's reversible-cleanup contract.

### Disable all filesystem analysis

Rejected. Read-only analysis can be made object-bound independently and remains useful without granting mutation authority.

## Consequences

- Cache discovery remains available when the root can be opened and enumerated without following replacement links.
- Cache cleanup remains disabled where DiskSage cannot prove object-bound recycle semantics through the destructive boundary.
- The UI must not invite, confirm, or claim a cache deletion that the backend intentionally refuses.
- Artifact or other cleanup paths must be assessed against the same identity rule rather than inheriting authority from this ADR by assumption.
- Product documentation and acquisition claims distinguish read-only evidence from shipped mutation capability.

## Security and privacy impact

This decision prevents an attacker-controlled pathname substitution from turning an authorized cache cleanup into mutation of an unrelated user path. It does not claim protection from a privileged actor that can bypass the operating system's own object/permission model.

No additional filesystem identifiers, raw paths, or telemetry are exported by this decision.

## Verification and acceptance

A mutation implementation is acceptable only when tests demonstrate all applicable cases:

- symlink/reparse-point roots are rejected;
- root replacement after authorization cannot redirect enumeration or mutation;
- child replacement after authorization cannot redirect mutation;
- the exact authorized object, not merely a matching pathname, is the object moved to the recycle bin;
- failure to acquire or preserve object identity is fail-closed;
- the original outside fixture remains untouched under deterministic replacement-race tests;
- recycle/restore semantics remain intact on every supported platform.

Static source contracts may supplement but never replace runtime filesystem-race regressions.

## Migration and rollback

Until an object-bound recycle primitive satisfies the acceptance criteria, the safe migration state is read-only cache discovery plus an explicit unavailable mutation boundary. Rollback from a future implementation restores that fail-closed state; it must not fall back to path-based destructive behavior.

## Supersession

A later ADR may supersede this decision only if it defines an equally strong or stronger identity-preserving destructive authority model and proves reversible behavior across the supported platform set.
