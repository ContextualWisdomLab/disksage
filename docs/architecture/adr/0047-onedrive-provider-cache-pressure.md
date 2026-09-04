# ADR-0047: Treat OneDrive provider cache as diagnostic evidence, never a deletion target

- Status: Accepted
- Date: 2026-08-29

## Decision

DiskSage may read the allocated size, file count, active-use evidence, and File Provider global
state of OneDrive-managed cache and transient work storage. Reports retain no item names or user paths. Cache bytes are
not classified as orphaned or reclaimable, and DiskSage never deletes or resets provider internals.

`provider-sync-stalled` requires two complete observations with the same path-free aggregate
fingerprint, either a pending global state in both observations or the same provider-reported
local-disk-full error in both, and an explicit caller-supplied service deadline. A cache size alone
is not a stall. When sync is not stalled, provider-reported local-disk-full plus a nonzero combined
provider-cache and temporary-work allocation is `internal-pressure`. Missing scan, active-use, or
global-activity evidence is `unavailable`.

After two complete unchanged observations, provider-reported local disk full, pending/error sync,
and no observed provider activity, DiskSage may offer the supported customer action: gracefully
quit the fixed `/Applications/OneDrive.app`, reopen it, and rescan. Execution requires a fresh exact
approval and rationale, rechecks the unchanged aggregate and fixed executable identity, uses only
bounded literal platform commands, and writes an immutable outcome including partial failure.
DiskSage never runs a reset, sends a force-quit signal, or deletes provider storage. Item-local space recovery continues through
the operating system's File Provider eviction API after exact upload, identity, conflict, pin, and
open-file checks. A OneDrive reset remains an operator recovery because Microsoft documents that it
disconnects sync connections, requires setup again, and performs a full sync. It is therefore not a
disk-reclaim primitive and remains a manual support recovery outside DiskSage.

## Evidence and consequences

The 2026-08-29 live read-only observation found 13,663,981,568 allocated bytes across 13 provider
cache files. Two later read-only observations found 18,095,132,672 allocated bytes across 25 files
in OneDrive's transient work area; OneDrive reported local disk full and no open handle was
observed during either bounded probe. The unchanged complete observations reached the supported
quit/reopen/rescan recommendation. These are pressure evidence, not orphan or reclaim evidence.
The original observation had complete active-use evidence with zero users of the cache root and a File Provider
global error containing local-disk-full. DiskSage classified this as `internal-pressure`; mutation
and restart authorization remained false.

Customer guidance describes the next action without exposing DBFS, SQLite, container identifiers,
or provider implementation boundaries.

The recovery uses no OAuth and grants no cloud-write, local-copy eviction, or provider-internal
deletion authority. A changed aggregate, active handle, unavailable observation, changed app
identity, stale approval, quit timeout, failed launch, or missing receipt destination fails closed.

## References

Apple Inc. (n.d.). *NSFileProviderManager: evictItem(identifier:completionHandler:)*. Apple
Developer Documentation. https://developer.apple.com/documentation/fileprovider/nsfileprovidermanager/evictitem(identifier:completionhandler:)

Apple Inc. (n.d.). *Synchronizing the File Provider extension*. Apple Developer Documentation.
https://developer.apple.com/documentation/fileprovider/synchronizing-the-file-provider-extension

Microsoft. (n.d.). *Reset OneDrive*. Microsoft Support.
https://support.microsoft.com/en-us/onedrive/reset-onedrive
