# Naruon cloud-copy readiness envelope

## Problem

A cloud root being readable is not enough evidence to copy a Downloads
candidate. DiskSage independently evaluates metadata review, destination
safety, the local provider client, remote account capacity, and provider-wide
synchronization admission (the immutable CloudDocs queue for iCloud and
bounded File Provider evidence for OneDrive/Google Drive). Naruon needs one
bounded contract that
can validate those pre-copy facts without receiving filenames, paths, account
identifiers, or raw embedded metadata.

The contract is advisory evidence. It never approves a human review, writes to
a provider, attests synchronization, or authorizes local source eviction.

## Contract

`disksage.naruon.cloud-copy-readiness` version 7 contains:

- provider and destination account scope;
- the DiskSage decision-batch fingerprint;
- candidate, planner-unblocked, review-required, and currently-ready
  count/byte aggregates;
- production-time evidence aggregates in the fixed order embedded metadata,
  explicit filename date, filesystem creation time, then filesystem modified
  time;
- bounded blocker-code aggregates;
- the complete path-free provider-client runtime snapshot;
- the complete provider-authoritative capacity assessment;
- for iCloud, waiting and active upload queue counts/bytes plus the remaining
  admission blocker inputs and a bounded native `brctl status` summary
  (`needs-sync-up` and `needs-sync-down` are blockers, even when the private queue is quiet).
- for iCloud, the path-free, integrity-checked three-stream pre-copy evidence cohort and an
  explicit `pre_copy_evidence_met` binding; a missing, incomplete, stale, or tampered cohort
  blocks readiness even when the provider queue is quiet.
- for OneDrive and Google Drive, bounded provider-wide File Provider transfer
  and indexing state, without retaining provider paths or filenames.

`filesystem:modified-fallback` belongs to the filesystem-modified aggregate.
Filename dates remain auxiliary evidence even when they are the selected
production-time source.

The envelope fixes these claims to false:

- human review decisions applied;
- local paths, relative names, raw metadata, or account identifiers included;
- provider synchronization attested;
- cloud write executed;
- source eviction authorized.

## Readiness calculation

DiskSage starts with the existing per-candidate transfer blockers. It then adds:

1. the exact provider-runtime blocker when the bounded local observation is not
   satisfied;
2. the capacity blocker obtained by reassessing that candidate against the
   exported authoritative snapshot and reserve;
3. for iCloud, every current queue/native-status blocker, or
   `icloud-new-copy-admission-evidence-unavailable` when the immutable local
   probe cannot be obtained, plus `pre-copy-evidence-cohort-unavailable` when
   the three-stream freshness/integrity cohort is missing or incomplete.
4. for OneDrive and Google Drive, every provider-global-sync blocker, or
   `provider-global-sync-evidence-unavailable` when the bounded dump cannot be
   obtained.

A candidate is `ready_without_new_review` only when no blocker remains. The
envelope state is `no-candidates`, `blocked`, `partially-ready`, or
`ready-without-new-review`. This does not bypass a later fresh copy-time check.

## Fingerprint

DiskSage clears `readiness_fingerprint_sha256`, converts the complete envelope
to JSON, recursively sorts every object by UTF-8 key, emits compact UTF-8 JSON,
and hashes those bytes with SHA-256. Arrays retain their declared order.
Contract strings are bounded ASCII identifiers and notices, avoiding
cross-runtime Unicode normalization ambiguity.

The known Rust test vector for the fixed OneDrive fixture is:

`958f7a8e2e595f119bfd38f0ee231436217e3cb97c4d2745fdcfb5e29b5a299c`

Naruon must reconstruct the same canonical form and digest. It must also
recompute all semantic invariants; accepting a newly signed contradiction is
not allowed.

## Read-only CLI

The export requires one discovered destination and a fresh capacity attempt:

```bash
cargo run --manifest-path src-tauri/Cargo.toml \
  --features cloud-cli \
  --bin disksage-cloud-plan -- \
  --root "/absolute/source" \
  --provider onedrive \
  --verify-capacity \
  --export-naruon-copy-readiness \
  --naruon-copy-readiness-output "/absolute/new-readiness.json"
```

The optional output must be absolute, its parent must already be a real
directory, and the command creates a new mode-0600 file. It refuses to replace
an existing file. The operation scans local metadata and collects read-only
evidence; it does not copy, upload, evict, start a provider client, or modify a
cloud account.

OneDrive and Google Drive capacity remains unavailable without an existing
DiskSage read-only OAuth connection descriptor and credential. iCloud uses
Apple's bounded native quota client, immutable CloudDocs queue snapshot, and a
bounded read-only `brctl status` summary; timeout or missing native output is
retained as evidence-incomplete rather than guessed from local APFS free space.

## Integration boundary

The Naruon receiver is a strict, authenticated, aggregate-only validation
endpoint. It performs no filesystem, database, provider API, agent, or model
call. Noema, contextual-orchestrator, semantic-data-portal, pg-erd-cloud, and
fast-mlsirm are therefore outside this deterministic contract.
