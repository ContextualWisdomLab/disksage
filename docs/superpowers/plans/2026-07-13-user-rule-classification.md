# User-Defined Rule-Based Classification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users assign files to ontology classes via `userrules.json` (extension/name/path/size predicates), taking precedence over the LLM picker and the static extension table, in the reorganization preview.

**Architecture:** New pure `userrules.rs` (rule model + parse + first-match matcher); `organize::plan_moves_with` gains a `rules` slice applied first in the per-file precedence; `plan_organize` loads the override file and threads rules through; a `user_rules` command + advisory rule-count in Inventory.

**Tech Stack:** Rust, existing `serde`/`serde_json` (NO new deps), Svelte 5.

## Global Constraints

- **Spec:** `docs/superpowers/specs/2026-07-13-user-rule-classification-design.md`.
- **Precedence (deterministic):** user rule → LLM picker → extension `classify()` → skip. First matching user rule wins (source order).
- **Advisory/safe:** rules affect only the classification step of the **preview**; the safety layer (trash-only, `is_protected`, journaled moves, confirmation) is untouched.
- **Config precedent:** `userrules.json` in `app_config_dir` (mirrors `bundled_ontology_ttl`). Absent → empty (`"[]"`), no behavior change. Malformed → surfaced `Err` (like a malformed ontology override).
- **NO new dependencies.** `serde`/`serde_json` only; substring/size matching is std.
- **100% line coverage** on pure logic (`userrules.rs`, extended `plan_moves_with`). Verify with DEFAULT `cargo llvm-cov --lib --summary-only` (do NOT pass `--no-cfg-coverage`), from `src-tauri`. Commands/IO stay `#[cfg(not(coverage))]`.
- **Naming:** the module is `userrules.rs` — the existing `rules.rs` is the unrelated cache-cleanup catalog.
- **Determinism:** rule order is source order; no `HashMap` in output.

## File Structure

- `src-tauri/src/userrules.rs` — CREATE. `Rule`/`RuleMatch`, `parse_rules`, `classify_by_rules` (pure).
- `src-tauri/src/organize.rs` — MODIFY. `plan_moves_with` gains `rules: &[Rule]`; per-file precedence puts rules first. `plan_moves` passes `&[]`.
- `src-tauri/src/commands.rs` — MODIFY. `user_rules_json` (override-or-`"[]"`), `plan_organize` threads rules, `user_rules` command.
- `src-tauri/src/lib.rs` — MODIFY. `mod userrules;` + register `commands::user_rules`.
- `src/lib/api.ts` — MODIFY. `Rule`/`RuleMatch` + `getUserRules()`.
- `src/lib/Inventory.svelte` — MODIFY. Advisory "N active user rules".

---

## Task 1: userrules.rs — model + parse + matcher

**Files:** Create `src-tauri/src/userrules.rs`; Modify `src-tauri/src/lib.rs` (`mod userrules;`).

**Interfaces:**
- Produces: `Rule { pub r#match: RuleMatch, pub class: String }`; `RuleMatch { ext, name_contains, path_contains: Option<String>, min_size, max_size: Option<u64> }` (serde, all match fields optional); `parse_rules(&str) -> Result<Vec<Rule>, String>`; `classify_by_rules(rules: &[Rule], path: &Path, size: u64) -> Option<String>`.

- [ ] **Step 1: Write the failing tests + impl** (inline-impl module; the "red" is the missing module). Create `src-tauri/src/userrules.rs`:

