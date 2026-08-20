# Cache cleanup runtime evidence

DiskSage exposes only named, regenerable cache roots. The root itself is preserved; the UI first
asks Rust for its bounded direct-child snapshot, including logical bytes, modification time, and
filesystem object identity.

The cleanup command accepts that exact snapshot and refuses the operation when the catalog root or
any child changed. Each child is then moved to the operating-system Trash through the shared
identity-bound staging primitive and journal. The Trash is not emptied, so the operation remains
reversible.

For the current macOS low-disk incident, the **관측된 재생성 캐시 자동 정리** action invokes
`clean_regenerable_caches`. It is intentionally limited to npm (`~/.npm`), uv (`~/.cache/uv` or
`UV_CACHE_DIR`), pnpm, Adobe, Microsoft Edge, and Trivy. No extra approval phrase is needed for
these catalogued regenerable roots, but
the per-child identity, size/mtime, and complete inactive-use checks remain mandatory. A child
that is active, changed, or is DiskSage's own `.disksage-trash-*` staging directory is skipped; the
root and OS Trash are retained.

The same guarded path is available without the GUI as
`cargo run --locked --manifest-path src-tauri/Cargo.toml --bin disksage-cache-cleanup -- --execute`.
Omit `--execute` for a no-op check; pass an absolute `--journal-path` when running outside the
installed application so the operation remains auditable.

The cache catalog includes the macOS `uv`, Hugging Face, Codex runtime, Gradle, npm, pip, and Cargo
registry caches when present. Cache contents are not cloud candidates: they are reproducible local
artifacts, while user files continue through the metadata-first cloud planner and its provider
sync/eviction gates.
