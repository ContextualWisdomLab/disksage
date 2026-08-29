# ADR 0019: macOS File Provider local eviction

- Status: Accepted
- Date: 2026-08-29

## Context

DiskSage can prove that selected iCloud and OneDrive items are uploaded, current, idle, and locally
allocated. OneDrive's supported macOS **Free up space** action was not executable through DiskSage.

## Decision

DiskSage may release a locally materialized iCloud or OneDrive file only after the existing exact
path, allocation, uploaded/current, conflict, provider capability, item identity, and active-use
checks pass and a human approves the exact plan fingerprint. iCloud continues
to use Foundation's ubiquitous-item eviction. OneDrive native eviction remains blocked. A
read-only integration probe against a real OneDrive File Provider item returned
`NSFileProviderErrorProviderNotFound` (`-2001`) before identity resolution, so DiskSage cannot
claim that a cross-provider `NSFileProviderManager` can operate on OneDrive. The compiled helper
targets macOS 11.0, binds item and domain identity together, and rejects identity replacement
before its eviction call, but production Rust refuses to invoke that call until a reviewed
integration receipt proves path retention and reduced local allocation. A provider-wide new-copy
admission check is intentionally
not reused here because it governs adding copies, while this operation releases an already
uploaded local copy. Exact
item evidence still fails closed. The result must retain the path and show a reduced
allocation before DiskSage reports verification complete.

Google Drive remains blocked until its provider behavior is verified against the same contract.
OAuth is not required for local cache eviction because the signed-in desktop File Provider owns
that operation. Deleting, moving, or trashing the visible cloud item is never an eviction fallback.

## Consequences

Unsynced item edits, provider mismatch, non-evictable items, open handles,
incomplete evidence, native request failure, and unchanged post-action allocation fail closed. OneDrive
uses the same bounded batch fingerprint, per-item re-plan, immutable checkpoint, and stop-on-first-
failure contract as iCloud without weakening either provider's native execution boundary.

## Rejected alternatives

- Calling the iCloud ubiquitous-item API for OneDrive: the ownership contract is wrong.
- OneDrive's undocumented `/unpin` command: observed builds can print a native failure while
  exiting successfully, so process exit status cannot prove completion.
- Cross-provider `NSFileProviderManager` eviction without runtime proof: the harmless identity
  probe failed with the SDK-defined provider-not-found error.
- Finder UI automation: it is not an identity-bound or deterministic execution boundary.
- OAuth or direct deletion: neither is necessary for local cache eviction, and deletion changes the
  cloud object.

## References

Apple Inc. (2026). *NSFileProviderManager*. Apple Developer Documentation.
https://developer.apple.com/documentation/fileprovider/nsfileprovidermanager

Microsoft. (2026). *Deploy and configure OneDrive on macOS*. Microsoft Learn.
https://learn.microsoft.com/en-us/sharepoint/files-on-demand-mac
