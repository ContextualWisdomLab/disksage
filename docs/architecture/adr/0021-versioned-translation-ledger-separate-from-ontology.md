# ADR-0021: UI translations use a versioned ledger separate from ontology

**Status:** Proposed  
**Date:** 2026-09-11  
**Scope:** Localized Presentation Resource bounded context

## Context

DiskSage needs KO/EN/JA/ZH/VI/ES/DE/FR presentation while its filesystem ontology separately owns classification vocabulary. Those authorities cannot be merged: a domain classification term can remain stable while product copy, accessibility wording and supported locales change. The product is local-first, so lookup must also work without network availability and exact UI evidence must identify the translation release that supplied a message.

Settings JSON is mutable user configuration and is not a translation ledger. Ontology labels are not a fallback translation store. ADR number 0020 is already owned by the cache-Trash line, so this decision remains ADR-0021.

## Decision

The **Localized Presentation Resource** bounded context owns presentation resources, immutable release identity, native persistence and exact localized lookup. Filesystem ontology remains a separate bounded context.

The normalized local SQLite ledger is the schema introduced by #392:

- `translation_resource_versions(resource_version, schema_version, created_at_unix_ms, content_sha256)` identifies one immutable release.
- `translation_screen_keys(screen_key, screen_area)` owns stable presentation keys independently of wording.
- `translation_messages(resource_version, locale, screen_key, text_value)` identifies one message by the exact composite key `(resource_version, locale, screen_key)`.
- The admitted locale set is exactly `ko`, `en`, `ja`, `zh`, `vi`, `es`, `de`, `fr`; changing it is a contract change.
- Published resource versions, screen keys and messages are append-only. SQLite triggers reject UPDATE and DELETE on published rows.
- Each SQLite connection enables foreign-key enforcement.
- Lookup/cache identity always contains resource version, locale and screen key. Cache entries are disposable projections, never source of truth.
- The ledger contains presentation text and release metadata only. It contains no filesystem path, ontology triple/class, user identifier, telemetry record, authentication material or deletion authority.

A build admits exactly one bundled resource through a compile-time resource version, bundle-relative path and SHA-256 digest. Native admission is fail closed: the asset must be a bounded regular non-symlink file and its digest, embedded resource version, schema version, screen keys, non-empty messages and exact locale set must match the build contract.

Presentation IPC may supply only an explicit locale and stable screen key. It cannot choose a resource path, database path, resource version, digest, filesystem root, ontology identifier or fallback locale. A missing resource, locale, key or message remains explicit; this ADR does not authorize silent English/Korean fallback.

### Native persistence rule

Draft #407 implements the first native ledger adapter. The bridge resolves and authenticates the build-owned resource **before** opening a SQLite write transaction. The database location is native application authority under the application data directory. Frontend IPC cannot replace it.

Installation is idempotent for an already-published identical resource. If an existing `resource_version` disagrees on schema, digest, screen-area projection or message content, installation fails closed rather than mutating published rows. The write transaction is bounded to local ledger reads/inserts; bundle reads, digest verification, network/LLM calls, filesystem scanning and cross-service SQL do not occur while the explicit write lock is held.

The native SQLite binding is pinned to `rusqlite = 0.37.0` with bundled SQLite while this line proves the actual dependency graph against DiskSage's declared Rust 1.88 toolchain on Windows/Linux/macOS. The pin is not release authority until the exact Cargo lock graph and hosted platform builds are verified.

### Stable `screen_area` projection

The #392 schema requires `screen_area`, while the immutable resource owns stable `screen_key` values. The projection is therefore part of this bounded context rather than an ad-hoc persistence default:

- `screen_area` is the namespace before the final `.` in a valid stable key.
- `app.action.scan` therefore persists as screen key `app.action.scan` with screen area `app.action`.
- The projected area must be non-empty, at most 80 bytes, and contain only lower-case ASCII letters, digits, `.`, `_` or `-`.
- A key without a projectable namespace, or a projection that violates the schema bound, fails before the write transaction starts.
- `screen_area` is presentation grouping metadata only. It does not grant filesystem, ontology, authorization or deletion authority.

This rule preserves the already-released stable key as identity; it does not rewrite or infer the key from component names at runtime.

## DDD and transaction boundary

Subdomain: user-facing localized presentation.  
Bounded Context: Localized Presentation Resource.  
Aggregate: `TranslationResourceVersion`; its published messages are immutable.  
Entities/VOs: `ResourceVersion`, `ScreenKey`, `ScreenArea`, `LocaleTag`, `TranslationMessage`.  
Repository: build-pinned bundle admission plus native SQLite immutable-release store.  
Application service: native translation bridge installs the admitted release idempotently and serves one exact `(resource_version, locale, screen_key)` lookup.  
Invariants: wording cannot change in place; ontology labels never satisfy a missing presentation message; cache identity includes version and locale; unverified bundle bytes never enter the ledger; presentation input cannot choose path/version/digest/database/fallback authority.

