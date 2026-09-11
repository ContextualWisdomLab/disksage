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

This ADR does **not** choose a silent fallback locale. A missing resource, locale, or screen key must
remain explicit until product requirements separately define fallback behavior and corresponding
rendered acceptance. The initial schema/cache slice likewise does not claim that Tauri persistence or
localized screens are shipped.

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

The persistence transaction is intentionally small: install/import a complete version as one bounded
operation, then serve read-only lookup traffic. UI rendering must not perform cross-service SQL and
must not mutate ledger rows.

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

A SQLite schema alone is not product localization. Remaining work includes the native persistence
adapter, packaged resource installation, screen-key migration of user-facing copy, locale selection,
missing-message policy, realistic German/French/Spanish/Vietnamese expansion, CJK rendering, all
normal/loading/empty/error/permission states, keyboard/focus parity, and native Windows WebView2 and
macOS WebKit evidence.

## Evidence and acceptance

The first implementation slice must execute the migration against a real SQLite engine in the
supported Node test runtime and prove locale, foreign-key, uniqueness, append-only, and cache-version
invariants. Source-string matching is not sufficient. The implementing PR remains Draft until exact
head tests and owned frontend production coverage pass.

This ADR remains Proposed while the implementing line is unmerged. It cannot be Accepted merely
because schema tests pass; acceptance additionally requires the native repository/adapter and at least
one end-to-end localized screen-key flow, after which later UI slices can prove all supported locales
and native shells.

## Traceability

- Issue #340 — material UI multilingual/native-shell delivery gap.
- Issue #391 — DB-backed versioned translation ledger boundary.
- Draft #392 — executable schema and lookup-cache foundation.
- SQLite. (2026). *STRICT tables*. https://www.sqlite.org/stricttables.html
- SQLite. (2026). *SQLite foreign key support*. https://www.sqlite.org/foreignkeys.html
- SQLite. (2026). *PRAGMA statements supported by SQLite*. https://www.sqlite.org/pragma.html
- Phillips, A., & Davis, M. (Eds.). (2009). *Tags for identifying languages* (BCP 47, RFC 5646).
  RFC Editor. https://www.rfc-editor.org/rfc/rfc5646.html
