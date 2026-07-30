# DiskSage

> **The wise way to reclaim your disk.**
> 디스크의 현자 — 내 디스크에 뭐가 있는지 알려주고, 지워도 되는지 판별해주는 크로스플랫폼 디스크 정리 앱.

**DiskSage** is a cross-platform (Windows / Linux / macOS) disk-space manager by [ContextualWisdomLab](https://github.com/ContextualWisdomLab). It scans your drives, shows what's actually there, and uses a fully offline on-device LLM to advise whether files are safe to delete — while an OWL ontology keeps your files organized.

## Features (v1 roadmap)

- 🗺 **Large file explorer** — parallel scan with treemap visualization
- 🧹 **Known cache & temp cleanup** — OS, browser, and package-manager caches
- 🛠 **Dev artifact cleanup** — stale `node_modules`, `target/`, `venv`, …
- 👯 **Duplicate finder** — size → partial hash → BLAKE3 full hash
- 🗂 **Ontology-based organizing** — files classified into an OWL taxonomy you can edit
- 📊 **Disk inventory** — "what is on my disk?", aggregated by category, unknowns surfaced
- 🧠 **On-device LLM advisor** — embedded llama.cpp model judges delete-safety, fully offline
- ☁️ **Metadata-first cloud archive** — detects iCloud Drive, OneDrive, and Google Drive; inspects embedded file metadata, bounded dataset schemas, Rust-parsed ZIP indexes, and incomplete-download archive fragments without extracting payloads; exports a bounded path-free pre-copy preview contract for semantic-data-portal; verifies macOS iCloud quota through Apple's read-only native account client and revalidates authoritative OneDrive/Google account capacity through read-only OAuth with a conservative reserve; performs gated copy-plus-hash verification; verifies macOS File Provider status first with native PKCE OAuth checksum plus exact OneDrive path or Google My Drive parent-chain fallback; and distinguishes normal provider-confirmation waits from overdue unconfirmed copies while retaining the source
- 🧩 **Split-archive set audit** — groups `.zip.partNNN` siblings, proves internal gaps and duplicate indices, totals discard-review bytes, and emits a stable path-redacted fingerprint while keeping exact paths in an optional create-new mode-0600 private dossier
- ⏳ **Incomplete-download audit** — inventories `.crdownload` files by bounded magic bytes, embedded ZIP structure, acquisition context, filesystem-modified staleness, final-sibling presence, and bounded active-use evidence without treating download time or filename dates as production dates
- 🩺 **Incomplete-download recovery validation** — decodes bounded PNG payloads and streams bounded whole-file or embedded ZIP ranges to EOF with entry CRC checks, without extraction, rename, or discard
- 🧬 **Incomplete-download materialization plan** — binds fresh audit and recovery fingerprints to exact non-overlapping byte ranges, SHA-256/BLAKE3 content lineage, and content-addressed filename suggestions while withholding destination selection and write approval
- ☁️ **Destination-bound recovery approval plan** — binds validated output units to one discovered cloud root, relative destination, fresh provider capacity evidence, collision checks, and one exact human-approval fingerprint without creating any output
- 🌿 **Stale Git worktree audit** — resolves an exact retention-reference OID set, preserves every exact retained tip plus primary/current/dirty/unmerged/locked/prunable/active worktree, measures bounded allocated bytes, and emits a path-redacted fingerprint and exact approval phrase without pruning or removing anything

## Safety first

Every destructive action goes through explicit review and the OS trash — DiskSage has **no permanent-delete code path**. Cloud archiving currently exposes copy and evidence only: even a successful provider attestation returns a local-eviction permit without deleting the source. All destructive operations are journaled and undoable.

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

Stale-worktree auditing is read-only. It does not fetch or assume that local remote-tracking
references are current, so operators should refresh every selected reference before auditing.
`--reference-ref` is repeatable: use the integration branch and every current open-PR exact head.
An exact retained tip is always preserved. A different secondary worktree is a removal candidate
only when its HEAD is already contained in at least one resolved retention OID, its tracked and
untracked state is clean, its bounded allocated-byte scan is complete, it is neither locked nor
prunable, and no active CWD or recursive `lsof` consumer is observed. Local paths, branch names,
and reference names appear only in an optional create-new mode-0600 report. The public approval
phrase is plan evidence; this command has no remove, prune, branch-delete, or filesystem mutation
path.

```sh
cargo run --features cloud-cli --bin disksage-git-worktree-audit -- \
  --repository-root /absolute/repository/worktree \
  --reference-ref origin/develop \
  --reference-ref CURRENT_OPEN_PR_HEAD_OID \
  --private-output /absolute/private/new-git-worktree-audit.json
```

## Status

🚧 Early development. See the [base design](docs/superpowers/specs/2026-07-10-disksage-design.md), [dataset metadata profile design](docs/superpowers/specs/2026-07-16-dataset-metadata-profile-design.md), [cloud OAuth security design](docs/superpowers/specs/2026-07-16-cloud-provider-oauth-pkce-design.md), [cloud capacity evidence design](docs/superpowers/specs/2026-07-21-cloud-capacity-evidence-design.md), [redacted Naruon capacity export design](docs/superpowers/specs/2026-07-29-naruon-cloud-capacity-export-design.md), and [semantic catalog pre-copy export design](docs/superpowers/specs/2026-07-29-semantic-catalog-export-design.md).

## Tech

Tauri 2 · Rust · Svelte 5 · llama.cpp · OWL/Turtle

## License

MIT
