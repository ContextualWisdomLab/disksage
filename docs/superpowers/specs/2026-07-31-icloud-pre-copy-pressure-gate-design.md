# iCloud pre-copy pressure gate

## Problem

A readable iCloud Drive root and a running system File Provider do not prove that adding more
local upload work is prudent. DiskSage already exposes a read-only aggregate of the local
CloudDocs upload queue, but that evidence was not part of the new-copy execution gate. A large or
unhealthy queue could therefore grow while the source was being archived.

This gate is independent of remote account capacity, copy integrity, per-item upload attestation,
and later local-source eviction approval.

## Bounded evidence

DiskSage opens the fixed local `client.db` path through SQLite URI
`immutable=1&mode=ro`. It never uses writable SQLite mode and does not apply the live WAL because
doing so may create or modify managed sidecars. The bounded query returns only counts and transfer
byte aggregates for scheduled, active, blocked, out-of-quota, unclassified, and errored work.

The public report adds:

- `new_copy_admission_state`, either `clear` or `blocked`; and
- `new_copy_admission_blockers`, containing only stable reason codes.

No local path, account identifier, user filename, user-file content, remote capacity, or
per-item synchronization claim is emitted. Queue bytes are transfer sizes, not remote quota or a
unique-unsynchronized-content measurement.

## Planning and execution

An iCloud plan adds exactly one local admission notice:

- `icloud-new-copy-admission-clear`;
- `icloud-new-copy-admission-blocked`; or
- `icloud-new-copy-admission-evidence-unavailable`.

Immediately before a new iCloud copy, DiskSage repeats the immutable probe. Pending, active,
blocked, out-of-quota, unclassified, or errored queue work blocks the copy. Failure to obtain the
local evidence also fails closed. Existing-copy adoption remains read-only and does not add queue
work, so it keeps its separate path.

A clear result permits only the next independent gates. DiskSage still requires authoritative
capacity, exact metadata review where needed, destination safety, copy-plus-hash integrity, and
provider-native per-item evidence before any source eviction.

## Current operational consequence

The observed local queue contains hundreds of thousands of scheduled and blocked entries and
about 60.69 GiB of transfer size. DiskSage must not add another iCloud copy while that positive
evidence remains. The gate does not start, stop, or reconfigure iCloud and does not modify
Passepartout or any network setting.
