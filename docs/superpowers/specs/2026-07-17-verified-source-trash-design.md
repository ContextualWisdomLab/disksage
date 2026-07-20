# Verified cloud source trash design

## Goal

Free local space only after a copy receipt and fresh provider-native synchronization evidence prove that the cloud destination contains the same bytes. The action moves the local source to the operating-system Trash; it never permanently deletes data.

## Safety contract

The headless action requires all of the following in one invocation:

1. An immutable, integrity-valid copy receipt.
2. Fresh provider-native evidence for the receipt destination and a matching `LocalEvictionPermit`.
3. An operator confirmation value exactly equal to the 64-hex receipt id.
4. An absolute eviction-record directory and destructive-operation journal path.
5. A regular, non-symlink source whose size, modification time, BLAKE3, SHA-256, QuickXor, and stable file identity still match the receipt.

Failure is closed and leaves the source untouched.

## Durable lineage receipt

Receipt version 3 seals the metadata evidence that justified the archive destination before any
source can be trashed. The receipt carries an immutable `CloudLineageSnapshot` containing the
candidate and review fingerprints, optional review-decision id, disposition and timestamp, archive
kind, destination account scope, embedded production time and confidence, source-relative context,
title, authors, contextual fields, duration, dataset profile, and every bounded metadata-evidence
item. A separate lineage fingerprint hashes the canonical snapshot and is itself included in the
receipt id.

Receipt validation fails closed when the lineage is missing, changed, or belongs to another
candidate. Existing version 2 receipts remain readable and integrity-valid with their original id
algorithm so already verified provider copies do not need to be hydrated or recopied. Version 2
receipts cannot claim that they contain the new lineage snapshot.

Because lineage can contain sensitive local context, the writer rejects symlink receipt
directories, enforces the existing 64 KiB receipt bound before creation, and uses owner-read-only
`0400` permissions on Unix. A write failure rolls back the just-created cloud copy and never touches
the source.

## Account-scoped destination suitability

Cloud roots carry a deterministic `personal`, `organization`, `shared`, or `unknown` account
scope. Known consumer domains and OneDrive Personal labels classify as personal; non-consumer
email domains classify as organization; Google shared-drive roots classify as shared. iCloud and
ambiguous provider labels remain unknown instead of being guessed.

The selected scope is copied into each candidate, bound into the review fingerprint, displayed in
the UI, and sealed in the version 3 lineage snapshot. Changing only the destination account scope
therefore invalidates an earlier operator decision. Unknown and shared scopes always require
review. A personal destination with recording, personal-data, confidential, geolocation, opaque
container, or sensitive dataset evidence adds
`personal-cloud-sensitive-context-needs-explicit-approval`.

The copy gate independently requires the candidate scope to match the freshly selected cloud root
scope. A caller cannot construct an organization-scoped candidate and copy it into a personal root.

Before collection, both the CLI and Tauri command verify that the source directory can actually be
enumerated. This distinguishes an empty source from macOS privacy controls that permit metadata
lookup but deny `read_dir`; unreadable roots fail with `source-root-unreadable` instead of emitting
a misleading zero-candidate plan.

## Crash-safe sequence

DiskSage writes a bounded, read-only, receipt-bound intent with `create_new`, flushes it, and fsyncs its directory. It then atomically renames the source within its existing directory to a deterministic hidden staging path. The staged file is verified again before the existing trash-only safety API is called.

The intent makes interrupted operations resumable:

- source present, staging absent: verify and stage;
- source absent, staging present: verify and resume Trash;
- both present: stop as ambiguous;
- neither present: record a reconciled completion without touching another path.

After Trash succeeds, DiskSage writes a bounded, read-only completion record. A completion record prevents receipt replay, including deletion of a later file recreated at the original path.

## CLI surface

`disksage-cloud-plan` adds one mutually exclusive action:

```text
--evict-receipt RECEIPT.json
--confirm-receipt-id HEX64
--eviction-dir ABSOLUTE_PATH
--journal-path ABSOLUTE_PATH
```

The action recollects provider-native evidence immediately before eviction. It does not accept a serialized permit or stale evidence from another process.

## Scope

This slice stays deterministic and Rust-only. It does not require Noema, an external LLM, semantic-data-portal, pg-erd-cloud, or fast-mlsirm. The real personal source is not evicted during development or tests.
