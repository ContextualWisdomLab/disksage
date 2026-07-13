# User-Defined Rule-Based Classification — Design (sub-project A)

**Status:** Draft for review. Sub-project A of "advanced reasoning" (B = OWL subsumption reasoner, merged PR #11; C = LLM advanced reasoning + opt-in web, PR #12). This is the deterministic, user-controlled classification layer that sits *above* the LLM picker and the static extension table.

## 1. Goal

Let a user steer classification with explicit rules — "anything under `Downloads` ending `.iso` is an Installer", "files over 1 GB named `backup` are Archives" — without editing code. Rules live in a `userrules.json` the user drops in the app config dir (the same override mechanism as `ontology.ttl`), and they take **precedence over** the LLM picker and the static extension `classify()`. This makes the reorganization preview predictable and tailorable, complementing the probabilistic LLM path (sub-project C) and the extension heuristic.

## 2. Non-goals (YAGNI — deferred to a v2 if asked)

- **Glob/regex patterns.** Substring (`name_contains`/`path_contains`) + `ext` cover the overwhelming majority of intent without a glob parser or a regex dependency. (`ext: "iso"` == `*.iso`; `name_contains: "backup"` == `*backup*`.)
- **Date/age predicates.** `FileEntry { path, size }` carries no mtime; plumbing it touches a widely-used type (dupes/inventory/organize). Deferred — v1 predicates are extension/name/path/size only.
- **A rule-editing GUI.** Rules are edited in the JSON file, exactly like the ontology TTL. The UI only *surfaces* the active rule count (advisory), it does not edit.
- **Rule actions other than "assign class."** A rule maps a file to an ontology class; the existing reasoner then resolves the destination folder. No delete/tag/custom-action.

## 3. Constraints & invariants

- **Advisory/safe:** rules only influence the *classification* step of the reorganization **preview**. They never move or delete anything directly — the existing safety layer (trash-only, `is_protected`, journaled moves, user confirmation) is the sole authority over the filesystem, unchanged.
- **Precedence (deterministic):** `user rule` → `LLM picker` (sub-project C, if enabled) → `extension classify()` → skip. First matching user rule wins (source order preserved).
- **Config precedent:** `userrules.json` in `app_config_dir`, mirroring `bundled_ontology_ttl`. Absent file → empty rule set (no-op, the existing behavior is unchanged). **Malformed** file → surfaced error (like a malformed ontology override — the user is told their rules file is broken rather than having it silently ignored).
- **No new dependency:** `serde`/`serde_json` already present; substring/size matching is std. No glob/regex/toml crate.
- **100% line coverage** on all pure logic (`userrules.rs`, the extended `plan_moves_with`). FFI/command/IO stays `#[cfg(not(coverage))]`. Determinism preserved (rule order is source order; no HashMap).
- **A class a rule names that isn't in the active ontology** simply fails to resolve a target folder downstream (existing `resolve_target` → `None` → file skipped) — same graceful behavior as any other unresolved class. No hard validation needed.

## 4. Design decisions (alternatives considered)

**Fork 1 — where rules hook in.** (a) Change the `pick` closure signature to add `size`; (b) add a `rules: &[Rule]` parameter to `plan_moves_with` and apply it first in the loop (where `f.size` is already in scope). **Chosen (b):** rules need `size`, which the loop has but the `pick(&Path, &[&str])` closure does not; threading a new closure signature through every call site (and the LLM/test closures) is more churn than one new slice parameter. `plan_moves` passes `&[]` (its 8 tests are unaffected); only `plan_moves_with`'s 3 direct-caller tests gain a `&[]` argument.

**Fork 2 — rule matching expressiveness.** Full glob/regex vs. substring+ext+size. **Chosen substring+ext+size:** no parser, no dependency, trivially 100%-coverable, and `ext`+`contains` express the real use cases. Glob is a clean v2 add (behind the same `RuleMatch` struct) if users ask.

**Fork 3 — malformed rules file: error vs ignore.** **Chosen error-surface** (consistent with the ontology override): a broken `userrules.json` makes `plan_organize` return `Err` (the UI shows it) rather than silently classifying as if no rules existed — the user edited that file on purpose.

## 5. Architecture & components

- **`src-tauri/src/userrules.rs`** (CREATE — named to avoid collision with the existing cache-catalog `rules.rs`):
  - `Rule { r#match: RuleMatch, class: String }`, `RuleMatch { ext: Option<String>, name_contains: Option<String>, path_contains: Option<String>, min_size: Option<u64>, max_size: Option<u64> }` (serde Deserialize/Serialize; all match fields optional, AND semantics over present ones; an all-`None` match is a catch-all).
  - `parse_rules(json: &str) -> Result<Vec<Rule>, String>` (serde; error string on malformed).
  - `classify_by_rules(rules: &[Rule], path: &Path, size: u64) -> Option<String>` — returns the `class` of the first rule whose every present predicate matches (`ext` case-insensitive on the file's extension; `name_contains` on the file name; `path_contains` on the full path string; `min_size`/`max_size` inclusive bounds on `size`). Pure, 100%-covered.
- **`src-tauri/src/organize.rs`** (MODIFY): `plan_moves_with` gains `rules: &[Rule]` (before `pick`); the per-file `local` resolution tries `classify_by_rules(rules, &f.path, f.size)` first, then the picker, then `classify()`. `plan_moves` passes `&[]`.
- **`src-tauri/src/commands.rs`** (MODIFY): `user_rules_json(app)` (override-or-empty, `#[cfg(not(coverage))]`, mirrors `bundled_ontology_ttl` but returns `""`/empty when absent); `plan_organize` loads + parses rules and passes them to `plan_moves_with`; a `user_rules(app) -> Result<Vec<Rule>, String>` command for the UI.
- **`src-tauri/src/lib.rs`** (MODIFY): `mod userrules;` + register `commands::user_rules`.
- **`src/lib/api.ts`** (MODIFY): `Rule`/`RuleMatch` types + `getUserRules()`.
- **`src/lib/Inventory.svelte`** (MODIFY): show an advisory "N active user rules" line (like the coherence indicator); non-blocking.

## 6. Data flow

```
userrules.json (app_config_dir, optional) ──parse_rules──► Vec<Rule>
plan_organize ── passes rules ──► plan_moves_with:
   per file: classify_by_rules(rules, path, size)   ← FIRST (user intent)
             .or_else(pick[LLM])                     ← sub-project C, if enabled
             .or_else(classify[extension])           ← static table
             → ontology class → resolve_target → dst
```

## 7. Error handling & degradation

- No `userrules.json` → `Vec::new()` → precedence is exactly today's (picker → classify). Zero behavior change for users without rules.
- Malformed `userrules.json` → `parse_rules` `Err` → `plan_organize` returns `Err` (UI surfaces it). The user fixes their file.
- A rule naming a non-existent class → file falls through `resolve_target` → skipped (existing behavior).

## 8. Testing & coverage

- **Pure, 100%-covered:** `parse_rules` (valid / malformed→Err / empty array); `classify_by_rules` — each predicate in isolation (ext match/mismatch, name_contains, path_contains, min/max size boundaries incl. inclusive edges), AND-combination (two predicates, one fails → no match), first-match-wins ordering, all-`None` catch-all, empty-rules→None. `plan_moves_with` with a rule that overrides the picker + a rule that overrides extension classify + `&[]` (unchanged path).
- **`#[cfg(not(coverage))]`:** `user_rules_json`/`user_rules` command, `plan_organize` wiring.
- Frontend: `vitest` for any pure helper; the Inventory rule-count line is trivial wiring.

## 9. Decomposition (for the plan)

1. `userrules.rs` — model + `parse_rules` + `classify_by_rules` (pure).
2. `organize.rs` integration (`plan_moves_with` rules param, precedence) + `plan_organize`/`user_rules_json` config loader.
3. `user_rules` command + api.ts + Inventory advisory rule-count surfacing.

## 10. Follow-ups (out of scope)

- Glob/regex predicates, date/age predicates (needs `FileEntry.mtime` plumbing), and a rule-editing UI — all clean v2 additions behind the same `RuleMatch`/command surface.
- This completes the three advanced-reasoning sub-projects (A rule-based, B OWL reasoner, C LLM + opt-in web).
