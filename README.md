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

## Status

🚧 Early development. See the [base design](docs/superpowers/specs/2026-07-10-disksage-design.md), [dataset metadata profile design](docs/superpowers/specs/2026-07-16-dataset-metadata-profile-design.md), [cloud OAuth security design](docs/superpowers/specs/2026-07-16-cloud-provider-oauth-pkce-design.md), [cloud capacity evidence design](docs/superpowers/specs/2026-07-21-cloud-capacity-evidence-design.md), [redacted Naruon capacity export design](docs/superpowers/specs/2026-07-29-naruon-cloud-capacity-export-design.md), and [semantic catalog pre-copy export design](docs/superpowers/specs/2026-07-29-semantic-catalog-export-design.md).

## Tech

Tauri 2 · Rust · Svelte 5 · llama.cpp · OWL/Turtle

## License

MIT
