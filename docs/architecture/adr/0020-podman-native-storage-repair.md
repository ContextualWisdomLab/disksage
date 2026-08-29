# ADR-0020: Use Podman's native non-forced storage repair

Status: Accepted

## Context

A live Podman machine reported orphan candidates but `system df` and exact container removal were
blocked by inconsistent overlay-layer metadata. Editing the graph root directly would bypass
Podman's dependency knowledge and could damage running containers or persistent volumes.

## Decision

DiskSage runs `podman system check --quick` read-only, hashes the sorted damaged-layer identities,
and exposes an exact approval phrase. Execution rechecks that fingerprint and invokes only
`podman system check --quick --repair`; `--force` is prohibited. A nonzero native exit may still be
a successful partial repair, so the receipt derives repaired and remaining counts from a fresh
postcheck. Orphan-container removal remains non-forced even after a fresh stopped-state audit:
there is no atomic Podman state precondition between that audit and `container rm`, while
`container rm --force` is allowed to remove a container that restarted or became paused in that
window. A damaged stopped container that cannot be removed normally therefore remains preserved
for a new evidence cycle instead of turning stale state evidence into kill/remove authority.

## Consequences

- Native dependency rules preserve damaged objects still required by containers.
- Partial recovery is visible rather than mislabeled as a no-op.
- A container that changes state after the fresh audit causes normal exact removal to fail rather
  than being force-stopped or removed.
- Remaining dependent corruption requires a new, narrower evidence contract; DiskSage does not
  mutate internal storage metadata.

## Rejected alternatives

- Directly editing the graph root cannot preserve Podman's dependency invariants.
- `system check --repair --force` may remove dependent containers and images, including live state.
- `container rm --force` after a stopped-state audit is rejected because the audit and mutation are
  not one atomic state transition and the force option can remove subsequently running/paused
  containers.

## Reference

Podman. (2026). *podman-system-check: Perform consistency checks on image and container storage*.
https://docs.podman.io/en/latest/markdown/podman-system-check.1.html

Podman. (2026). *podman-rm: Remove one or more containers*.
https://docs.podman.io/en/latest/markdown/podman-rm.1.html
