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
installed application so the reversible operation remains auditable.

## Proven-cache Trash preview — permanent purge disabled

DiskSage may inspect known regenerable cache directories that are already direct children of OS
Trash, but **permanent purge execution is currently disabled**. The earlier path-based deletion
implementation cannot yet prove that the reviewed descendant tree remains the exact object being
removed across the irreversible mutation, and it does not yet provide the required freshness and
post-crash receipt reconciliation. Those gaps must be repaired before permanent deletion can become
a product capability.

The supported operator action today is read-only preview only:

```bash
cargo run --locked --manifest-path src-tauri/Cargo.toml \
  --bin disksage-cache-cleanup -- \
  --purge-proven-cache-trash \
  --journal-path /ABSOLUTE/journal.jsonl \
  > /ABSOLUTE/cache-trash-preview.json
```

The `proven_cache_trash` array contains direct OS-Trash children whose known cache name, structural
signature, bounded symlink-free tree, byte count, modification time, and filesystem object identity
were observed. This is review evidence only; it is **not deletion authority**.

Do not treat an approval manifest as a way to re-enable the operation. Any invocation containing
both `--execute` and `--purge-proven-cache-trash` fails before journal-directory creation or cache
mutation, whether or not `--approved-cache-trash-candidates` is supplied. The CLI reports that
permanent purge is unavailable and directs the operator back to the safe preview above.

A future re-enable must be a separately reviewed implementation that, at minimum, binds the full
reviewed descendant identity to a descriptor-relative/no-follow deletion primitive, has an explicit
approval-freshness boundary, resists pathname replacement races, and can reconcile an interrupted
or failed terminal journal write without repeating deletion. Its tests must prove that equal-sized
nested replacements and concurrent target replacement cannot cross the reviewed boundary.

Until those conditions are met, leave the reviewed cache directories in OS Trash or use operating-
system/user-managed Trash controls outside DiskSage. DiskSage does not claim permanently reclaimed
bytes from the disabled path.

The cache catalog includes the macOS `uv`, Hugging Face, Codex runtime, Gradle, npm, pip, and Cargo
registry cache/source roots when present. The Cargo registry source root is catalogued for explicit
review but is not part of the automatic six-cache action because rebuilding it may require network
downloads. Cache contents are not cloud candidates: they are reproducible local artifacts, while
user files continue through the metadata-first cloud planner and its provider sync/eviction gates.