```rust
//! 사용자 정의 분류 규칙 — 확장자/이름/경로/크기 술어로 파일→온톨로지 클래스. 첫 매칭 규칙 승리.
//! 순수 로직(파싱/매칭)만 여기 — 파일 로드/커맨드는 commands.rs(cfg not coverage). 캐시 카탈로그 rules.rs와 무관.
use std::path::Path;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RuleMatch {
    #[serde(default)] pub ext: Option<String>,
    #[serde(default)] pub name_contains: Option<String>,
    #[serde(default)] pub path_contains: Option<String>,
    #[serde(default)] pub min_size: Option<u64>,
    #[serde(default)] pub max_size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Rule {
    pub r#match: RuleMatch,
    pub class: String,
}

/// JSON 배열 → 규칙들. 손상 JSON은 Err(사용자에게 알림 — 온톨로지 오버라이드와 동일 원칙).
pub fn parse_rules(json: &str) -> Result<Vec<Rule>, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

/// 첫 매칭 규칙의 클래스. 매칭 규칙 없으면 None.
pub fn classify_by_rules(rules: &[Rule], path: &Path, size: u64) -> Option<String> {
    rules.iter().find(|r| rule_matches(&r.r#match, path, size)).map(|r| r.class.clone())
}

/// 존재하는 모든 술어가 AND로 일치해야 매칭. 술어 전무(all-None)면 catch-all(true).
fn rule_matches(m: &RuleMatch, path: &Path, size: u64) -> bool {
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
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn m() -> RuleMatch { RuleMatch { ext: None, name_contains: None, path_contains: None, min_size: None, max_size: None } }

    #[test]
    fn parse_valid_and_malformed() {
        let json = r#"[{"match":{"ext":"iso"},"class":"Installer"}]"#;
        let rules = parse_rules(json).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].class, "Installer");
        assert_eq!(rules[0].r#match.ext.as_deref(), Some("iso"));
        assert!(parse_rules("not json").is_err());
        assert!(parse_rules("[]").unwrap().is_empty());
    }

    #[test]
    fn ext_predicate_case_insensitive() {
        let r = vec![Rule { r#match: RuleMatch { ext: Some("ISO".into()), ..m() }, class: "Installer".into() }];
        assert_eq!(classify_by_rules(&r, &PathBuf::from("/d/x.iso"), 0).as_deref(), Some("Installer"));
        assert_eq!(classify_by_rules(&r, &PathBuf::from("/d/x.zip"), 0), None); // 확장자 불일치
        assert_eq!(classify_by_rules(&r, &PathBuf::from("/d/noext"), 0), None); // 확장자 없음
    }

    #[test]
    fn name_and_path_contains() {
        let rn = vec![Rule { r#match: RuleMatch { name_contains: Some("backup".into()), ..m() }, class: "Archive".into() }];
        assert_eq!(classify_by_rules(&rn, &PathBuf::from("/d/my_backup.tar"), 0).as_deref(), Some("Archive"));
        assert_eq!(classify_by_rules(&rn, &PathBuf::from("/d/report.tar"), 0), None);
        assert_eq!(classify_by_rules(&rn, &PathBuf::from("/"), 0), None); // 파일명 없음 → "" → 불일치
        let rp = vec![Rule { r#match: RuleMatch { path_contains: Some("Downloads".into()), ..m() }, class: "Dl".into() }];
        assert_eq!(classify_by_rules(&rp, &PathBuf::from("/home/Downloads/x.bin"), 0).as_deref(), Some("Dl"));
        assert_eq!(classify_by_rules(&rp, &PathBuf::from("/home/Docs/x.bin"), 0), None);
    }

    #[test]
    fn size_bounds_inclusive() {
        let r = vec![Rule { r#match: RuleMatch { min_size: Some(100), max_size: Some(200), ..m() }, class: "Mid".into() }];
        assert_eq!(classify_by_rules(&r, &PathBuf::from("/x"), 100).as_deref(), Some("Mid")); // 하한 포함
        assert_eq!(classify_by_rules(&r, &PathBuf::from("/x"), 200).as_deref(), Some("Mid")); // 상한 포함
        assert_eq!(classify_by_rules(&r, &PathBuf::from("/x"), 99), None);  // 하한 미만
        assert_eq!(classify_by_rules(&r, &PathBuf::from("/x"), 201), None); // 상한 초과
    }

    #[test]
    fn and_semantics_and_first_match_wins_and_catch_all() {
        // AND: ext+min_size 둘 다 만족해야
        let r = vec![Rule { r#match: RuleMatch { ext: Some("mp4".into()), min_size: Some(1000), ..m() }, class: "BigVid".into() }];
        assert_eq!(classify_by_rules(&r, &PathBuf::from("/x.mp4"), 2000).as_deref(), Some("BigVid"));
        assert_eq!(classify_by_rules(&r, &PathBuf::from("/x.mp4"), 500), None); // ext OK, size 미달 → AND 실패
        // 첫 매칭 승리
        let ord = vec![
            Rule { r#match: RuleMatch { ext: Some("log".into()), ..m() }, class: "First".into() },
            Rule { r#match: RuleMatch { ext: Some("log".into()), ..m() }, class: "Second".into() },
        ];
        assert_eq!(classify_by_rules(&ord, &PathBuf::from("/x.log"), 0).as_deref(), Some("First"));
        // all-None catch-all
        let catch = vec![Rule { r#match: m(), class: "Any".into() }];
        assert_eq!(classify_by_rules(&catch, &PathBuf::from("/anything.zzz"), 0).as_deref(), Some("Any"));
        // 빈 규칙 → None
        assert_eq!(classify_by_rules(&[], &PathBuf::from("/x"), 0), None);
    }
}
```

