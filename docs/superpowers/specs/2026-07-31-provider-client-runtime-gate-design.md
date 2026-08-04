# Provider client runtime gate

## Problem

A readable directory under macOS `~/Library/CloudStorage` proves only that the
File Provider root can be enumerated at that moment. After a restart, the local
vendor runtime can be absent while its root remains visible. Copying into that
root without detecting the missing runtime can create a locally staged file
that has no timely path to remote synchronization.

This prerequisite is separate from account authentication, remote capacity,
copy integrity, provider sync attestation, and local-source eviction approval.

## Bounded local evidence

DiskSage runs the fixed macOS command `/bin/ps -Ac -o comm=` with no shell,
null stdin and stderr, a three-second timeout, and a 64 KiB output cap. Standard
output is drained concurrently while the child runs, retaining at most the cap
plus one sentinel byte so a full pipe cannot deadlock the process and oversized
evidence fails closed. Timeout and wait-error paths kill and reap the child and
join the reader before returning. DiskSage compares trimmed process names only
against exact, built-in OneDrive and Google Drive client names. Substrings and
command-line arguments do not count. iCloud is recorded separately as a
system-managed File Provider.

The emitted version 1 snapshot contains:

- provider, observation time, evidence kind, and runtime state;
- an explicit local copy-prerequisite result and stable blocker;
- a SHA-256 fingerprint binding the provider, time, state, and blocker; and
- explicit false claims for raw process names, local paths, remote capacity,
  remote sync, and cloud writes.

No process name, command line, PID, path, account identifier, token, or provider
response is serialized. Failure to collect or decode the bounded process list
is `evidence-unavailable`, not proof that the client is stopped.

## Copy and planning behavior

Live plans replace `provider-client-runtime-unverified` with one of:

- `provider-client-runtime-observed`;
- `provider-client-runtime-not-observed`; or
- `provider-client-runtime-evidence-unavailable`.

A new OneDrive or Google Drive copy rebuilds the plan and then repeats this
runtime check immediately before the independent capacity gate. Missing or
unavailable runtime evidence fails closed. Existing-copy adoption is not a new
local write and therefore keeps its separate validation path.

Runtime presence does not make a candidate copy-ready by itself. A new copy
still requires the existing metadata review, destination, collision,
authoritative account-capacity, and copy-integrity gates. Source eviction still
requires later provider-native remote sync evidence and explicit approval.

## Headless audit

`disksage-provider-client-runtime` produces one path-free report for iCloud,
OneDrive, and Google Drive. `--output` accepts only an absolute path and writes a
new mode-0600 file without overwriting an existing artifact:

```sh
cargo run --manifest-path src-tauri/Cargo.toml --features cloud-cli \
  --bin disksage-provider-client-runtime -- \
  --output /absolute/new-provider-runtime-audit.json
```

The audit is read-only. It does not start or stop a provider client, inspect
browser or credential state, contact a provider API, write a cloud root, or
attest synchronization.
