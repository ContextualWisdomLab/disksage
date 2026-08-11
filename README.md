# DiskSage

> **The wise way to reclaim your disk.**
> 디스크의 현자 — 내 디스크에 뭐가 있는지 알려주고, 지워도 되는지 판별해주는 크로스플랫폼 디스크 정리 앱.

**DiskSage** is a cross-platform (Windows / Linux / macOS) disk-space manager by [ContextualWisdomLab](https://github.com/ContextualWisdomLab). It scans your drives, shows what's actually there, and uses a fully offline on-device LLM to advise whether files are safe to delete — while an OWL ontology keeps your files organized.

## Features (v1 roadmap)

- 🗺 **Large file explorer** — parallel scan with treemap visualization
- 🧹 **Known cache & temp cleanup** — OS, browser, and package-manager caches (including uv); selected developer caches are revalidated by a Rust metadata manifest (path, size, mtime, file count) immediately before trashing
- 🛠 **Dev artifact cleanup** — stale `node_modules`, `target/`, `venv`, …
- 🧭 **Stale Git worktree audit** — bounded, read-only registration evidence; prune/remove stays behind an explicit review boundary
- 👯 **Duplicate finder** — size → partial hash → BLAKE3 full hash
- 🗂 **Ontology-based organizing** — files classified into an OWL taxonomy you can edit
- 📊 **Disk inventory** — "what is on my disk?", aggregated by category, unknowns surfaced
- 🧠 **On-device LLM advisor** — embedded llama.cpp model judges delete-safety, fully offline
- ☁️ **Metadata-first cloud archive** — detects iCloud Drive, OneDrive, and Google Drive; inspects embedded file metadata, bounded dataset schemas, Rust-parsed ZIP indexes, incomplete-download archive fragments, and per-entry ZIP content inclusion without extracting payloads; verifies macOS iCloud quota through Apple's read-only native account client and revalidates authoritative OneDrive/Google account capacity through read-only OAuth with a conservative reserve; performs gated copy-plus-hash verification; and verifies macOS File Provider status first with native PKCE OAuth checksum plus exact OneDrive path or Google My Drive parent-chain fallback while retaining the source

Cloud planning is bounded as well as read-only: only the largest 32 eligible files enter the initial
external metadata-probe set, the probe wall-clock budget is 10 seconds, and duplicate-content
hashing is capped at 16 MiB per plan. Deferred probes are retained as explicit evidence and review
reasons (`content-metadata-probe-deferred` / `content-hash-deferred`); they are never reported as
verified metadata or silently treated as safe to evict.

Cache cleanup planning is bounded too: the metadata manifest has a 2-second and 100,000-record
budget per catalog entry. A partial manifest is returned with `scan_complete=false` and
`metadata-manifest-bounded`; it is display-only and cannot be submitted to the trash-delete gate.

## Safety first

Every destructive action goes through explicit review and the OS trash — DiskSage has **no permanent-delete code path**. Cache cleanup is bound to the exact candidate path, byte count, file count, and metadata fingerprint observed at review time; a changed or incomplete scan is rejected and must be refreshed. Cloud archiving currently exposes copy and evidence only: even a successful provider attestation returns a local-eviction permit without deleting the source. All destructive operations are journaled and undoable.

For a headless, read-only cache inventory, run `cargo run --locked --features cleanup-cli --bin disksage-clean-plan` (add `--id trivy-cache`, `--id pnpm-cache`, or `--id uv-cache` to inspect one candidate). The command prints the current metadata fingerprint; it never deletes files.

For a headless Git worktree audit, run `cargo run --locked --features worktree-cli --bin disksage-git-worktree-audit -- --repo /path/to/repository`. It reports missing/prunable registrations and lock evidence without invoking `git worktree prune` or `git worktree remove`. The `git worktree list` probe and each raw admin-file read are bounded; a malformed registration falls back to read-only `.git/worktrees` evidence and marks `evidence_complete: false` for manual review. Each report includes a registration fingerprint; any future metadata prune must re-audit and match that fingerprint before an explicitly reviewed operation. The operator sequence for provider permissions, metadata evidence, copy, attestation, and separate source eviction is in [`docs/cloud-offload-operator-runbook.md`](docs/cloud-offload-operator-runbook.md).

### Metadata and integration boundaries

Archive and organization decisions keep the evidence chain in this order: embedded production metadata, an explicit date in the filename as secondary evidence, filesystem creation time, then modification time. A filename date is never treated as proof on its own; context, confidence, and lineage remain attached to the candidate. The default advisor is the offline Rust/llama.cpp path (never Ollama). Noema, an external orchestrator, the semantic-data portal, `pg-erd-cloud`, and `fast-mlsirm` are integration points only when the corresponding agent, catalog/ontology, ERD, or LLM-as-a-Judge contract is actually required; the current cache/cloud safety paths do not invoke them.

## Status

🚧 Early development. See the [base design](docs/superpowers/specs/2026-07-10-disksage-design.md), [dataset metadata profile design](docs/superpowers/specs/2026-07-16-dataset-metadata-profile-design.md), [cloud OAuth security design](docs/superpowers/specs/2026-07-16-cloud-provider-oauth-pkce-design.md), and [cloud capacity evidence design](docs/superpowers/specs/2026-07-21-cloud-capacity-evidence-design.md).

## Tech

Tauri 2 · Rust · Svelte 5 · llama.cpp · OWL/Turtle

## License

MIT
