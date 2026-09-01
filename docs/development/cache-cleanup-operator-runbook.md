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
these catalogued regenerable roots, but the per-child identity, size/mtime, and complete inactive-use
checks remain mandatory. A child that is active, changed, or is DiskSage's own
`.disksage-trash-*` staging directory is skipped; the root and OS Trash are retained.

The same guarded path is available without the GUI as:

```bash
cargo run --locked --manifest-path src-tauri/Cargo.toml \
  --bin disksage-cache-cleanup -- --execute
```

Omit `--execute` for a no-op check; pass an absolute `--journal-path` when running outside the
installed application so the operation remains auditable.

## Irreversible proven-cache Trash purge

The proven-cache purge is a separate authority boundary. The command flags select the operation;
they do not authorize whatever matching caches happen to exist at execution time. Permanent
removal requires an exact candidate set that was shown and reviewed beforehand.

### 1. Preview the current candidates

Run the purge mode without `--execute` and retain the JSON output:

```bash
cargo run --locked --manifest-path src-tauri/Cargo.toml \
  --bin disksage-cache-cleanup -- \
  --purge-proven-cache-trash \
  --journal-path /ABSOLUTE/journal.jsonl \
  > /ABSOLUTE/cache-trash-preview.json
```

The `proven_cache_trash` array contains only direct OS-Trash children whose known cache name,
structural signature, bounded symlink-free tree, byte count, modification time, and filesystem
object identity were observed together. Review every displayed candidate and the reported bytes.

### 2. Persist exactly the reviewed candidate array

After review, copy only the exact `proven_cache_trash` array into a separate approval manifest. For
example, with `jq` installed:

```bash
jq '.proven_cache_trash' \
  /ABSOLUTE/cache-trash-preview.json \
  > /ABSOLUTE/approved-cache-trash.json
```

Do not add a path, edit an identity field, or regenerate the array immediately before execution.
A changed approval manifest is a new review artifact and must be reviewed again.

### 3. Execute only the reviewed set

```bash
cargo run --locked --manifest-path src-tauri/Cargo.toml \
  --bin disksage-cache-cleanup -- \
  --execute \
  --purge-proven-cache-trash \
  --approved-cache-trash-candidates /ABSOLUTE/approved-cache-trash.json \
  --journal-path /ABSOLUTE/journal.jsonl
```

Execution validates the entire approved set before the first irreversible mutation and revalidates
each candidate immediately before its own removal. A reviewed candidate that moved, was replaced,
changed size or modification time, changed filesystem identity or signature, became a symlink, or
is no longer an exact direct Trash child causes a fail-closed result rather than widening authority.
A new matching cache that appears after preview is not in the approval manifest and therefore
remains untouched until a later preview and review.

Each attempted purge writes a `pending` journal record before deletion and a terminal `ok` or
`error` record afterward. The path never empties Trash generally and never targets cloud
placeholders, arbitrary Trash entries, or user files. Review the command result, both journal
states, and measured filesystem free-space evidence before treating reported logical bytes as
physically reclaimed capacity.

The cache catalog includes the macOS `uv`, Hugging Face, Codex runtime, Gradle, npm, pip, and Cargo
registry cache/source roots when present. The Cargo registry source root is catalogued for explicit
review but is not part of the automatic six-cache action because rebuilding it may require network
downloads. Cache contents are not cloud candidates: they are reproducible local artifacts, while
user files continue through the metadata-first cloud planner and its provider sync/eviction gates.
