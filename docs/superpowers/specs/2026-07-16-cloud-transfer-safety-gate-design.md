# Cloud transfer safety gate

## Scope

This slice turns an approved cloud-archive candidate into a verified **copy**. It does not evict,
trash, move, hydrate, or delete the source. A later source-eviction command must require the permit
defined here and must still use DiskSage's trash-only deletion path.

No model is involved. Eligibility, hashing, path containment, receipt integrity, and sync evidence
matching are deterministic Rust operations.

## Required sequence

1. Accept only a planner candidate with an embedded, high-confidence production date, no review
   requirement, no planner blocker, and a non-empty metadata fingerprint.
2. Revalidate absolute source/destination paths and require the destination to be under the selected
   provider root.
3. Re-stat the source immediately before copying; size and modification time must still match the
   dry-run candidate.
4. Create the destination with `create_new` so an existing or concurrently created file is never
   overwritten.
5. Stream-copy while calculating BLAKE3, SHA-256, and Microsoft QuickXorHash, `sync_all` the
   destination, then re-hash both source and destination. Source metadata, byte counts, and all
   hashes must still match.
6. Persist a create-only, read-only receipt whose identifier binds the planner fingerprint,
   provider, paths, byte count, all three content digests, source modification time, copy time, and
   state flags.
7. Keep the source. A verified copy remains `awaiting-provider-sync`.
8. Accept only provider API or provider-native sync evidence whose receipt ID, provider,
   destination, byte count, destination BLAKE3, and timestamp match. Only then create a
   local-eviction permit.

The headless CLI regenerates a fresh metadata plan before a fingerprint-selected copy. It offers a
separate iCloud attestation action, but neither action removes the source.

## Fail-closed boundaries

- A filename date or filesystem timestamp cannot enter the copy phase by itself.
- An embedded date that is a known document-template default, contradicts later evidence, or is
  accompanied by sensitive metadata remains review-required and cannot enter the copy phase.
- A symlinked destination parent that resolves outside the cloud root is rejected.
- A copy failure removes only a destination that this invocation successfully created.
- A pre-existing receipt is never overwritten or removed.
- Provider sync evidence is a separate immutable record. Manual assumption that a local cloud
  folder "will eventually sync" is not evidence.
- Before a provider adapter reads a destination path from a receipt, the CLI requires an absolute,
  regular, read-only receipt file named for its receipt ID and validates receipt version, integrity,
  copy state, and safe absolute source/destination paths.
- The iCloud adapter checks Foundation's ubiquitous-item, uploaded, and local-current status before
  reading content, so an evicted placeholder is not hydrated merely to prove its hash. It then
  revalidates file identity, size, modification time, and BLAKE3 around the status observation.
- This slice deliberately exposes no eviction function, so a permit cannot accidentally delete a
  source until the trash-only follow-up is separately implemented and reviewed.

## Provider adapter status

The pure evidence gate is provider-neutral. macOS iCloud now has a Foundation-backed, per-file
native adapter. OneDrive and Google Drive response parsers and evidence builders bind Graph
QuickXorHash and Drive v3 SHA-256 respectively to the local receipt. Their authenticated read-only
client accepts an ephemeral caller-supplied OAuth access token and provider-native object ID, calls
only fixed Microsoft or Google HTTPS hosts with redirects disabled and bounded response bodies, and
re-hashes the local destination before and after the API request. OAuth consent and secure token
acquisition remain a UI/platform integration concern; DiskSage does not persist the token. Until the
selected provider's adapter returns complete evidence, the source remains local.
