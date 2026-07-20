# Cloud Provider Capacity Evidence Design

## Purpose

DiskSage must not infer remote cloud capacity from local APFS free space. Before a new OneDrive or
Google Drive copy, it reads the provider's authoritative account-capacity metadata, applies a
conservative reserve, and blocks the copy when the evidence is missing or insufficient. Planning
remains read-only and actual source eviction remains a separate, later step.

Reading embedded metadata or filesystem timestamps does not require administrator permission.
Provider capacity is different: it needs the user's existing read-only OAuth grant so that the
provider can identify which account's quota to return. DiskSage stores refresh credentials in the OS
credential store and never asks for a provider write scope merely to inspect quota.

## Provider contracts

| Provider | Authoritative evidence | DiskSage rule |
| --- | --- | --- |
| OneDrive | Microsoft Graph `GET /me/drive?$select=id,driveType,quota` with `Files.Read` | Bind the evidence fingerprint to the drive ID, require `total`, `used`, `remaining`, and a recognized provider quota state. |
| Google Drive | Drive v3 `about.get` with `drive.metadata.readonly` and an explicit `fields` mask | Bind the evidence fingerprint to the authenticated user's `permissionId`; use total cross-service `storageQuota.usage`, not only `usageInDrive`; honor `maxUploadSize`. |
| iCloud Drive | No supported third-party API in the macOS File Provider surface for the user's account quota | Report `icloud-quota-api-unavailable`; never substitute APFS free space. Continue only with source-preserving copy and later per-file sync evidence. |

Google Workspace pooled-storage accounts can return organization-wide limit and usage. DiskSage
therefore includes `google-capacity-may-reflect-pooled-organization-storage` as a notice rather than
mislabeling the figures as necessarily personal.

## Decision gate

The default reserve is 1 GiB. A candidate is copy-eligible only when a fresh provider snapshot proves
that `candidate bytes + reserve <= remaining bytes`, the provider is not in `exceeded` state, and the
largest upload is within the provider's advertised maximum. Integer overflow, malformed numeric
strings, unknown states, redirects, oversized responses, and missing account-binding fields fail
closed.

The plan-level assessment uses the total potentially reclaimable candidate bytes. A plan may report
that the full batch does not fit even though a smaller individual candidate can fit; the copy command
therefore obtains a fresh snapshot and evaluates that exact candidate immediately before copying.
Adopting an already-present cloud object does not upload bytes and skips this capacity gate.

No capacity check authorizes deletion. DiskSage still retains the source until copy hash verification,
provider sync attestation, immutable receipt validation, and an explicit local-eviction confirmation
all succeed.

## Restart and connection verification

A non-secret connection descriptor is not proof that the OS credential store still contains the
matching refresh token or that the provider still accepts it. After launch, DiskSage therefore labels
the descriptor as discovered rather than claiming the account is connected. The user can explicitly
run `verify_cloud_provider_capacity`, which reads the credential store and contacts only the fixed
provider metadata endpoint on a blocking worker.

The command returns provider evidence on success. On failure it emits an unavailable snapshot with
one of a bounded set of redacted reasons: missing or ambiguous descriptor, invalid descriptor
document, unavailable credential, failed OAuth refresh, or unavailable provider API. Raw token,
transport, response-body, and account-identifier details never cross the Tauri command boundary.
Planning uses the same redaction so a missing connection is distinguishable from an insufficient
quota without weakening the copy gate.

## References

- [Microsoft Graph drive quota resource](https://learn.microsoft.com/en-us/graph/api/resources/quota?view=graph-rest-1.0)
- [Microsoft Graph get drive](https://learn.microsoft.com/en-us/graph/api/drive-get?view=graph-rest-1.0)
- [Google Drive v3 About resource](https://developers.google.com/workspace/drive/api/reference/rest/v3/about)
- [Google Drive v3 about.get](https://developers.google.com/workspace/drive/api/reference/rest/v3/about/get)
- [Apple File Provider](https://developer.apple.com/documentation/fileprovider)
