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
- 🧾 **Evidence-bound reclaim planning** — separates logical size, observed allocation, and
  unverified physical reclaimability instead of promising `du` bytes
- 🧠 **On-device LLM advisor** — embedded llama.cpp model judges delete-safety, fully offline

## Safety first

Every destructive action goes through explicit review and the OS trash — DiskSage has **no permanent-delete code path**. All operations are journaled and undoable. Moving data to Trash does not free its blocks until Trash is emptied, and APFS clone sharing can make physical recovery smaller than logical size.

### Read-only reclaim evidence

```sh
cargo run --manifest-path src-tauri/Cargo.toml --bin disksage-reclaim-plan -- \
  --operation trash --pretty PATH...
```

The JSON report never moves or deletes supplied paths. It is identified by
`schema_kind: disksage.reclaim-plan` and `schema_version: 1`, and reports logical bytes and observed
allocated bytes, while `physically_reclaimable_bytes` remains `null` until strong post-operation or
filesystem-native evidence exists. Interchange output is bounded to 1,000 normalized roots and
4,096 UTF-8 bytes per canonical path; non-UTF-8 and control-character paths fail closed instead of
being serialized ambiguously.

### Read-only Podman VM evidence

```sh
cargo run --manifest-path src-tauri/Cargo.toml --bin disksage-podman-reclaim-plan -- \
  --machine podman-machine-default --pretty
```

This bounded Rust probe separates the guest filesystem, Podman store, raw image logical size, and
observed host allocation. It emits `schema_kind: disksage.podman-reclaim-plan` and never prunes
images, removes containers or volumes, changes machine state, or runs TRIM. Podman/guest reported
space is not promoted to host physical reclaim proof; privileged or destructive follow-up remains
human-approved and must be verified with a before/after host free-space observation.

## Status

🚧 Early development. See the [design spec](docs/superpowers/specs/2026-07-10-disksage-design.md).

## Tech

Tauri 2 · Rust · Svelte 5 · llama.cpp · OWL/Turtle

## License

MIT
