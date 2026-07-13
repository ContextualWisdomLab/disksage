# LLM Advanced Reasoning + Opt-in Web Enrichment — Design (sub-project C)

**Status:** Draft for review. Sub-project C of "advanced reasoning" (B = OWL subsumption reasoner, merged as PR #11; A = rule-based classification, deferred). The user selected **B + C with web search**, opt-in online mode (default offline).

## 1. Goal

Make **unknown files actionable** — the app's core pain point ("파일이 뭐가 있는지 모르겠다"). Today an unknown file (extension not in the static `classify()` table, or class absent from the ontology) lands in an "Unknown" bucket with only a count + a whole-bucket LLM summary. This sub-project adds **per-file-type reasoning**: for each distinct unknown extension, the local LLM proposes *what kind of file it is* and *which ontology class (if any) fits* — and, **only when the user opts into online mode**, enriches that with an anonymous web lookup of the file-type.

Advisory only: nothing here ever moves, deletes, or gates a file. It changes what the user *sees*, never what the app *does*.

## 2. Non-goals

- No reasoning over file **contents** — extension tokens only (see §3).
- No general web search of filenames, paths, or user data — only the bare extension.
- No always-on network. Default is 100% offline; web enrichment is strictly opt-in.
- No new inference framework — extend the existing `InferenceEngine` + prompt pattern.
- No auto-classification/auto-move from reasoning — suggestions are advisory, surfaced in the Unknown view.
- Sub-project **A** (user-defined rule-based classification) is separate; noted as a follow-up in §11.

## 3. Privacy & safety invariants (the hard constraints — these govern every decision)

These are the user's explicit conditions and the existing codebase's structural guarantees. A regression on any is a design failure.

1. **File contents never leave the machine** — not to the LLM (local, in-process), not to the web. Enforced by construction: the LLM receives `FileMeta` (no content field); the web layer receives only a normalized extension token.
2. **Only the bare extension token reaches the web** — e.g. `"fbx"`, `"parquet"`, `"dwg"`. Never the filename, path, size, mtime, or any composite that could identify the user or their data. An extension is a generic, public, non-identifying token.
3. **Default offline.** `online_mode` defaults to `false`. With it off, **no web request is ever issued** — the offline LLM reasoning still works.
4. **Anonymous.** Web requests carry a generic User-Agent (`DiskSage/<version>`), no cookies, no API key, no identifiers, no telemetry. The chosen backend (DuckDuckGo Instant Answer) is keyless and privacy-respecting by design.
5. **Advisory only.** Reasoning/enrichment output is display-only. The existing safety layer (trash-only deletion, `is_protected`, journaled moves, user confirmation) is untouched and remains the sole authority over any filesystem mutation.

## 4. Design decisions (alternatives considered)

**Fork 1 — Offline-only vs. offline + opt-in web.** An offline extension→type dataset (or the LLM's own world knowledge) resolves most common extensions with zero privacy surface. But the user explicitly requested web search for the long tail of obscure formats. **Chosen:** offline LLM reasoning as the always-on default; opt-in web enrichment as the escalation. This honors "web search included" while keeping the default fully offline and making the network path opt-in and rarely needed.

**Fork 2 — Web backend.** (a) A general search API (Google/Bing) needs an API key and sends queries to an ad-tech provider — fails the anonymity intent. (b) A bundled static extension DB is offline but never updates and bloats the binary. (c) **DuckDuckGo Instant Answer API** (`api.duckduckgo.com`) is keyless, anonymous, privacy-focused, and returns a text `AbstractText` for a query. **Chosen:** DDG Instant Answer, behind a small `WebLookup` trait so the backend is swappable and unit-testable without network. Query shape: `?q=<ext>+file+format&format=json&no_html=1&no_redirect=1`.

**Fork 3 — Does the web result feed the LLM or stand alone?** Feeding web text back into the LLM adds latency and a second failure mode. **Chosen:** keep them **additive and independent** — the offline LLM produces (type-guess, suggested-class); the web lookup produces a (type-description) string. The UI shows both. No hard dependency between them; either can be `None` and the other still renders.

**Fork 4 — Granularity.** Reasoning per *file* would re-ask identical questions for every `.fbx`. **Chosen:** reason per **distinct unknown extension**, computed from the inventory's unknown samples. Bounded, cheap, cacheable.

## 5. Architecture & components

New/extended units (each with one responsibility, mirroring existing patterns):

- **`llm/prompt.rs`** (extend) — add `ext_reason_prompt(ext: &str, candidates: &[&str]) -> String`: forces JSON `{"type":"<short human type>","class":"<one of candidates or 'none'>"}`. Pure.
- **`llm/mod.rs`** (extend) — add `reason_extension(engine, ext, candidates) -> Option<ExtReasoning>` where `ExtReasoning { type_desc: String, class: Option<String> }`; `class` validated against `candidates` (reuse the `parse_class_pick` discipline — never free-generate a class). Fail-closed to `None`.
- **`llm/parse.rs`** (extend) — `parse_ext_reasoning(&str) -> Option<ExtReasoning>` (same balanced-brace + serde + fail-closed pattern).
- **`web/mod.rs`** (new) — `trait WebLookup { fn file_type(&self, ext: &str) -> Result<Option<String>, String>; }` + pure `ddg_query(ext) -> String` (query builder) and `parse_ddg_abstract(json: &str) -> Option<String>` (extracts `AbstractText`, empty→`None`). These pure pieces are 100%-covered.
- **`web/ureq_lookup.rs`** (new, `#[cfg(not(coverage))]`) — `struct DdgLookup` implementing `WebLookup` via `ureq` with the anonymous UA + timeout; the only network egress. Mirrors `model.rs::download_to`'s gating.
- **`settings.rs`** (new) — `Settings { online_mode: bool }` (serde, `Default` = `{ online_mode: false }`); pure `parse_settings(&str) -> Settings` (malformed → default, since a corrupt settings file must never break the app) and `serialize_settings(&Settings) -> String`. 100%-covered.
- **`commands.rs`** (extend) — `get_settings`/`set_settings` (persist to `app_config_dir/settings.json`, mirroring `bundled_ontology_ttl`'s read + `journal` write patterns); `reason_unknown_extensions(app) -> Vec<ExtInsight>` where `ExtInsight { ext, type_desc: Option<String>, suggested_class: Option<String>, source: "llm"|"web"|"both"|"none" }`. Network/engine blocks `#[cfg(...)]`-gated as established.
- **`AppState`** (extend) — add `settings: Arc<Mutex<Settings>>` loaded at startup.
- **Frontend** — `api.ts`: `Settings`, `ExtInsight`, `getSettings`/`setSettings`/`reasonUnknownExtensions`. A small **Settings** surface (toggle for online mode, mirroring the model-download capability-gating UI). `Inventory.svelte`: in the Unknown section, list distinct unknown extensions with the LLM type-guess + suggested class, and (when online) the web type-description. Advisory styling like the coherence warnings.

## 6. Data flow

```
Inventory.unknown_samples ──(distinct extensions)──► reason_unknown_extensions
   for each ext:
     offline: llm.reason_extension(ext, ontology candidates) ──► (type_desc?, class?)
     if settings.online_mode:                                  ┐ opt-in only
        web.file_type(ext)  ──(DDG, ext-token only)──► type_desc' ┘
     merge ──► ExtInsight { ext, type_desc (web preferred if present), suggested_class, source }
Inventory.svelte ──renders advisory list under "Unknown"
```

With `online_mode=false`, the web branch is unreachable — the insight is LLM-only or empty.

## 7. Error handling & degradation

- LLM feature off / model absent / inference error → `reason_extension` returns `None` (existing `Unrated`-style degradation). The extension still lists with `source:"none"` or web-only.
- Web disabled or request fails/times out → web branch yields `None`; offline result stands. Never surfaces a hard error to the user; a failed lookup is silently `None` (advisory feature).
- Corrupt `settings.json` → parsed as default (`online_mode:false`) — fail-safe toward offline.
- All degradation paths keep the app fully functional; this feature only ever *adds* advisory hints.

## 8. Testing & coverage

- **Pure, 100%-covered (Linux gate):** `ext_reason_prompt`, `parse_ext_reasoning` (valid / malformed / class-outside-candidates→none / empty), `ddg_query`, `parse_ddg_abstract` (present / empty-abstract→None / malformed→None), `parse_settings`/`serialize_settings` (default / round-trip / corrupt→default), `reason_extension` against a fake `InferenceEngine`, and the merge logic in `reason_unknown_extensions`'s pure inner against a fake `WebLookup` (offline-only vs both-sources vs none).
- **`#[cfg(not(coverage))]` (typecheck-only in CI, like the engine/download):** `DdgLookup` (real `ureq`), the Tauri command wrappers, settings file IO.
- **Privacy assertions as tests:** a test that `ddg_query` contains only the ext token (asserts the built query has no filename/path); a test that with `online_mode:false` the merge path never invokes the `WebLookup` (fake lookup panics if called).
- Frontend: `vitest` for any pure TS helper (e.g. insight badge formatting).
- No new dependency: `ureq` already present; `serde`/`serde_json` already present.

## 9. Rollout / integration

Additive. No existing command changes signature. The Unknown view gains an advisory sub-list; everything else is unchanged. Ships behind the existing `llm-engine` feature for the LLM part; the web part is independent of that feature and gated only by `online_mode` + `#[cfg(not(coverage))]`.

## 10. Decomposition (for the implementation plan)

1. Settings core + persistence + commands + AppState (offline default).
2. Offline LLM extension reasoning (prompt + parse + `reason_extension`).
3. Web lookup layer (`WebLookup` trait + pure DDG query/parse + gated `ureq` impl).
4. `reason_unknown_extensions` merge command + Inventory/Settings frontend surfacing.

## 11. Follow-ups (out of scope)

- Sub-project **A** — user-defined rule-based classification (regex/size/date predicates) hooking the `plan_moves_with` `pick` seam, with a `rules.toml` override mirroring the ontology-TTL precedent. Requires plumbing mtime into `FileEntry`. Separate spec→plan→implement cycle.
- Caching web results across sessions (currently session-only would suffice; persistent cache is a later optimization).
