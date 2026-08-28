# ADR 0019: macOS File Provider local eviction

- Status: Accepted
- Date: 2026-08-29

## Context

DiskSage can prove that selected iCloud and OneDrive items are uploaded, current, idle, and locally
allocated. OneDrive's supported macOS **Free up space** action was not executable through DiskSage.

## Decision

DiskSage may release a locally materialized iCloud or OneDrive file only after the existing exact
path, allocation, uploaded/current, conflict, provider capability, item identity, and active-use
checks pass and a human approves the exact plan fingerprint. iCloud continues to use Foundation's
ubiquitous-item eviction. OneDrive resolves the user-visible URL to its registered File Provider
domain and item identifier, then asks that domain manager to evict the item. Raw identifiers are
never persisted. The result must retain the path and show a reduced allocation before DiskSage
reports verification complete.

Google Drive remains blocked until its provider behavior is verified against the same contract.
OAuth is not required for local cache eviction because the signed-in desktop File Provider owns
that operation. Deleting, moving, or trashing the visible cloud item is never an eviction fallback.

## Consequences

Unsynced edits, provider/domain mismatch, non-evictable items, open handles, incomplete evidence,
and unchanged post-action allocation fail closed. OneDrive gains a native single-item path without
weakening iCloud behavior. Recursive/batch OneDrive eviction remains separate work.

## Rejected alternatives

- Calling the iCloud ubiquitous-item API for OneDrive: the ownership contract is wrong.
- Finder UI automation: it is not an identity-bound or deterministic execution boundary.
- OAuth or direct deletion: neither is necessary for local cache eviction, and deletion changes the
  cloud object.

## References

Apple Inc. (2026). *evictItem(identifier:completionHandler:)*. Apple Developer Documentation.
https://developer.apple.com/documentation/fileprovider/nsfileprovidermanager/evictitem(identifier:completionhandler:)

Microsoft. (2026). *Save disk space with OneDrive Files On-Demand for Mac*. Microsoft Support.
https://support.microsoft.com/en-us/onedrive/save-disk-space-with-onedrive-files-on-demand-for-mac
