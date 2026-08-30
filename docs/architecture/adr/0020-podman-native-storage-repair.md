# ADR-0020: Use Podman's native non-forced storage repair

Status: Accepted

## Context

A live Podman machine reported orphan candidates but `system df` and exact container removal were
blocked by inconsistent overlay-layer metadata. Editing the graph root directly would bypass
Podman's dependency knowledge and could damage running containers or persistent volumes.

## Decision

DiskSage runs `podman system check --quick` read-only and hashes the sorted damaged-layer identities
as evidence, but does not present that set as mutation authority: Podman's native repair command
cannot atomically restrict repair to caller-supplied layer IDs. Approval is therefore bound to the
selected machine and exact `podman --connection <machine> system check --quick --repair` scope;
`--force` is prohibited. A nonzero native exit may still be a successful partial repair, so the
receipt compares pre/post identities only after a complete fresh postcheck. Once the repair process
starts, timeout, capture, wait, and decoding failures return an attempted-command receipt and run a
bounded postcheck instead of erasing possible mutation evidence. Orphan-container removal remains
non-forced even after a fresh stopped-state audit:
there is no atomic Podman state precondition between that audit and `container rm`, while
`container rm --force` is allowed to remove a container that restarted or became paused in that
window. A damaged stopped container that cannot be removed normally therefore remains preserved
for a new evidence cycle instead of turning stale state evidence into kill/remove authority.
When Podman refuses repair because a damaged layer remains referenced by a container, the receipt
records `podman-storage-repair-provider-unable-to-detach-damaged-container`. DiskSage does not
retry, add `--force`, reset storage, or edit graph-driver files; the next action is a new
container-lineage audit and an independently approved normal provider removal.

## Consequences

- Native dependency rules preserve damaged objects still required by containers.
- Approval names the real machine-wide native repair scope instead of claiming unsupported
  candidate-level atomicity.
- Partial recovery is visible rather than mislabeled as a no-op.
- Post-spawn failures remain auditable; numeric counts stay absent without a complete postcheck.
- A container that changes state after the fresh audit causes normal exact removal to fail rather
  than being force-stopped or removed.
- Remaining dependent corruption requires a new, narrower evidence contract; DiskSage does not
  mutate internal storage metadata.
- Provider exit 125 with a container-referenced layer is distinguished from transport/runtime
  failures without exposing layer or container identifiers in the public receipt.

## Rejected alternatives

- Directly editing the graph root cannot preserve Podman's dependency invariants.
- Candidate-fingerprint mutation approval is rejected because native repair cannot accept that
  candidate list as an atomic precondition.
- `system check --repair --force` may remove dependent containers and images, including live state.
- `container rm --force` after a stopped-state audit is rejected because the audit and mutation are
  not one atomic state transition and the force option can remove subsequently running/paused
  containers.

## Reference

Podman. (2026). *podman-system-check: Perform consistency checks on image and container storage*.
https://docs.podman.io/en/latest/markdown/podman-system-check.1.html

Podman. (2026). *podman-rm: Remove one or more containers*.
https://docs.podman.io/en/latest/markdown/podman-rm.1.html
