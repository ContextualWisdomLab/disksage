# Age-Based Rule Predicates — Design (sub-project A-v2)

**Status:** Draft for review. Follow-up to sub-project A (user-defined rule classification, [PR #13](https://github.com/ContextualWisdomLab/disksage/pull/13)), which explicitly deferred date/age predicates to v2 (A spec §10: "no date/age predicates in v1 — mtime isn't on `FileEntry`"). **Depends on #13 being merged** (builds on `userrules.rs`).

## 1. Goal

Add `min_age_days` / `max_age_days` predicates to user classification rules, so a rule can match on how old a file is — "anything in `Downloads` older than 365 days → Archive", "installers older than 90 days → StaleInstallers". File **age** is the central axis of disk cleanup; the app already reads mtime (it flows to the LLM as `mtime_days`) but no rule or view can yet act on it. This closes that gap in the deterministic, user-controlled rule layer.

## 2. Non-goals (YAGNI)

- No absolute date matching (`before 2024-01-01`) — relative age (days) covers the cleanup use case without date parsing.
- No age dimension in the Inventory view or an LLM age signal — this is scoped to the **rule** predicate only. (An inventory "stale files" breakdown is a clean future feature on the same `FileEntry.mtime` plumbing.)
- No change to the safety layer — rules remain advisory over the preview classification only.

## 3. Constraints & invariants (inherited from A)

- **Advisory/preview-only**; safety layer untouched.
- **Precedence unchanged:** user rule → LLM picker → extension classify → skip; first-match-wins; AND semantics over present predicates.
- **Malformed/unknown keys → `Err`** (`#[serde(deny_unknown_fields)]` already on `RuleMatch`; the two new optional fields extend it).
- **Determinism + 100% coverage** on pure logic. **The age computation must stay pure/testable** — no `SystemTime::now()` inside a covered pure function (see §4 Fork).
- **NO new dependencies.**

## 4. Design decisions (alternatives considered)

**Fork 1 — where "now" enters (keeping purity).** Age = `(now − mtime)`. `classify_by_rules` and `plan_moves_with` are pure, 100%-covered functions; injecting a wall clock breaks that. Options: (a) store precomputed `age_days` in `FileEntry` (clock used once at scan time in `collect_files`); (b) store raw `mtime_ms` in `FileEntry` and thread a `now_ms: u64` parameter through `plan_moves_with` → `classify_by_rules`. **Chosen (b):** `FileEntry` stays pure data (raw mtime, no embedded clock/derived value that goes stale), and `now_ms` as an explicit parameter keeps every function deterministic — tests pass a fixed `now_ms`, so age boundaries are exactly assertable. `collect_files` (already impure I/O) fills `mtime_ms`; the single real clock read lives in the `plan_organize` command (`#[cfg(not(coverage))]`), which passes `now_ms()` (the existing helper) down.

**Fork 2 — `FileEntry` field.** `FileEntry { path, size }` → add `mtime_ms: u64` (epoch millis, `0` when unavailable — same fallback as `meta_items`/`now_ms`). Raw millis (not days) keeps the type free of a clock-derived value; age-in-days is computed at match time. Every `FileEntry` construction site (dupes/inventory/organize test helpers) gains the field.

**Fork 3 — age semantics.** `age_days = now_ms.saturating_sub(mtime_ms) / 86_400_000` (same formula as `meta_items`). `min_age_days` = file must be **at least** this old (inclusive); `max_age_days` = **at most** this old (inclusive). A future-dated file (mtime > now) → `age_days = 0` via `saturating_sub`.

## 5. Architecture & components

- **`src-tauri/src/dupes.rs`** (MODIFY): `FileEntry` gains `pub mtime_ms: u64`; `collect_files` fills it from the `metadata()` it already calls for size (`modified()` → duration since epoch → millis; `0` on error). Existing `FileEntry { path, size }` literals in tests updated.
- **`src-tauri/src/userrules.rs`** (MODIFY): `RuleMatch` gains `#[serde(default)] pub min_age_days: Option<u64>` and `max_age_days: Option<u64>`. `classify_by_rules` signature gains `age_days: u64` (`classify_by_rules(rules, path, size, age_days)`); `rule_matches` gains the two inclusive age checks. Pure, 100%-covered (boundary tests).
- **`src-tauri/src/organize.rs`** (MODIFY): `plan_moves_with` gains `now_ms: u64` (before `rules`); per file computes `age_days = now_ms.saturating_sub(f.mtime_ms) / 86_400_000` and passes it to `classify_by_rules`. `plan_moves` passes `now_ms = 0` (age predicates then never match a positive `min_age_days`, and `max_age_days` treats everything as age 0 — acceptable for the extension-only helper; documented). Direct-caller tests gain the `now_ms` arg.
- **`src-tauri/src/commands.rs`** (MODIFY): `plan_organize` passes `now_ms()` to `plan_moves_with`.
- **`src/lib/api.ts`** (MODIFY): `RuleMatch` interface gains `min_age_days: number | null; max_age_days: number | null;`. (No new command; `api.test.ts` unchanged — `getUserRules` already covered.)
- No frontend behavior change beyond the type (rules are edited in the JSON file).

## 6. Data flow

```
scan → collect_files: FileEntry{path,size,mtime_ms}
plan_organize → now_ms() ─┐
plan_moves_with(files, onto, home, now_ms, rules, pick):
  per file: age_days = (now_ms − f.mtime_ms)/86_400_000
            classify_by_rules(rules, &f.path, f.size, age_days)  ← age predicates checked here
            → class → resolve_target → dst
```

## 7. Testing & coverage

- **`userrules.rs` (pure, 100%):** `min_age_days` boundary (age == min passes, age == min−1 fails), `max_age_days` boundary (age == max passes, max+1 fails), age combined AND with ext/size (one fails → no match), and the `saturating_sub` future-date (mtime > now → age 0) via the `plan_moves_with` layer.
- **`organize.rs` (pure, 100%):** a rule with `min_age_days` that matches an old file (fixed `now_ms`, small `mtime_ms`) and does not match a fresh file (`mtime_ms ≈ now_ms`); the `now_ms = 0` path via `plan_moves` (existing tests, extended `FileEntry`).
- **`dupes.rs`:** `collect_files` populates `mtime_ms` for a real temp file (> 0); the `0`-on-error fallback is exercised by the existing missing/metadata-error tests if any, else a targeted case.
- **api.ts:** `RuleMatch` type extension is compile-checked; `getUserRules` coverage already in `api.test.ts`.
- No new dependency; determinism preserved (fixed `now_ms` in all tests).

## 8. Decomposition (for the plan)

1. `FileEntry.mtime_ms` + `collect_files` fill + construction-site updates (`dupes.rs`).
2. `RuleMatch` age fields + `classify_by_rules(age_days)` + `rule_matches` age checks (`userrules.rs`) + api.ts type.
3. `plan_moves_with(now_ms, …)` threading + `plan_organize` `now_ms()` + test updates (`organize.rs`, `commands.rs`).

## 9. Follow-ups (out of scope)

- Inventory "stale files" / age-bucket breakdown (reuses `FileEntry.mtime_ms`).
- Absolute-date predicates; glob/regex predicates (A spec §10).
