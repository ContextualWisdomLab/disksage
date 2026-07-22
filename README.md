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
- ☁️ **Cloud offload planner** — detects iCloud Drive, OneDrive, and Google Drive; previews old/large archive candidates with embedded-metadata-first production time and source-path lineage (dry-run only)

## Safety first

Every destructive action goes through explicit review and the OS trash — DiskSage has **no permanent-delete code path**. All operations are journaled and undoable.

## Status

🚧 Early development. See the [design spec](docs/superpowers/specs/2026-07-10-disksage-design.md).

## Tech

Tauri 2 · Rust · Svelte 5 · llama.cpp · OWL/Turtle

## License

MIT
