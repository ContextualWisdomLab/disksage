# Cloud Provider Capacity Evidence Design

## Purpose

DiskSage must not infer remote cloud capacity from local APFS free space. Before a new iCloud,
OneDrive, or Google Drive copy, it reads authoritative account-capacity metadata, applies a
conservative reserve, and blocks the copy when the evidence is missing or insufficient. Planning
remains read-only and actual source eviction remains a separate, later step.

Reading embedded metadata or filesystem timestamps does not require administrator permission.
Provider capacity is different: OneDrive and Google Drive need the user's existing read-only OAuth
grant so that the provider can identify which account's quota to return. DiskSage stores refresh
credentials in the OS credential store and never asks for a provider write scope merely to inspect
quota. On macOS, Apple's own `brctl quota` client identifies the signed-in personal iCloud account,
so that path needs neither administrator permission nor a separate OAuth client.

## Provider contracts

| Provider | Authoritative evidence | DiskSage rule |
| --- | --- | --- |
| OneDrive | Microsoft Graph `GET /me/drive?$select=id,driveType,quota` with `Files.Read` | Bind the evidence fingerprint to the drive ID, require `total`, `used`, `remaining`, and a recognized provider quota state. |
| Google Drive | Drive v3 `about.get` with `drive.metadata.readonly` and an explicit `fields` mask | Bind the evidence fingerprint to the authenticated user's `permissionId`; use total cross-service `storageQuota.usage`, not only `usageInDrive`; honor `maxUploadSize`. |
| iCloud Drive | Apple's read-only `/usr/bin/brctl quota`, pinned to the C locale | Accept only the exact bounded `N bytes of quota remaining in personal account` shape, bind the fingerprint to the personal-account scope, and retain total/used as unknown. Timeout, localization drift, malformed output, and unsupported platforms fail closed. Never substitute APFS free space. |

Google Workspace pooled-storage accounts can return organization-wide limit and usage. DiskSage
therefore includes `google-capacity-may-reflect-pooled-organization-storage` as a notice rather than
mislabeling the figures as necessarily personal.

## Decision gate

The default reserve is 1 GiB. A candidate is copy-eligible only when a fresh provider snapshot proves
that `candidate bytes + reserve <= remaining bytes`, the provider is not in `exceeded` state, and the
largest upload is within the provider's advertised maximum. Integer overflow, malformed numeric
strings, unknown states, redirects, oversized responses, and missing account-binding fields fail
closed.

The iCloud native client exposes remaining bytes but not total or used bytes. DiskSage therefore
labels a positive native snapshot `available` rather than inventing a provider health percentage;
the same exact byte-plus-reserve comparison still gates the copy. A zero remaining count is
`exceeded`.

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

The command returns provider evidence on success. For iCloud it runs the fixed Apple binary on a
blocking worker with a five-second timeout, a 128 KiB output bound, suppressed stderr, and no shell.
For OneDrive and Google Drive it refreshes the saved credential and contacts the fixed API endpoint.
On failure it emits an unavailable snapshot with
one of a bounded set of redacted reasons: missing or ambiguous descriptor, invalid descriptor
document, unavailable credential, failed OAuth refresh, unavailable provider API, or unavailable
native quota status. Raw token,
transport, response-body, and account-identifier details never cross the Tauri command boundary.
Planning uses the same redaction so a missing connection is distinguishable from an insufficient
quota without weakening the copy gate.

## References

- [Microsoft Graph drive quota resource](https://learn.microsoft.com/en-us/graph/api/resources/quota?view=graph-rest-1.0)
- [Microsoft Graph get drive](https://learn.microsoft.com/en-us/graph/api/drive-get?view=graph-rest-1.0)
- [Google Drive v3 About resource](https://developers.google.com/workspace/drive/api/reference/rest/v3/about)
- [Google Drive v3 about.get](https://developers.google.com/workspace/drive/api/reference/rest/v3/about/get)
- [Apple File Provider](https://developer.apple.com/documentation/fileprovider)
- `brctl help` on macOS, which documents the read-only `quota` command
