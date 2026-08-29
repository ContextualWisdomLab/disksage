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
postcheck. Exact orphan-container removal may use `container rm --force` only after a fresh audit
proves every selected container is non-running; it never adds volume removal.

## Consequences

- Native dependency rules preserve damaged objects still required by containers.
- Partial recovery is visible rather than mislabeled as a no-op.
- Remaining dependent corruption requires a new, narrower evidence contract; DiskSage does not
  mutate internal storage metadata.

## Rejected alternatives

- Directly editing the graph root cannot preserve Podman's dependency invariants.
- `system check --repair --force` may remove dependent containers and images, including live state.

## Reference

Podman. (2026). *podman-system-check: Perform consistency checks on image and container storage*.
https://docs.podman.io/en/latest/markdown/podman-system-check.1.html
