# ADR-0004: Treat the local GGUF as executable supply-chain input

## Status

Proposed. The implementation is split across active PR #141 (installation integrity) and stacked PR #142 (load-time integrity); neither is protected-main authority until merged and revalidated.

## Context

DiskSage's optional llama.cpp advisor loads a GGUF artifact that can influence product recommendations and is parsed by native code. A successful transport, trusted repository name, pre-existing local file, or prior successful installation does not prove that the bytes loaded now are the reviewed bytes. Large model artifacts also create memory, disk, truncation, overwrite, and local namespace-race risks.

## Decision drivers

- Model files are executable-adjacent supply-chain inputs, not passive user documents.
- Mutable upstream references are insufficient identities.
- A ~1 GiB artifact must not require whole-file buffering for verification.
- Local staging/destination names can be raced.
- An artifact can change after installation and before load.
- Errors must not leak local paths or upstream response bodies.

## Alternatives considered

### Trust HTTPS and upstream repository identity

Rejected. Transport authenticity does not bind one immutable reviewed model file.

### Verify only after download

Insufficient. A valid installed file can be replaced or tampered with later.

### Immutable revision + exact byte count + digest at install and load

Selected.

## Decision

The reviewed default model identity is represented by an immutable upstream revision, exact expected byte count, and SHA-256 digest.

Installation shall:

- validate the trusted specification before network access;
- stream with a fixed bounded buffer and explicit size limit;
- reject declared or observed size drift;
- recompute SHA-256 over bytes actually staged;
- use create-new staging;
- flush/synchronize verified bytes;
- publish without clobbering an existing destination;
- make race cleanup identity-bound so foreign replacements are preserved;
- return stable privacy-safe error codes.

Load shall re-check non-following metadata, regular-file identity, exact byte count, readable bytes, and SHA-256 immediately before llama backend/model initialization. A pre-existing file is accepted only if it matches the same reviewed specification.

## Consequences

### Positive

- Mutable upstream branches and local post-install tampering no longer silently authorize model load.
- Verification has bounded memory use.
- Existing valid installations remain compatible.
- Privacy-safe diagnostics can cross product boundaries without paths/model bytes.

### Negative

- Startup/load requires hashing the model artifact.
- An upstream artifact change requires a reviewed specification update and fresh artifact.
- Digest verification does not establish model quality, safety, provenance, or license suitability.

## Failure and recovery

A missing, linked, non-regular, truncated, oversized, unreadable, or digest-mismatched artifact fails closed. Recovery is replacement through the reviewed bounded installation path or a separately reviewed specification migration. Availability pressure is not a reason to skip verification.

## Security and governance impact

The model file is treated as untrusted until exact identity verification. Installation and load errors are stable reason categories and exclude paths, response bodies, and model content. Upstream license/provenance evidence remains part of release/acquisition diligence.

## Verification and acceptance

Required tests include known digest vectors, immutable-revision metadata, invalid trusted specification, exact/short/long/wrong-digest streams, staging/destination collision races, symlink/non-regular inputs, reader/open failures, cleanup ownership, loopback HTTP behavior, and source binding proving verification occurs before llama initialization. Exact-head coverage/security/review gates still apply.

## Migration and rollback

A replacement model updates immutable revision, exact bytes, and digest together after independent validation and regression tests. Rollback may select another reviewed immutable specification but may not restore mutable `/main/` URLs, unbounded buffering, clobbering publication, or a bypass for pre-existing files.

## Supersession conditions

A content-addressed artifact or signed provenance system may supplement this design. Supersession must still bind exact bytes at the point of installation and load and preserve race, privacy, and rollback properties.