The transaction is intentionally small. Resource-file I/O and integrity checks complete first. A native install transaction then performs bounded local schema/release reads and inserts and commits. Read-only lookup occurs outside that write transaction. UI rendering performs no cross-service SQL.

## Alternatives rejected or deferred

- Strings embedded in Svelte components are not canonical authority because they cannot establish locale/version provenance.
- `settings.json` is rejected as the ledger because its lifecycle is mutable user configuration.
- OWL/RDF labels are rejected because classification semantics and presentation copy have different owners and release cadence.
- Ad-hoc component maps are rejected as canonical storage; generated projections may exist only when tied to an immutable resource version.
- Remote translation lookup at render time is rejected for the local-first baseline.
- Frontend-supplied resource or database paths are rejected because they convert presentation input into native authority.
- Mutable current-version rows and `INSERT OR REPLACE` semantics are rejected because they destroy exact-release evidence.
- Holding a SQLite write transaction while reading/verifying bundle files is rejected because resource verification is not a database critical section.
- Automatic English or Korean fallback is deferred. Missing-copy behavior in destructive/recovery workflows needs explicit product acceptance and rendered E2E evidence.

## Risks and effects

Append-only releases consume more local metadata than in-place updates, but translation resources are small relative to scanned filesystem data and deterministic provenance is more valuable. A future retention policy must preserve versions referenced by audit or acceptance evidence.

Digest verification is not a code-signing substitute. Package signing, release provenance, rollback and native persistence remain separate controls. Tauri command registration also does not replace input authorization; future multi-window trust zones must review capability scoping explicitly.

The bundled SQLite dependency reduces host-SQLite drift but increases the native dependency/reproducibility surface. The exact lockfile, SBOM/provenance and Windows/Linux/macOS builds therefore remain release requirements.

A schema, authenticated bundle, native store and IPC projection are still not complete product localization. Remaining work includes migrating real user-facing copy to stable screen keys, locale selection, explicit missing/fallback policy, German/French/Spanish/Vietnamese text expansion, CJK/font fallback, normal/loading/empty/error/permission states, keyboard/focus parity, and Windows WebView2/macOS WebKit current-head E2E evidence.

## Evidence and acceptance

- #392 exact `a4e587bfe384337da4df635eb7ad4d3397e68026`, Test `34592826429`: terminal SUCCESS for real-SQLite schema, locale, foreign-key, uniqueness, append-only and cache-version invariants.
- #396 exact `bf7fe773815872b2b8fc1b03200ec0102f1c0114`, Test `34599835468`: terminal SUCCESS for build-pinned path/digest, tampering, schema/version, locale/key/message, size and Unix-symlink admission checks.
- #398 exact `198b0f17720fb8e987ecbe339c2ab41520b278a8`, Test `34604043086`: terminal SUCCESS for fixed-resource read-only Tauri projection. It is the exact-GREEN parent of the native persistence line.
- #406 records the native persistence buyer gap and transaction/ownership constraints.
- #407 contract-first exact `fa9c4093a5dd6151e25f93e0dec30c65a2d4c7a7` required native persistence before the implementation existed. Current implementation exact `a4d867c12ba1c1bf1f3e5f6c4a0f2337ece6954c` adds the crate-private native store, pinned bundled SQLite binding, native app-data database authority, idempotent immutable installation, exact lookup and `screen_area` projection. Its fresh Test `34773864531` is queued/nonterminal at this update; no predecessor GREEN transfers to it.

This ADR remains **Proposed**. Native persistence must earn exact current-head Windows/Linux/macOS evidence and the dependency lock graph must be immutable. Acceptance additionally requires at least one real end-to-end localized screen-key flow; full UI delivery still requires all supported locales and native shells.

## Traceability

- Issue #340 — material multilingual/native-shell delivery gap.
- Issue #391 — DB-backed versioned translation-ledger boundary.
- Draft #392 — executable SQLite schema/cache foundation.
- Issue #395 / Draft #396 — immutable bundled-resource admission.
- Issue #397 / Draft #398 — fixed-resource Tauri presentation IPC.
- Issue #406 / Draft #407 — native SQLite immutable-release persistence and exact lookup.
- SQLite. (2026). *STRICT tables*. https://www.sqlite.org/stricttables.html
- SQLite. (2026). *SQLite foreign key support*. https://www.sqlite.org/foreignkeys.html
- SQLite. (2026). *PRAGMA statements supported by SQLite*. https://www.sqlite.org/pragma.html
- Tauri Programme. (2026). *Embedding additional files*. https://v2.tauri.app/develop/resources/
- Tauri Programme. (2026). *Inter-process communication*. https://v2.tauri.app/concept/inter-process-communication/
- Tauri Programme. (2026). *Runtime authority*. https://v2.tauri.app/security/runtime-authority/
- Tauri Programme. (2026). *Capabilities*. https://v2.tauri.app/security/capabilities/
- Phillips, A., & Davis, M. (Eds.). (2009). *Tags for identifying languages* (BCP 47, RFC 5646). RFC Editor. https://www.rfc-editor.org/rfc/rfc5646.html
