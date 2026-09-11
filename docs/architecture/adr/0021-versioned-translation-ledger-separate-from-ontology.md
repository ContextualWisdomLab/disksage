# ADR-0021: UI translations use a versioned ledger separate from ontology

**Status:** Proposed  
**Date:** 2026-09-11  
**Scope:** Localized Presentation Resource bounded context

## Context

DiskSage currently renders user-facing copy directly from component source while its ontology owns
filesystem classification vocabulary. Issue #340 requires KO/EN/JA/ZH/VI/ES/DE/FR presentation,
text-expansion evidence, and explicit font fallback. Reusing ontology labels as UI translations would
mix two authorities: a domain classification term may remain stable while product copy, accessibility
wording, and supported locales change independently.

A local-first desktop application also needs translation lookup to work without network availability.
The resource loaded by the product must therefore have an explicit version and stable screen keys so
rendered evidence can identify which translation release was exercised. Settings JSON is mutable user
configuration and is not an authoritative translation ledger.

ADR number 0020 is already reserved by the active cache-Trash owner #263. This Draft uses 0021 rather
than creating a conflicting record.

## Decision

Create a **Localized Presentation Resource** bounded context owned by DiskSage presentation code.
Filesystem ontology remains a separate bounded context and is not a Shared Kernel for translated UI
copy.

The persistence contract is a normalized local SQLite ledger:

- `translation_resource_versions` identifies an immutable resource release by `resource_version`,
  schema version, creation time, and SHA-256 content digest.
- `translation_screen_keys` owns stable screen keys separately from wording. Keys use bounded
  lower-case product namespaces; multiword terms use underscores inside a namespace segment.
- `translation_messages` identifies one text value by the composite key
  `(resource_version, locale, screen_key)` and references both owner tables.
- The first admitted locale set is `ko`, `en`, `ja`, `zh`, `vi`, `es`, `de`, and `fr`. These are
  BCP 47 primary-language tags; expanding or specializing that set requires an explicit contract
  change rather than accepting arbitrary strings.
- Published resource versions, screen keys, and messages are append-only. A wording change creates a
  new resource version; database triggers reject UPDATE and DELETE on published ledger rows.
- Each SQLite connection must enable foreign-key enforcement. The executable schema test does so
  explicitly rather than relying on a library default.
- UI lookup cache identity is `(resource_version, locale, screen_key)`. Cache entries are disposable
  projections and never become source of truth. A resource-version change cannot reuse a stale entry.
- The ledger stores presentation text only. It contains no filesystem path, ontology triple/class,
  user identifier, telemetry record, authentication material, or deletion authority.

The application bundle additionally carries immutable presentation-resource bytes. A build admits one
known resource release by a compile-time `ResourceVersion`, bundle-relative path, and lower-case
SHA-256 digest. Native admission is fail closed before publication: the resource must be a bounded
regular non-symlink file whose digest, embedded resource version, schema version, stable screen keys,
non-empty messages, and exact KO/EN/JA/ZH/VI/ES/DE/FR locale set all match the build contract. A
future application adapter resolves that fixed path with Tauri's resource-directory resolver; the
frontend does not supply a path and a network response cannot become translation authority.

The first packaged resource in #396 intentionally contains only real `scan` and `cancel` action copy.
It proves packaging and native integrity admission, not screen-wide migration, locale selection,
persistence, or rendered localization. When native persistence installs a resource version, the
verified content digest is the value that can be associated with that immutable release; this ADR
does not permit recomputing authority from mutable UI state.

This ADR does **not** choose a silent fallback locale. A missing resource, locale, or screen key must
remain explicit until product requirements separately define fallback behavior and corresponding
rendered acceptance. The initial schema/cache and packaged-resource slices likewise do not claim that
native SQLite persistence, Tauri IPC, or localized screens are shipped.

## DDD and transaction boundary

Subdomain: user-facing localized presentation.  
Bounded Context: Localized Presentation Resource.  
Aggregate: `TranslationResourceVersion`, whose published messages are immutable.  
Entities/VOs: `ResourceVersion`, `ScreenKey`, `LocaleTag`, `TranslationMessage`.  
Repository: a future native adapter will open the local ledger and return a resource version plus
screen-key lookups; UI code consumes that repository through a narrow application boundary.  
Invariant: wording cannot change in place for an already identified resource version.  
Invariant: ontology labels never satisfy a missing presentation message.  
Invariant: cache lookup always includes resource version and locale.  
Invariant: bundled resource bytes are not admitted unless they match the build-pinned release digest.