- [ ] **Step 2: Register + run.** Add `mod userrules;` to `src-tauri/src/lib.rs` (near the other `mod` decls, e.g. after `mod scanner;`). Match the sibling `#[cfg_attr(coverage, allow(dead_code))]` if the neighbors use it. Run `cargo test --lib userrules` from `src-tauri` → passes.

- [ ] **Step 3: Coverage.** `cargo test --lib` (whole suite); `cargo llvm-cov --lib --summary-only` → `userrules.rs` **100%**. If a branch is <100% (e.g. an unhit predicate arm), add a minimal case.

- [ ] **Step 4: Commit** `git commit -m "feat(userrules): rule model + parse + first-match classifier"`.

---

## Task 2: organize.rs integration + config loader

**Files:** Modify `src-tauri/src/organize.rs`, `src-tauri/src/commands.rs`.

**Interfaces:**
- Consumes: `crate::userrules::{Rule, classify_by_rules, parse_rules}`.
- Produces: `plan_moves_with(files, onto, home, rules: &[Rule], pick) -> Vec<MovePlan>` (new `rules` param before `pick`); `user_rules_json(&AppHandle) -> String`.

- [ ] **Step 1: Update the failing tests.** In `organize.rs`, the 3 tests that call `plan_moves_with` directly (`picker_choice_overrides_extension_classify`, `picker_none_falls_back_to_extension_classify`, `picker_candidates_include_ontology_class_names`) must add a `&[]` rules arg before `&pick`/`&pick`-closure. Then add two new tests:

```rust
    #[test]
    fn user_rule_overrides_picker_and_extension() {
        // pic.png는 확장자로 Image지만, 사용자 규칙(ext png → Installer)이 우선 → Installer 목적지
        let onto = parse_ttl(ONTO).unwrap();
        let home = Path::new("/home/u");
        let rules = vec![crate::userrules::Rule {
            r#match: crate::userrules::RuleMatch { ext: Some("png".into()), name_contains: None, path_contains: None, min_size: None, max_size: None },
            class: "Installer".into(),
        }];
        let pick = |_p: &Path, _c: &[&str]| Some("Image".to_string()); // picker가 Image를 골라도
        let plans = plan_moves_with(&[fe("/d/pic.png", 10)], &onto, home, &rules, &pick);
        assert_eq!(plans.len(), 1);
        assert!(plans[0].class_id.ends_with("Installer")); // 규칙이 picker를 이긴다
    }

    #[test]
    fn no_user_rule_match_falls_through_to_picker() {
        // 규칙이 있으나 매칭 안 되면(ext iso) 기존 precedence(picker→classify)로
        let onto = parse_ttl(ONTO).unwrap();
        let home = Path::new("/home/u");
        let rules = vec![crate::userrules::Rule {
            r#match: crate::userrules::RuleMatch { ext: Some("iso".into()), name_contains: None, path_contains: None, min_size: None, max_size: None },
            class: "Installer".into(),
        }];
        let pick = |_p: &Path, _c: &[&str]| None;
        let plans = plan_moves_with(&[fe("/d/pic.png", 10)], &onto, home, &rules, &pick);
        assert_eq!(plans.len(), 1);
        assert!(plans[0].class_id.ends_with("Image")); // 확장자 폴백
    }
```

