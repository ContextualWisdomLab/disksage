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
- ☁️ **Metadata-first cloud archive** — detects iCloud Drive, OneDrive, and Google Drive; inspects embedded file metadata plus bounded CSV/TSV/JSONL schemas without retaining cell values; performs gated copy-plus-hash verification; and verifies provider metadata with native PKCE OAuth while retaining the source

## Safety first

Every destructive action goes through explicit review and the OS trash — DiskSage has **no permanent-delete code path**. Cloud archiving currently exposes copy and evidence only: even a successful provider attestation returns a local-eviction permit without deleting the source. All destructive operations are journaled and undoable.

## Status

🚧 Early development. See the [base design](docs/superpowers/specs/2026-07-10-disksage-design.md), [dataset metadata profile design](docs/superpowers/specs/2026-07-16-dataset-metadata-profile-design.md), and [cloud OAuth security design](docs/superpowers/specs/2026-07-16-cloud-provider-oauth-pkce-design.md).

## Tech

Tauri 2 · Rust · Svelte 5 · llama.cpp · OWL/Turtle

## License

MIT
