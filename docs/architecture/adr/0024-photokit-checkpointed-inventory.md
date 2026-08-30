# ADR-0024: Checkpoint PhotoKit inventory at native completion boundaries

- Status: Accepted
- Date: 2026-08-30

## Context

Reading locally available originals can take materially different time per asset. A single large
PhotoKit request kept the customer waiting without progress or cancellation, while an arbitrary
wall-clock timeout discarded completed evidence.

## Decision

DiskSage requests one `PHAsset` per native page. A page is accepted only after PhotoKit's resource
completion handler, and records its measured duration rather than using it as a guessed cutoff.
Rust rejects gaps, repeated offsets, missing completion evidence, and inconsistent totals. The UI
checkpoints after every page, yields for rendering, and stops between pages when requested. A
checkpoint is resumable; it never authorizes deletion. Network access remains disabled and no
PhotoKit change request is made during inventory.

## Consequences

Large libraries take as long as their locally available originals require, but the customer sees
progress, may stop safely, and can resume without repeating accepted pages. Destructive planning
still requires a complete inventory and the existing fresh re-fetch and exact approval contract.

## Rejected alternatives

An arbitrary whole-library timeout was rejected because elapsed wall time is not evidence that
PhotoKit failed. Large fixed pages were rejected because they cannot yield promptly between assets.

## References

Apple. (n.d.). *PHAssetResourceManager*. Apple Developer Documentation.
https://developer.apple.com/documentation/photokit/phassetresourcemanager

Apple. (n.d.). *Fetching and caching assets and thumbnails*. Apple Developer Documentation.
https://developer.apple.com/documentation/photokit/fetching-and-caching-assets-and-thumbnails