- [ ] **Step 2: Run — FAIL** `cargo test --lib organize` (arity mismatch on `plan_moves_with`).

- [ ] **Step 3: Implement.** In `organize.rs`, add the `rules` param and apply it first:
```rust
pub fn plan_moves_with(
    files: &[FileEntry],
    onto: &Ontology,
    home: &Path,
    rules: &[crate::userrules::Rule],
    pick: &dyn Fn(&Path, &[&str]) -> Option<String>,
) -> Vec<MovePlan> {
    let candidates: Vec<&str> = onto.classes.iter().map(|c| local_name(&c.id)).collect();
    let reasoner = crate::ontology::Reasoner::build(onto);
    let mut plans = Vec::new();
    for f in files {
        let Some(name) = f.path.file_name() else { continue };
        // precedence: 사용자 규칙 → picker(LLM) → 확장자 classify → 제외
        let local: String = match crate::userrules::classify_by_rules(rules, &f.path, f.size) {
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
Update `plan_moves` to pass `&[]`:
```rust
pub fn plan_moves(files: &[FileEntry], onto: &Ontology, home: &Path) -> Vec<MovePlan> {
    plan_moves_with(files, onto, home, &[], &|_, _| None)
}
```

- [ ] **Step 4: Add the config loader + thread rules through `plan_organize`.** In `commands.rs`, add near `bundled_ontology_ttl`:
```rust
/// 사용자 규칙 JSON 오버라이드 로드 — app_config_dir/userrules.json, 없으면 빈 배열. 파싱은 호출부(에러 표면화).
#[cfg(not(coverage))]
fn user_rules_json(app: &AppHandle) -> String {
    use tauri::Manager;
    if let Ok(dir) = app.path().app_config_dir() {
        if let Ok(s) = std::fs::read_to_string(dir.join("userrules.json")) { return s; }
    }
    "[]".to_string()
}
```
In `plan_organize` (`commands.rs:387`), parse rules once at the top and pass to both paths. Replace the body so rules are loaded before the feature block, the LLM path passes `&rules`, and the fallback uses `plan_moves_with(..., &rules, &|_,_| None)` (NOT `plan_moves`, so rules apply even without the model):
```rust
pub fn plan_organize(root: String, app: AppHandle, state: State<AppState>) -> Result<Vec<organize::MovePlan>, String> {
    let onto = load_ontology_from(&bundled_ontology_ttl(&app)?)?;
    let rules = crate::userrules::parse_rules(&user_rules_json(&app))?; // malformed → Err surfaced
    let files = dupes::collect_files(Path::new(&root));
    let home = resolve_home(&app);
    #[cfg(feature = "llm-engine")]
    {
        use tauri::Manager;
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        if model_status_for(&model_file_path(&dir)).present {
            let mut guard = state.engine.lock().unwrap();
            if guard.is_none() {
                if let Ok(e) = crate::llm::LlamaEngine::new(&model_file_path(&dir)) { *guard = Some(e); }
            }
            if let Some(engine) = guard.as_ref() {
                let pick = |p: &Path, cands: &[&str]| {
                    let meta = file_meta_at(p, 0, 0);
                    crate::llm::pick_class(engine, &meta, cands)
                };
                return Ok(organize::plan_moves_with(&files, &onto, &home, &rules, &pick));
            }
        }
    }
    Ok(organize::plan_moves_with(&files, &onto, &home, &rules, &|_, _| None))
}
```
(If `plan_organize` currently has `#[cfg_attr(not(feature = "llm-engine"), allow(unused_variables))]` or similar attrs, preserve them; `state` is used only in the feature block.)

