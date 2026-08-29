# ADR 0019: macOS File Provider local eviction

- Status: Accepted
- Date: 2026-08-29

## Context

DiskSage can prove that selected iCloud and OneDrive items are uploaded, current, idle, and locally
allocated. OneDrive's supported macOS **Free up space** action was not executable through DiskSage.

## Decision

DiskSage may release a locally materialized iCloud or OneDrive file only after the existing exact
path, allocation, uploaded/current, conflict, provider capability, item identity, and active-use
checks pass and a human approves the exact plan fingerprint. iCloud and OneDrive both use
Foundation's public ubiquitous-item eviction after a fresh File Provider snapshot proves
downloaded/current, uploaded/not-uploading, conflict-free, included, unpaused, evictable, and not
explicitly retained. The snapshot fingerprint binds the provider item and version identifiers
without recording either raw identifier. A provider-wide new-copy
admission check is intentionally
not reused here because it governs adding copies, while this operation releases an already
uploaded local copy. Exact
item evidence still fails closed. The result must retain the path and show a reduced
allocation before DiskSage reports verification complete.

Production observations on anonymized regular files proved this boundary for OneDrive: two
initial files and two later bounded batches retained their cloud paths and uploaded state while
local allocation fell. Aggregate OneDrive local allocation fell from about 83.46 GB to 21.13 GB
and host APFS availability rose by about 61 GiB. A separate apparent 10 GB diagnostic file already
had zero allocated bytes, so it was excluded and contributed zero reclaim. These are operational
observations, not fixed expected savings.

Google Drive remains blocked until its provider behavior is verified against the same contract.
OAuth is not required for local cache eviction because the signed-in desktop File Provider owns
that operation. Deleting, moving, or trashing the visible cloud item is never an eviction fallback.

## Consequences

Unsynced item edits, provider mismatch, non-evictable items, open handles,
incomplete evidence, native request failure, and unchanged post-action allocation fail closed. OneDrive
uses the same bounded batch fingerprint, per-item re-plan, immutable checkpoint, and stop-on-first-
failure contract as iCloud without weakening either provider's native execution boundary.

## Amendment: public iCloud per-item evidence (2026-08-29)

iCloud eligibility no longer depends on the undocumented `fileproviderctl evaluate` output.
DiskSage uses Foundation URL resource values and requires, for the exact item, ubiquitous identity,
`isUploaded=true`, downloading status `current`, no active upload/download, no upload/download
error, no unresolved conflict, and no sync exclusion. `isUploaded=false` remains
`provider-sync-incomplete` and cannot produce an eviction permit. These fields, allocation,
filesystem metadata, path, and active-use evidence are bound into version 3 of the plan fingerprint.

Execution remains separately approved and uses Foundation's public
`FileManager.evictUbiquitousItem(at:)`, which removes only the local copy and leaves the iCloud item
present. A successful request is not success evidence by itself: the postcheck must retain the
ubiquitous path, confirm the item is still uploaded, observe downloading status `notDownloaded`,
and measure lower local allocation. A read-only metadata snapshot at 2026-08-29 15:53 +0900
observed 71 locally allocated items of at least 10 MiB (3,587,207,168 bytes); 61 items
(2,955,091,968 bytes) met the public per-item sync contract, while 10 reported
`NSUbiquitousFileUbiquityServerNotAvailable` and remained blocked. The eligible cohort grew during
the observation loop, so these aggregate observations are fixture evidence, not an executable plan
or approval.

The macOS release contract packages and checksums both
`disksage-cloud-local-inventory-macos-arm64` and
`disksage-icloud-local-eviction-batch-macos-arm64`. The inventory producer must run read-only from
the same exact-head artifact as the planner. Inventory output from a predecessor executable is
stale evidence and cannot be reused to approve or execute a new plan.

## Amendment: exact-head redacted release evidence (2026-08-29)

Release lineage `9c010252fccbf92256ef1d19ffae063ea060becc` (artifact ZIP SHA-256
`c6d2125684237adfa00c1ebef63b38179f7d40561c5f38e768526d0208968af8`) produced
path-free, mode-0700/0600 receipt `disksage-cloud-live-20260829-9c010252`. Its complete
iCloud inventory emitted 120 candidates totaling 20,860,424,192 allocated bytes, but the exact
plan admitted zero and remained blocked. Its complete OneDrive traversal emitted an
allocation-descending top 128; all 128 were eligible, totaling 5,272,006,656 allocated bytes,
under exact fingerprint
`ad0118c3316579e768df8de2e1942b8109c76e92381b4346b2824e146e01b80a`.
The top-128 result is not whole-root authority, and neither read-only plan performed a mutation.

## Amendment: Google Drive fail-closed evidence (2026-08-29)

A metadata-only audit detected two personal, one shared, and one organization Google Drive File
Provider root. At the 1 MiB allocation floor every root produced zero candidates and zero allocated
candidate bytes. A zero-byte-floor probe emitted only 59 metadata-sized items totaling 319,488
allocated bytes from an incomplete personal-root scan; the other roots emitted none. The generic
batch planner therefore continues to reject Google Drive: DiskSage has not established the public
Foundation eligibility, item-identity revalidation, or post-eviction contract required to inherit
the iCloud and OneDrive executor. Receipt `disksage-google-drive-live-20260829-e53df609` retains the
path-free observation under mode 0700/0600. No eligibility fingerprint or mutation was produced.
Adding an executor for this negligible, incomplete cohort would weaken the provider boundary
without a measurable customer benefit.

## Rejected alternatives

- OneDrive's undocumented `/unpin` command: observed builds can print a native failure while
  exiting successfully, so process exit status cannot prove completion.
- Cross-provider `NSFileProviderManager` eviction: the earlier identity probe failed with the
  SDK-defined provider-not-found error, while FileManager's ubiquitous-item eviction is the public
  boundary verified by the successful production observations.
- Finder UI automation or Accessibility scripting: it is not an identity-bound or deterministic
  execution boundary. Revealing the exact approved selection for an explicit customer action is
  retained because it uses public AppKit and performs no provider mutation.
- OAuth or direct deletion: neither is necessary for local cache eviction, and deletion changes the
  cloud object.

## References

Apple Inc. (2026). *NSFileProviderManager*. Apple Developer Documentation.
https://developer.apple.com/documentation/fileprovider/nsfileprovidermanager

Apple Inc. (2026). *NSFileProviderCustomAction*. Apple Developer Documentation.
https://developer.apple.com/documentation/fileprovider/nsfileprovidercustomaction

Apple Inc. (2026). *activateFileViewerSelecting(_:)*. Apple Developer Documentation.
https://developer.apple.com/documentation/appkit/nsworkspace/activatefileviewerselecting(_:)

Apple Inc. (2026). *evictUbiquitousItem(at:)*. Apple Developer Documentation.
https://developer.apple.com/documentation/foundation/filemanager/evictubiquitousitem(at:)

Apple Inc. (2026). *ubiquitousItemIsUploadedKey*. Apple Developer Documentation.
https://developer.apple.com/documentation/foundation/urlresourcekey/ubiquitousitemisuploadedkey

Apple Inc. (2026). *URLUbiquitousItemDownloadingStatus*. Apple Developer Documentation.
https://developer.apple.com/documentation/foundation/urlubiquitousitemdownloadingstatus

Microsoft. (2026). *Deploy and configure OneDrive on macOS*. Microsoft Learn.
https://learn.microsoft.com/en-us/sharepoint/files-on-demand-mac
