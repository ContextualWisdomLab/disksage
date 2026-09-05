# Cache cleanup runtime evidence

DiskSage exposes only named, regenerable cache roots. The root itself is preserved; the UI first
asks Rust for its bounded direct-child snapshot, including logical bytes, modification time, and
filesystem object identity.

The supported cleanup command accepts that exact snapshot and refuses the operation when the
catalog root or a reviewed child changed. Each eligible child is moved to the operating-system
Trash through the identity-bound staging primitive and journal. DiskSage does not empty Trash as
part of this ordinary cleanup flow, so the product action remains reversible.

For the current catalogued regenerable-cache action, npm, uv, pnpm, Adobe, Microsoft Edge, and Trivy
roots are treated independently. A child that is active, changed, ambiguous, or is DiskSage's own
`.disksage-trash-*` staging directory is skipped; the root and unrelated children are retained.

The same reversible path is available without the GUI as:

```bash
cargo run --locked --manifest-path src-tauri/Cargo.toml \
  --bin disksage-cache-cleanup -- --execute
```

Omit `--execute` for a no-op check; pass an absolute `--journal-path` when running outside the
installed application so the reversible operation remains auditable.

## Proven-cache Trash inspection

DiskSage may inspect known regenerable cache directories that are already direct children of OS
Trash. That inspection is useful for understanding local storage, but it is not permanent-deletion
authority.

Use read-only preview mode:

```bash
cargo run --locked --manifest-path src-tauri/Cargo.toml \
  --bin disksage-cache-cleanup -- \
  --purge-proven-cache-trash \
  --journal-path /ABSOLUTE/journal.jsonl \
  > /ABSOLUTE/cache-trash-preview.json
```

The current `proven_cache_trash` array is review evidence with exactly the candidate's `name`,
`path`, logical `bytes`, and recognized structural `signature`. The bounded scanner rejects
symlinks while deriving that evidence, but the preview payload does not currently publish
modification time, filesystem object identity, or a full descendant fingerprint. The preview is
therefore inspection evidence only and cannot authorize an irreversible mutation.

## Permanent purge is not an operator-supported capability

The protected source lineage has contained a legacy pathname-based permanent-purge path. That
implementation has known replacement-race, descendant-identity, approval-freshness, and
post-delete receipt-reconciliation gaps. Its presence in a checkout does **not** make it a supported
DiskSage operation.

On the canonical fail-closed cache-safety lineage, supplying `--execute` together with
`--purge-proven-cache-trash` is rejected with
`cache-trash-identity-bound-permanent-delete-unavailable` before journal or filesystem mutation.
Do not run that combination from a revision that still exposes a legacy irreversible implementation.
The canonical cache-safety repair must reach protected authority with its exact-head tests/reviews
before operator documentation can treat the fail-closed source behavior as shipped truth, much less
re-enable irreversible deletion.

A future irreversible implementation requires all of the following before it can become supported:

- full reviewed descendant identity rather than shallow size/signature evidence;
- explicit approval freshness/expiry;
- descriptor-relative/no-follow or equivalently strong mutation binding so validation and deletion
  act on the same filesystem object;
- deterministic replacement-race tests proving a swapped sentinel target is never removed;
- durable pending/terminal evidence plus restart/retry reconciliation without repeated deletion;
- rejection of symlinks/reparse points, nested candidates, arbitrary Trash entries, user files and
  provider placeholders; and
- platform-specific integration evidence for every enabled platform.

Until that evidence is integrated, leave reviewed cache directories in OS Trash or use the
operating system's own user-managed Trash controls. DiskSage does not claim physically reclaimed
capacity from an unsupported permanent-purge path.

## Catalog scope

The cache catalog can include uv, Hugging Face, Codex runtime, Gradle, npm, pip, Cargo and other
known local cache/source roots when present. Cataloguing a root is not mutation authority. Some
roots—such as source registries that may need network access to rebuild—remain explicit-review
rather than automatic-cleanup targets.

Cache contents are not cloud candidates: they are reproducible local artifacts. User files continue
through the metadata-first cloud planner and its provider sync, review and reversible local-action
boundaries.
