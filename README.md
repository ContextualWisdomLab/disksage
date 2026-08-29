# DiskSage

> **The wise way to reclaim your disk.**
> 디스크의 현자 — 내 디스크에 뭐가 있는지 알려주고, 지워도 되는지 판별해주는 크로스플랫폼 디스크 정리 앱.

**DiskSage** is a cross-platform (Windows / Linux / macOS) disk-space manager by [ContextualWisdomLab](https://github.com/ContextualWisdomLab). It scans your drives, shows what's actually there, and uses a fully offline on-device LLM to advise whether files are safe to delete — while an OWL ontology keeps your files organized.

## Features (v1 roadmap)

- 🗺 **Large file explorer** — parallel scan with treemap visualization
- 🧹 **Known cache & temp cleanup** — OS, browser, and package-manager caches
- 🛠 **Dev artifact cleanup** — stale `node_modules`, `target/`, `venv`, …
- 👯 **Duplicate finder** — size → partial hash → BLAKE3 full hash
- 🗂 **Ontology-based organizing** — files classified into an OWL taxonomy you can edit; move plans use a complete bounded scan, bind metadata-first production-time lineage and source size/mtime, revalidate them immediately before moving, and skip File Provider dataless sources
- 📊 **Disk inventory** — "what is on my disk?", aggregated by category, unknowns surfaced
- 🧠 **On-device LLM advisor** — embedded llama.cpp model judges delete-safety and ontology classes from bounded metadata-first lineage evidence, fully offline
- ☁️ **Metadata-first cloud archive** — detects iCloud Drive, OneDrive, and Google Drive; inspects embedded file metadata, bounded dataset schemas, Rust-parsed ZIP indexes, and incomplete-download archive fragments without extracting payloads; exports a bounded path-free pre-copy preview contract for semantic-data-portal; verifies macOS iCloud quota through Apple's read-only native account client and revalidates authoritative OneDrive/Google account capacity through read-only OAuth with a conservative reserve when configured; for personal roots, permits copy-only through an observed native desktop client when OAuth quota evidence is the only missing input; requires a fresh bounded local provider-client runtime observation before a new vendor-root copy; refuses to add a new iCloud item while the read-only local CloudDocs queue reports pending, blocked, out-of-quota, unclassified, or errored work; performs gated copy-plus-hash verification; verifies macOS File Provider status first with native PKCE OAuth checksum plus exact OneDrive path or Google My Drive parent-chain fallback; and distinguishes normal provider-confirmation waits from overdue unconfirmed copies while retaining the source
- 🟢 **Provider client runtime gate** — observes only bounded process names, never emits a command line, path, account identifier, or process name, and blocks new OneDrive/Google Drive copies when the local vendor runtime is not observed; runtime presence remains only a local prerequisite and never becomes an account-authentication, capacity, or sync-completion claim
- ⏸️ **iCloud pre-copy pressure gate** — reads the private CloudDocs database through immutable SQLite mode, a bounded native `brctl status` summary, and a path-free `fileproviderctl dump` activity probe; emits only queue/state aggregates and stable blocker codes, and fails closed before a new iCloud copy when the existing local upload queue is non-empty, unhealthy, native status times out, or File Provider reports no-progress fetches; a quiet queue still does not prove remote capacity, per-item synchronization, or eviction safety
- 🧾 **Naruon cloud-copy readiness envelope** — combines path-free production-time evidence aggregates, planner/review blockers, provider-client runtime, authoritative capacity assessment, and iCloud queue/native status; binds them with a recursively key-sorted SHA-256 fingerprint while keeping every write, sync, review, and eviction authority false
- 🧩 **Split-archive set audit** — groups `.zip.partNNN` siblings, proves internal gaps and duplicate indices, totals discard-review bytes, and emits a stable path-redacted fingerprint while keeping exact paths in an optional create-new mode-0600 private dossier
- ⏳ **Incomplete-download audit** — inventories `.crdownload` files by bounded magic bytes, embedded ZIP structure, acquisition context, filesystem-modified staleness, final-sibling presence, and bounded active-use evidence without treating download time or filename dates as production dates
- 🩺 **Incomplete-download recovery validation** — decodes bounded PNG payloads and streams bounded whole-file or embedded ZIP ranges to EOF with entry CRC checks, without extraction, rename, or discard
- 🧬 **Incomplete-download materialization plan** — binds fresh audit and recovery fingerprints to exact non-overlapping byte ranges, SHA-256/BLAKE3 content lineage, and content-addressed filename suggestions while withholding destination selection and write approval
- ☁️ **Destination-bound recovery approval plan** — binds validated output units to one discovered cloud root, relative destination, fresh provider capacity evidence, collision checks, and one exact human-approval fingerprint without creating any output
- 🌿 **Stale Git worktree audit/removal** — resolves an exact retention-reference OID set, preserves every exact retained tip plus primary/current/dirty/unmerged/locked/prunable/active worktree, measures bounded allocated bytes, and gates clean merged inactive candidates behind a path-bound fingerprint, exact approval phrase, immediate re-audit, and immutable approval/result records; branch deletion and `git worktree prune` are unreachable

## Safety first

Every user-file destructive action goes through explicit review and the OS trash — DiskSage has **no permanent-delete code path** for those files. Developer-artifact selections carry a bounded, metadata-only fingerprint, byte/file counts, scan status, and a platform filesystem-object identity; the Rust command re-scans immediately before trashing, atomically stages the exact identity in a private sibling directory, and rejects changed, recreated, unreadable, or incomplete candidates. Cloud archiving currently exposes copy and evidence only: even a successful provider attestation returns a local-eviction permit without deleting the source. Stale Git worktree removal is the explicit repository-management exception: it invokes non-force `git worktree remove` only for clean, merged, inactive, fingerprint-identical candidates, records immutable approval/result evidence, retains branches, and never runs prune. All user-file trash operations are journaled and retain their private recovery directory so OS-trash undo has a valid staged target, while restoring to the original path remains a separate recovery step.

The headless split-archive audit is read-only. A contiguous sequence does not invent proof that its
last observed member is the terminal part, and a missing-part result is never automatic deletion
authority:

```sh
cargo run --features cloud-cli --bin disksage-multipart-archive-audit -- \
  --root /absolute/source \
  --private-output /absolute/private/new-audit.json
```

Incomplete-download auditing is also read-only. Its default 30-day threshold is only a review
signal: detected payload types can still be incomplete, and undetected payloads may still be
partially recoverable.

```sh
cargo run --features cloud-cli --bin disksage-incomplete-download-audit -- \
  --root /absolute/source \
  --stale-after-days 30 \
  --private-output /absolute/private/new-incomplete-download-audit.json
```

Recovery validation must follow a fresh audit and remains read-only. PNG output memory, ZIP entry
count, individual uncompressed size, and total uncompressed size are bounded. A successful result
is recovery evidence only and does not authorize automatic extension restoration or discard.

```sh
cargo run --features cloud-cli --bin disksage-incomplete-download-recovery -- \
  --root /absolute/source \
  --stale-after-days 30 \
  --private-output /absolute/private/new-incomplete-download-recovery.json
```

Materialization planning repeats the fresh audit and recovery validation, hashes every validated
range, and remains destination-independent. Exact paths, offsets, digests, and suggested filenames
appear only in the optional create-new mode-0600 private report. The public summary cannot authorize
output creation because no destination has been selected.

```sh
cargo run --features cloud-cli --bin disksage-incomplete-download-materialization -- \
  --root /absolute/source \
  --stale-after-days 30 \
  --private-output /absolute/private/new-incomplete-download-materialization.json
```

The next read-only stage can bind that plan to an existing discovered cloud root and a relative
destination. It verifies provider capacity and destination collisions, but still creates no
directory or output. Paths and proposed names remain only in the optional mode-0600 private report.

```sh
cargo run --features cloud-cli --bin disksage-incomplete-download-destination-plan -- \
  --source-root /absolute/source \
  --cloud-root /absolute/existing/cloud/root \
  --destination-subdirectory DiskSage/Recovered/IncompleteDownloads \
  --live-icloud-capacity \
  --private-output /absolute/private/new-incomplete-download-destination-plan.json
```

Materialization is a separate mutating command. It refuses to run without the exact destination
plan fingerprint, attributed human approval, an explicit `--execute` flag, fresh provider capacity,
and a private receipt directory outside both source and cloud roots. It regenerates and compares
the full source lineage, stages every range with create-new names, verifies all planned digests,
then promotes each verified inode with a no-clobber create-new hard link before removing its
staging name. Failures roll back files created by that invocation. It never renames, discards,
trashes, or deletes a source, and the resulting receipt does not claim provider sync or authorize
local-source eviction.

```sh
cargo run --features cloud-cli --bin disksage-incomplete-download-materialize -- \
  --source-root /absolute/source \
  --destination-plan /absolute/private/destination-plan.json \
  --confirm-plan-fingerprint LOWERCASE_HEX64 \
  --receipt-dir /absolute/private/receipts \
  --approved-by human:reviewer \
  --rationale "Approved exact provider, account, units, bytes, and destination plan" \
  --live-icloud-capacity \
  --execute
```

Stale-worktree auditing does not fetch or assume that local remote-tracking
references are current, so operators should refresh every selected reference before auditing.
`--reference-ref` is repeatable: use the integration branch and every current open-PR exact head.
An exact retained tip is always preserved. A different secondary worktree is a removal candidate
only when its HEAD is already contained in at least one resolved retention OID, its tracked and
untracked state is clean, its bounded allocated-byte scan is complete, it is neither locked nor
prunable, and no active CWD or recursive `lsof` consumer is observed. Local paths, branch names,
and reference names appear only in an optional create-new mode-0600 report. The public approval
phrase is plan evidence; it is not execution authority by itself.

```sh
cargo run --features cloud-cli --bin disksage-git-worktree-audit -- \
  --repository-root /absolute/repository/worktree \
  --reference-ref origin/develop \
  --reference-ref CURRENT_OPEN_PR_HEAD_OID \
  --private-output /absolute/private/new-git-worktree-audit.json
```

After reviewing that exact private report, the mutating command repeats the full audit immediately
before removal. It requires the unchanged plan fingerprint, exact approval phrase, attributed
reviewer, rationale, and a record root outside every audited worktree. It removes only the
currently matching candidates and stops on any drift; branches remain and no prune is performed.

```sh
cargo run --features cloud-cli --bin disksage-git-worktree-remove -- \
  --repository-root /absolute/repository/worktree \
  --reference-ref origin/develop \
  --approved-removal-plan-fingerprint LOWERCASE_HEX64 \
  --confirmation-exact-approval-phrase 'DiskSage stale worktree … 승인 LOWERCASE_HEX64' \
  --reviewed-by human:reviewer \
  --rationale "Merged, clean, inactive worktree with no retained unmerged commits" \
  --record-root /absolute/private/disksage-app-data
```

## Local volume evidence CLI

DiskSage can capture a read-only, path-redacted filesystem-capacity snapshot:

```sh
cargo run --manifest-path src-tauri/Cargo.toml \
  --features volume-cli \
  --bin disksage-volume-snapshot -- --path /System/Volumes/Data
```

The JSON reports native `total`, `free`, and user-available bytes, allocation granularity,
available-space basis points, and a deterministic pressure band. It includes a SHA-256 evidence
fingerprint but never emits the queried path, mount name, account identifier, or file content.

To compare a fresh observation with a previously saved snapshot:

```sh
cargo run --manifest-path src-tauri/Cargo.toml \
  --features volume-cli \
  --bin disksage-volume-snapshot -- \
  --path /System/Volumes/Data \
  --baseline before.json \
  --logical-removed-bytes 3806089216
```

The comparison binds both complete snapshots and the calculated deltas. A logical removal count is
recorded as operator evidence only: `physical_reclaim_bytes` remains `null` and attribution remains
`unproven`, because APFS, sync providers, swap, builds, and other concurrent writers can change free
space during the same interval. Baselines are limited to a regular, non-symlink JSON file of at most
64 KiB.

## Status

🚧 Early development. See the [base design](docs/superpowers/specs/2026-07-10-disksage-design.md), [dataset metadata profile design](docs/superpowers/specs/2026-07-16-dataset-metadata-profile-design.md), [cloud OAuth security design](docs/superpowers/specs/2026-07-16-cloud-provider-oauth-pkce-design.md), [cloud capacity evidence design](docs/superpowers/specs/2026-07-21-cloud-capacity-evidence-design.md), [provider client runtime gate design](docs/superpowers/specs/2026-07-31-provider-client-runtime-gate-design.md), [iCloud pre-copy pressure gate design](docs/superpowers/specs/2026-07-31-icloud-pre-copy-pressure-gate-design.md), [Naruon cloud-copy readiness design](docs/superpowers/specs/2026-07-31-naruon-cloud-copy-readiness-design.md), [redacted Naruon capacity export design](docs/superpowers/specs/2026-07-29-naruon-cloud-capacity-export-design.md), and [semantic catalog pre-copy export design](docs/superpowers/specs/2026-07-29-semantic-catalog-export-design.md).

## Tech

Tauri 2 · Rust · Svelte 5 · llama.cpp · OWL/Turtle

## License

MIT
