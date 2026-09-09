# ADR 0023: Use PhotoKit rather than Photos library package traversal

Status: Accepted

## Context

Apple Photos libraries can include locally materialized originals and iCloud-only assets. Treating
the `.photoslibrary` package as an ordinary directory bypasses Photos authorization, relationship,
change-notification, and system deletion semantics. A file path is therefore not deletion authority.

## Decision

DiskSage uses a macOS-native PhotoKit boundary and never descends into or mutates a managed Photos
library package. Read/write authorization is requested only after the customer selects **Connect
Photos**. Inventory is bounded to 10,000 image assets and 512 MiB per locally available original;
resource requests disable network access, so iCloud-only originals remain unmaterialized and block
all destructive planning.

Exact groups require SHA-256 content identity. Width, height, pixel count, encoded bytes, resource
type, and UTI remain separate measured evidence; no composite score or arbitrary weight chooses a
keeper. Near-duplicate deletion remains unavailable until measured equivalence evidence exists.
The customer must select one keeper per exact group, enter the fresh exact approval phrase and a
rationale, and then accept Photos' own deletion confirmation. Immediately before the change,
DiskSage re-fetches local identifiers and re-reads local content without network access. The change
uses `PHPhotoLibrary.performChanges`/`PHAssetChangeRequest.deleteAssets`; an immutable, create-new
receipt records only successful completion. Non-macOS builds fail closed.

## Consequences

- iCloud-only assets are preserved without implicit download.
- Changes made by Photos, another device, or another app invalidate the reviewed evidence at the
  identifier, metadata, or content recheck.
- Deleted assets follow Photos' Recently Deleted and system-confirmation behavior; DiskSage never
  directly unlinks a managed original.
- Limited Photos access inventories only the assets Apple exposes, and the UI states the next action.

## Rejected alternatives

- Direct `.photoslibrary` traversal or deletion: bypasses PhotoKit authority and can corrupt the library.
- Filesystem Trash for managed originals: bypasses Photos' change transaction and confirmation.
- Automatically downloading iCloud originals: creates disk pressure and expands mutation scope.
- Weighted “best photo” scoring: no calibrated model supports an arbitrary cross-metric weight.

## References

Apple. (n.d.). *Delivering an enhanced privacy experience in your Photos app*. Apple Developer.
https://developer.apple.com/documentation/photokit/delivering-an-enhanced-privacy-experience-in-your-photos-app

Apple. (n.d.). *Observing changes in the photo library*. Apple Developer.
https://developer.apple.com/documentation/photokit/observing-changes-in-the-photo-library

Apple. (n.d.). *PHAssetChangeRequest*. Apple Developer.
https://developer.apple.com/documentation/photos/phassetchangerequest

Apple. (n.d.). *PHPhotoLibrary*. Apple Developer.
https://developer.apple.com/documentation/photos/phphotolibrary

Apple. (n.d.). *Requesting changes to the photo library*. Apple Developer.
https://developer.apple.com/documentation/photokit/requesting-changes-to-the-photo-library
