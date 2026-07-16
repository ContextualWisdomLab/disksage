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
