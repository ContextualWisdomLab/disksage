# ADR-0047: Treat OneDrive provider cache as diagnostic evidence, never a deletion target

- Status: Accepted
- Date: 2026-08-29

## Decision

DiskSage may read the allocated size, file count, active-use evidence, and File Provider global
state of OneDrive-managed cache storage. Reports retain no item names or user paths. Cache bytes are
not classified as orphaned or reclaimable, and DiskSage never deletes or resets provider internals.

`provider-sync-stalled` requires two complete observations with the same path-free aggregate
fingerprint, a pending global state in both observations, and an explicit caller-supplied service
deadline. A cache size alone is not a stall. Provider-reported local-disk-full plus nonzero provider
cache allocation is `internal-pressure`. Missing scan or active-use evidence is `unavailable`.

The diagnostic cannot authorize a provider restart. Item-local space recovery continues through
the operating system's File Provider eviction API after exact upload, identity, conflict, pin, and
open-file checks. A OneDrive reset remains an operator recovery because Microsoft documents that it
disconnects sync connections and rebuilds client state.

## Evidence and consequences

The 2026-08-29 live read-only observation found 13,663,981,568 allocated bytes across 13 provider
cache files, complete active-use evidence with zero users of that cache root, and a File Provider
global error containing local-disk-full. DiskSage classified this as `internal-pressure`; mutation
and restart authorization remained false.

Customer guidance describes the next action without exposing DBFS, SQLite, container identifiers,
or provider implementation boundaries.

## References

Apple Inc. (n.d.). *NSFileProviderManager: evictItem(identifier:completionHandler:)*. Apple
Developer Documentation. https://developer.apple.com/documentation/fileprovider/nsfileprovidermanager/evictitem(identifier:completionhandler:)

Microsoft. (2026). *Reset OneDrive*. Microsoft Support.
https://support.microsoft.com/en-us/office/reset-onedrive-34701e00-bf7b-42db-b960-84905399050c
