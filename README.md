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
- ☁️ **Metadata-first cloud archive** — detects iCloud Drive, OneDrive, and Google Drive; inspects embedded file metadata, bounded dataset schemas, Rust-parsed ZIP indexes, and incomplete-download archive fragments without extracting payloads; verifies macOS iCloud quota through Apple's read-only native account client and revalidates authoritative OneDrive/Google account capacity through read-only OAuth with a conservative reserve; performs gated copy-plus-hash verification; verifies macOS File Provider status first with native PKCE OAuth checksum plus exact OneDrive path or Google My Drive parent-chain fallback; and distinguishes normal provider-confirmation waits from overdue unconfirmed copies while retaining the source

## Safety first

Every destructive action goes through explicit review and the OS trash — DiskSage has **no permanent-delete code path**. A receipt-bound source can move to Trash only after fresh provider attestation, exact receipt confirmation, attributed human approval, and bounded active-use checks. iCloud local-copy eviction retains the cloud item and requires an exact native-status plan fingerprint. All destructive operations produce immutable evidence records.

For one exact multi-item iCloud local-cache approval, the Rust batch coordinator re-plans every
manifest item before the first mutation, reports unavailable inputs without exposing their paths,
and stops after the first failed or unverified result:

```sh
cargo run --manifest-path src-tauri/Cargo.toml --features cloud-cli \
  --bin disksage-icloud-local-eviction-batch -- \
  --cloud-root "/absolute/iCloud/root" \
  --manifest "/absolute/local/private-plan.json"
```

The default JSON output is path-redacted. Execution additionally requires `--execute`, the exact
batch fingerprint in both `--approved-batch-fingerprint` and `--confirm-batch-fingerprint`,
`--approved-by human:IDENTITY`, a non-empty `--rationale`, and an existing local `--record-dir`
outside cloud storage. All selected paths are re-planned and all individual approval records are
created before the first eviction request; a create-new checkpoint follows every attempt.

## Status

🚧 Early development. See the [base design](docs/superpowers/specs/2026-07-10-disksage-design.md), [dataset metadata profile design](docs/superpowers/specs/2026-07-16-dataset-metadata-profile-design.md), [cloud OAuth security design](docs/superpowers/specs/2026-07-16-cloud-provider-oauth-pkce-design.md), and [cloud capacity evidence design](docs/superpowers/specs/2026-07-21-cloud-capacity-evidence-design.md).

## Tech

Tauri 2 · Rust · Svelte 5 · llama.cpp · OWL/Turtle

## License

MIT
