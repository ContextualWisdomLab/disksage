# Age-Based Rule Predicates Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `min_age_days`/`max_age_days` predicates to user classification rules by plumbing file mtime through `FileEntry` and threading a `now_ms` clock (as a parameter) into the pure planner.

**Architecture:** `FileEntry` gains `mtime_ms`; `collect_files` fills it. `RuleMatch` gains two age fields; `classify_by_rules` takes `age_days`. `plan_moves_with` takes `now_ms`, computes per-file `age_days`, and passes it down. The single clock read lives in the `plan_organize` command; every pure function stays deterministic.

**Tech Stack:** Rust, existing `serde`/`serde_json`/std (NO new deps), Svelte 5 (type-only change).

## Global Constraints

- **Spec:** `docs/superpowers/specs/2026-07-13-age-based-rule-predicates-design.md`.
- **Purity for coverage:** no `SystemTime::now()` inside a coverage-measured pure function. `now_ms` is a parameter (tests pass a fixed value); the one real clock read is `now_ms()` in `plan_organize` (`#[cfg(not(coverage))]`).
- **Precedence/semantics unchanged:** user rule → picker → classify → skip; first-match-wins; AND over present predicates; `#[serde(deny_unknown_fields)]` (the two new optional fields extend it — a typo still `Err`s).
- **Age formula:** `age_days = now_ms.saturating_sub(mtime_ms) / 86_400_000`. `min_age_days` inclusive lower bound, `max_age_days` inclusive upper bound.
- **100% line coverage** on pure logic (`dupes.rs`, `userrules.rs`, `organize.rs`). Verify with DEFAULT `cargo llvm-cov --lib --summary-only` (NOT `--no-cfg-coverage`), from `src-tauri`. Keep every new line executed by a happy-path test (mtime extraction as a compact combinator/helper so the line gate — not region — is satisfied; see the codebase's region≠line convention).
- **NO new dependencies.** Determinism: fixed `now_ms` in all tests.

## File Structure

- `src-tauri/src/dupes.rs` — MODIFY. `FileEntry { …, mtime_ms }`; `collect_files` fills it; construction sites updated.
- `src-tauri/src/userrules.rs` — MODIFY. `RuleMatch` age fields; `classify_by_rules(…, age_days)`; `rule_matches` age checks.
- `src-tauri/src/organize.rs` — MODIFY. `plan_moves_with(…, now_ms, …)`; compute `age_days`; `plan_moves` passes `now_ms = 0`.
- `src-tauri/src/commands.rs` — MODIFY. `plan_organize` passes `now_ms()`.
- `src/lib/api.ts` — MODIFY. `RuleMatch` interface gains `min_age_days`/`max_age_days`.

---

## Task 1: FileEntry.mtime_ms + collect_files fill + construction sites

**Files:** Modify `src-tauri/src/dupes.rs`, `src-tauri/src/inventory.rs`, `src-tauri/src/organize.rs`.

**Interfaces:**
- Produces: `FileEntry { pub path: PathBuf, pub size: u64, pub mtime_ms: u64 }`; `collect_files` fills `mtime_ms` (epoch millis, `0` when unavailable).

- [ ] **Step 1: Add the field + fill + a coverage helper.** In `dupes.rs`, extend `FileEntry`:
```rust
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub size: u64,
    pub mtime_ms: u64,
}
```
Add a small helper (fully line-covered by a real-file test) + use it in `collect_files`:
```rust
/// Metadata의 수정시각 → epoch millis. 지원 안 되면 0 (플랫폼별 실패는 드묾; 0 폴백).
fn mtime_millis(md: &std::fs::Metadata) -> u64 {
    md.modified().ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
```
Change `collect_files`'s final `filter_map` (currently `…map(|md| FileEntry { path: e.path(), size: md.len() })`) to:
```rust
        .filter_map(|e| e.metadata().ok().map(|md| FileEntry { path: e.path(), size: md.len(), mtime_ms: mtime_millis(&md) }))
```

- [ ] **Step 2: Update every `FileEntry { … }` literal in `dupes.rs` tests** — add `mtime_ms: 0`. The `fe` helper (`dupes.rs:133`):
```rust
    fn fe(p: &str, size: u64) -> FileEntry {
        FileEntry { path: PathBuf::from(p), size, mtime_ms: 0 }
    }
```
And each direct literal in the `find_duplicates`/`hash_failures`/`groups_sorted_by_wasted_space` tests (`FileEntry { path: X, size: Y }` → `FileEntry { path: X, size: Y, mtime_ms: 0 }`) — there are several (in `end_to_end_finds_true_duplicates_only`, `groups_sorted_by_wasted_space_desc`, `same_size_different_prefix_drops_at_prefix_stage`, `hash_failures_are_skipped_not_fatal`). Add a test asserting the fill works:
```rust
    #[test]
    fn collect_files_populates_mtime() {
        let tmp = tempfile::tempdir().unwrap();
        write_file(tmp.path(), "x.bin", b"data");
        let files = collect_files(tmp.path());
        assert!(files.iter().any(|f| f.mtime_ms > 0), "mtime_ms filled for a real file");
    }
```

- [ ] **Step 3: Update `fe` helpers in `inventory.rs` and `organize.rs` tests.** Both have `fn fe(p: &str, size: u64) -> FileEntry { FileEntry { path: PathBuf::from(p), size } }` → add `, mtime_ms: 0`. (Their test sites call `fe()`, so no other test changes.) Grep for any other `FileEntry {` literal in these files and add `mtime_ms: 0`.

- [ ] **Step 4: Run — PASS** `cargo test --lib` (from `src-tauri`); `cargo llvm-cov --lib --summary-only` → `dupes.rs` **100%** (the `mtime_millis` line + fill covered by `collect_files_populates_mtime`; `inventory.rs`/`organize.rs` unchanged %). `cargo build --lib`.

- [ ] **Step 5: Commit** `git commit -m "feat(dupes): FileEntry.mtime_ms filled at scan time"`.

---

## Task 2: RuleMatch age fields + age-aware matcher

**Files:** Modify `src-tauri/src/userrules.rs`, `src/lib/api.ts`.

**Interfaces:**
- Consumes: nothing new.
- Produces: `RuleMatch` gains `min_age_days`/`max_age_days: Option<u64>`; `classify_by_rules(rules, path, size, age_days: u64) -> Option<String>` (new `age_days` param); `rule_matches` gains age checks.

- [ ] **Step 1: Write the failing tests** — the existing `classify_by_rules` calls need a new arg; update them and add age tests. Every existing `classify_by_rules(&r, path, size)` call in `userrules.rs` tests becomes `classify_by_rules(&r, path, size, 0)` (age 0 — existing non-age tests unaffected since they set no age predicate). Add:
```rust
    #[test]
    fn age_bounds_inclusive() {
        let r = vec![Rule { r#match: RuleMatch { min_age_days: Some(30), max_age_days: Some(90), ..m() }, class: "Stale".into() }];
        assert_eq!(classify_by_rules(&r, &PathBuf::from("/x"), 0, 30).as_deref(), Some("Stale")); // 하한 포함
        assert_eq!(classify_by_rules(&r, &PathBuf::from("/x"), 0, 90).as_deref(), Some("Stale")); // 상한 포함
        assert_eq!(classify_by_rules(&r, &PathBuf::from("/x"), 0, 29), None); // 하한 미만
        assert_eq!(classify_by_rules(&r, &PathBuf::from("/x"), 0, 91), None); // 상한 초과
    }
    #[test]
    fn age_ands_with_other_predicates() {
        let r = vec![Rule { r#match: RuleMatch { ext: Some("iso".into()), min_age_days: Some(365), ..m() }, class: "OldIso".into() }];
        assert_eq!(classify_by_rules(&r, &PathBuf::from("/x.iso"), 0, 400).as_deref(), Some("OldIso"));
        assert_eq!(classify_by_rules(&r, &PathBuf::from("/x.iso"), 0, 100), None); // ext OK, age 미달 → AND 실패
    }
```
Update the `m()` helper to include the two new fields as `None`:
```rust
    fn m() -> RuleMatch { RuleMatch { ext: None, name_contains: None, path_contains: None, min_size: None, max_size: None, min_age_days: None, max_age_days: None } }
```
(Every other place that builds a `RuleMatch { … }` literal in `userrules.rs` tests must also add `min_age_days: None, max_age_days: None` OR use `..m()`. Prefer `..m()` where the test already does.)

- [ ] **Step 2: Run — FAIL** `cargo test --lib userrules` (arity mismatch on `classify_by_rules` / missing fields).

- [ ] **Step 3: Implement.** Extend `RuleMatch`:
```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleMatch {
    #[serde(default)] pub ext: Option<String>,
    #[serde(default)] pub name_contains: Option<String>,
    #[serde(default)] pub path_contains: Option<String>,
    #[serde(default)] pub min_size: Option<u64>,
    #[serde(default)] pub max_size: Option<u64>,
    #[serde(default)] pub min_age_days: Option<u64>,
    #[serde(default)] pub max_age_days: Option<u64>,
}
```
Thread `age_days` through:
```rust
pub fn classify_by_rules(rules: &[Rule], path: &Path, size: u64, age_days: u64) -> Option<String> {
    rules.iter().find(|r| rule_matches(&r.r#match, path, size, age_days)).map(|r| r.class.clone())
}

fn rule_matches(m: &RuleMatch, path: &Path, size: u64, age_days: u64) -> bool {
    if let Some(ext) = &m.ext {
        let want = ext.to_lowercase();
        let got = path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase());
        if got.as_deref() != Some(want.as_str()) { return false; }
    }
    if let Some(sub) = &m.name_contains {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.contains(sub.as_str()) { return false; }
    }
    if let Some(sub) = &m.path_contains {
        if !path.to_string_lossy().contains(sub.as_str()) { return false; }
    }
    if let Some(min) = m.min_size { if size < min { return false; } }
    if let Some(max) = m.max_size { if size > max { return false; } }
    if let Some(min) = m.min_age_days { if age_days < min { return false; } }
    if let Some(max) = m.max_age_days { if age_days > max { return false; } }
    true
}
```

- [ ] **Step 4: Run — PASS** `cargo test --lib userrules` then `cargo test --lib`. `cargo llvm-cov --lib --summary-only` → `userrules.rs` **100%** (both new age arms covered by `age_bounds_inclusive`).

- [ ] **Step 5: api.ts** — extend the `RuleMatch` interface:
```ts
export interface RuleMatch {
  ext: string | null;
  name_contains: string | null;
  path_contains: string | null;
  min_size: number | null;
  max_size: number | null;
  min_age_days: number | null;
  max_age_days: number | null;
}
```
Run `npm run check` → 0 errors. (`api.test.ts` unchanged — `getUserRules` already covered; the type is compile-checked.)

- [ ] **Step 6: Commit** `git commit -m "feat(userrules): min_age_days/max_age_days predicates"`.

---

## Task 3: Thread now_ms → age into the planner

**Files:** Modify `src-tauri/src/organize.rs`, `src-tauri/src/commands.rs`.

**Interfaces:**
- Consumes: `classify_by_rules(…, age_days)` (Task 2), `FileEntry.mtime_ms` (Task 1).
- Produces: `plan_moves_with(files, onto, home, now_ms: u64, rules, pick)` (new `now_ms` before `rules`); `plan_moves` passes `now_ms = 0`.

- [ ] **Step 1: Update the failing tests.** The 3 direct `plan_moves_with(&…, &rules_or_[], &pick)` callers in `organize.rs` tests (`user_rule_overrides_picker_and_extension`, `no_user_rule_match_falls_through_to_picker`, `picker_choice_overrides_extension_classify`, `picker_none_falls_back_to_extension_classify`, `picker_candidates_include_ontology_class_names`) add a `now_ms` arg (use `0`) BEFORE the rules/pick args. Add a `fe_at` helper + an age test:
```rust
    fn fe_at(p: &str, size: u64, mtime_ms: u64) -> FileEntry {
        FileEntry { path: PathBuf::from(p), size, mtime_ms }
    }

    #[test]
    fn user_rule_age_predicate_matches_old_file_only() {
        // now = 100 days in ms; rule: min_age_days 30 → Installer. Old file (mtime 0 → age 100d) matches; fresh (mtime≈now → age 0) doesn't.
        let onto = parse_ttl(ONTO).unwrap();
        let home = Path::new("/home/u");
        let now = 100 * 86_400_000u64;
        let rules = vec![crate::userrules::Rule {
            r#match: crate::userrules::RuleMatch { ext: None, name_contains: None, path_contains: None, min_size: None, max_size: None, min_age_days: Some(30), max_age_days: None },
            class: "Installer".into(),
        }];
        let pick = |_p: &Path, _c: &[&str]| None;
        // old file → age 100d ≥ 30 → rule matches → Installer target
        let old = plan_moves_with(&[fe_at("/d/pic.png", 10, 0)], &onto, home, now, &rules, &pick);
        assert_eq!(old.len(), 1);
        assert!(old[0].class_id.ends_with("Installer"));
        // fresh file → age 0 < 30 → rule skips → extension classify (png→Image)
        let fresh = plan_moves_with(&[fe_at("/d/pic.png", 10, now)], &onto, home, now, &rules, &pick);
        assert_eq!(fresh.len(), 1);
        assert!(fresh[0].class_id.ends_with("Image"));
    }
```

- [ ] **Step 2: Run — FAIL** `cargo test --lib organize` (arity mismatch on `plan_moves_with`).

- [ ] **Step 3: Implement.** Add `now_ms` param + per-file age:
```rust
pub fn plan_moves_with(
    files: &[FileEntry],
    onto: &Ontology,
    home: &Path,
    now_ms: u64,
    rules: &[crate::userrules::Rule],
    pick: &dyn Fn(&Path, &[&str]) -> Option<String>,
) -> Vec<MovePlan> {
    let candidates: Vec<&str> = onto.classes.iter().map(|c| local_name(&c.id)).collect();
    let reasoner = crate::ontology::Reasoner::build(onto);
    let mut plans = Vec::new();
    for f in files {
        let Some(name) = f.path.file_name() else { continue };
        let age_days = now_ms.saturating_sub(f.mtime_ms) / 86_400_000;
        // precedence: 사용자 규칙 → picker(LLM) → 확장자 classify → 제외
        let local: String = match crate::userrules::classify_by_rules(rules, &f.path, f.size, age_days) {
            Some(c) => c,
            None => match pick(&f.path, &candidates) {
                Some(picked) => picked,
                None => match classify(&f.path) {
                    Some(c) => c.to_string(),
                    None => continue,
                },
            },
        };
        let Some(class) = onto.classes.iter().find(|c| local_name(&c.id) == local) else { continue };
        let Some(template) = onto.resolve_target_with(&reasoner, &class.id) else { continue };
        let folder = template.replacen('~', &home.to_string_lossy(), 1).replace("{class}", &local);
        let dst = Path::new(&folder).join(name);
        if f.path.parent() == Some(Path::new(&folder)) { continue; }
        plans.push(MovePlan {
            src: f.path.to_string_lossy().into_owned(),
            dst: dst.to_string_lossy().into_owned(),
            class_id: class.id.clone(),
        });
    }
    plans
}
```
Update `plan_moves` to pass `now_ms = 0`:
```rust
pub fn plan_moves(files: &[FileEntry], onto: &Ontology, home: &Path) -> Vec<MovePlan> {
    plan_moves_with(files, onto, home, 0, &[], &|_, _| None)
}
```

- [ ] **Step 4: `plan_organize` passes the real clock.** In `commands.rs::plan_organize`, both `plan_moves_with(&files, &onto, &home, &rules, &pick)` call sites (the `#[cfg(feature="llm-engine")]` block + the fallback) gain `now_ms()` before `&rules`: `plan_moves_with(&files, &onto, &home, now_ms(), &rules, &pick)` and `…, now_ms(), &rules, &|_, _| None)`. `now_ms()` (`commands.rs`) already exists. (The feature block is CI-verified, not locally compiled.)

- [ ] **Step 5: Run — PASS** `cargo test --lib organize` then `cargo test --lib`. `cargo llvm-cov --lib --summary-only` → `organize.rs` **100%** (the `age_days` line covered by all tests; the age-match branch by `user_rule_age_predicate_matches_old_file_only`). `cargo build --lib`; `npm run check` + `npm run build` clean.

- [ ] **Step 6: Commit** `git commit -m "feat(organize): age-aware rule matching via now_ms clock injection"`.

---

## Post-implementation

- Whole-branch review: purity (no clock in covered pure fns — `now_ms` is a param), age formula + inclusive bounds, `deny_unknown_fields` still rejects typos with the two new fields, precedence/AND unchanged, 100% coverage, no new dep, safety layer untouched.
- Open the PR on `feat/age-rules`. **api.ts changed → confirm `api.test.ts` already covers `getUserRules` (it does — only the `RuleMatch` type grew, no new wrapper), so the JS coverage-evidence gate passes** (see CI memory: changed `.ts` must be instrumented; `getUserRules` case already present).