- [ ] **Step 5: Run — PASS.** `cargo test --lib organize` then `cargo test --lib` (from `src-tauri`). `cargo llvm-cov --lib --summary-only` → `organize.rs` **100%** (the new rules-first branch is covered by the two new tests; the `&[]` no-rule path by the existing tests). `cargo build --lib`.

- [ ] **Step 6: Commit** `git commit -m "feat(organize): user rules take precedence over picker + extension classify"`.

---

## Task 3: user_rules command + api.ts + Inventory surfacing

**Files:** Modify `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`, `src/lib/api.ts`, `src/lib/Inventory.svelte`.

**Interfaces:**
- Produces: `#[cfg(not(coverage))] user_rules(app) -> Result<Vec<userrules::Rule>, String>`; api.ts `Rule`/`RuleMatch` + `getUserRules()`.

- [ ] **Step 1:** No new pure logic (the command is a thin `#[cfg(not(coverage))]` wrapper over `parse_rules`, already tested in Task 1). Add to `commands.rs` (after `plan_organize`):
```rust
/// 활성 사용자 규칙 조회(UI 표시용). 손상 파일은 Err.
#[cfg(not(coverage))]
#[tauri::command]
pub fn user_rules(app: AppHandle) -> Result<Vec<crate::userrules::Rule>, String> {
    crate::userrules::parse_rules(&user_rules_json(&app))
}
```
Register in `lib.rs` `generate_handler!`: `commands::user_rules,`.

- [ ] **Step 2:** `api.ts` — add:
```ts
export interface RuleMatch { ext: string | null; name_contains: string | null; path_contains: string | null; min_size: number | null; max_size: number | null; }
export interface Rule { match: RuleMatch; class: string; }
export const getUserRules = () => invoke<Rule[]>("user_rules");
```

- [ ] **Step 3:** `Inventory.svelte` — after the report loads, call `getUserRules()` and render an advisory line: when non-empty, "사용자 규칙 N개 적용 중"; on error, a small "규칙 파일 오류" note (the error message). Non-blocking (mirrors the coherence/insights advisory pattern; does not gate any control). Import `getUserRules`, `Rule`; `let userRulesCount = $state<number | null>(null);` set from the call, `.catch(() => {})`.

- [ ] **Step 4: Verify** `cargo build --lib`, `cargo test --lib` (unchanged pass), `cargo llvm-cov --lib --summary-only` (`commands.rs` pure helpers still 100%; the command is `#[cfg(not(coverage))]`), `npm run check` + `npm run build` clean.

- [ ] **Step 5: Commit** `git commit -m "feat(userrules): user_rules command + Inventory active-rule indicator"`.

---

## Post-implementation

- Whole-branch review focused on: precedence correctness (user rule beats picker beats classify), determinism (source-order first-match, no HashMap), the malformed-rules error-surface (not silent ignore), 100% coverage on `userrules.rs` + `organize.rs`, no new dependency, and that the safety layer is untouched (rules affect only the preview classification).
- Open the PR on `feat/user-rules` (spec committed). This completes the three advanced-reasoning sub-projects (A/B/C).
- Note: `plan_organize`/`commands.rs` also change in sub-project C (PR #12, still open); if #12 merges first, expect a trivial `commands.rs` merge (different functions — `reason_unknown_extensions` vs `plan_organize`/`user_rules`).
