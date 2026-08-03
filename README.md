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

## Safety first

Every destructive action goes through explicit review and the OS trash — DiskSage has **no permanent-delete code path**. All operations are journaled and undoable.

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

🚧 Early development. See the [design spec](docs/superpowers/specs/2026-07-10-disksage-design.md).

## Tech

Tauri 2 · Rust · Svelte 5 · llama.cpp · OWL/Turtle

## License

MIT