The persistence transaction is intentionally small: install/import a complete version as one bounded
operation, then serve read-only lookup traffic. UI rendering must not perform cross-service SQL and
must not mutate ledger rows. The packaged-resource reader has no filesystem mutation authority and
cannot elevate a resource path chosen by presentation code into an authorization capability.

## Alternatives rejected or deferred

- **Keep strings in Svelte components.** Rejected as the authority because rendered locale/version
  provenance cannot be established and eight-language state parity becomes source-code branching.
- **Use `settings.json` as the translation store.** Rejected because settings are mutable user
  configuration, have different lifecycle semantics, and cannot provide normalized version/message
  integrity.
- **Reuse OWL/RDF ontology labels.** Rejected because classification semantics and presentation copy
  have independent owners, release cadence, accessibility constraints, and localization needs.
- **Use ad-hoc component maps as the canonical store.** Rejected because they duplicate keys and do
  not create a versioned resource boundary. Generated/read-only projections may exist later but must
  point back to the ledger version.
- **Fetch translations from a remote service at render time.** Rejected for the local-first baseline;
  network availability must not be required to explain a disk-safety decision.
- **Let the frontend choose a resource pathname.** Rejected because resource identity is a release
  contract, not presentation input. The native application resolves the build-pinned bundle path.
- **Trust a packaged path without authenticating bytes.** Rejected because packaging provenance alone
  does not prove that the bytes correspond to the declared immutable `ResourceVersion`.
- **Make current-version rows mutable.** Rejected because exact-head UI evidence would no longer
  identify the wording actually rendered.
- **Choose automatic English or Korean fallback now.** Deferred. Missing-copy behavior affects user
  comprehension in destructive/recovery workflows and requires explicit product acceptance rather
  than an implementation default.

## Risks and effects

Append-only releases consume more local metadata than in-place updates, but translation resources are
small compared with scanned filesystem data and deterministic provenance is more valuable than
micro-optimizing this storage. A later retention policy must preserve any version referenced by audit
or acceptance evidence.

Digest verification detects changed bytes but is not a code-signing substitute. Platform package
signing, native persistence, rollback, and release provenance remain separate controls. The read-only
resource loader therefore rejects malformed or unexpected content but does not claim to establish a
new trust root outside the application build.

A SQLite schema and authenticated bundle alone are not product localization. Remaining work includes
the native persistence/application adapter, screen-key migration of user-facing copy, locale
selection, missing-message policy, realistic German/French/Spanish/Vietnamese expansion, CJK
rendering, all normal/loading/empty/error/permission states, keyboard/focus parity, and native Windows
WebView2 and macOS WebKit evidence.

## Evidence and acceptance

The first implementation slice (#392) executes the migration against a real SQLite engine in the
supported Node test runtime and proves locale, foreign-key, uniqueness, append-only, and cache-version
invariants. Exact `a4e587bfe384337da4df635eb7ad4d3397e68026`, Test `34592826429`, is terminal SUCCESS.

The next slice (#396) starts from that verified parent and requires a Rust integration contract plus
real file/resource tests for the build-pinned path/digest, tampering, schema/version mismatch,
locale/key/message validation, size limits, and Unix symlink rejection. Its own current-head Test must
settle independently; predecessor GREEN does not transfer.

This ADR remains Proposed while the implementing line is unmerged. It cannot be Accepted merely
because schema or bundle-integrity tests pass; acceptance additionally requires the native
repository/adapter and at least one end-to-end localized screen-key flow, after which later UI slices
can prove all supported locales and native shells.

## Traceability

- Issue #340 — material UI multilingual/native-shell delivery gap.
- Issue #391 — DB-backed versioned translation ledger boundary.
- Draft #392 — executable schema and lookup-cache foundation; exact `a4e587...`, Test `34592826429` SUCCESS.
- Issue #395 / Draft #396 — immutable bundled-resource admission and native digest verification.
- SQLite. (2026). *STRICT tables*. https://www.sqlite.org/stricttables.html
- SQLite. (2026). *SQLite foreign key support*. https://www.sqlite.org/foreignkeys.html
- SQLite. (2026). *PRAGMA statements supported by SQLite*. https://www.sqlite.org/pragma.html
- Tauri Programme. (2026). *PathResolver::resolve and BaseDirectory::Resource* (Tauri 2.9.3).
  https://docs.rs/tauri/2.9.3/tauri/path/struct.PathResolver.html
- Phillips, A., & Davis, M. (Eds.). (2009). *Tags for identifying languages* (BCP 47, RFC 5646).
  RFC Editor. https://www.rfc-editor.org/rfc/rfc5646.html