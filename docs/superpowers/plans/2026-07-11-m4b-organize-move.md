# DiskSage M4b: 온톨로지 기반 재배치 + 안전한 이동 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 스캔된 파일을 온톨로지 클래스의 targetFolder로 옮기는 "정리정돈" 기능을, 같은 볼륨 원자적 rename / 다른 볼륨 복사-검증-휴지통의 안전한 이동 연산과 저널 기반 되돌리기 위에 구현한다.

**Architecture:** `safety`에 새 파괴적 연산 `move_file`(스펙 §7-2)을 추가 — trash_delete와 동일한 리거(보호 경로 거부, 실행 전 저널, 실패 시 롤백). `organize` 모듈이 파일 → 클래스(M4a classify) → targetFolder(M4a resolve_target) → 최종 목적지 경로를 계산해 이동 계획을 세운다(실행 안 함). UI가 계획을 미리보기하고 사용자 확인 후에만 실행. 저널은 되돌리기 소스가 되므로 이동 기록은 원자적으로 남긴다.

**Tech Stack:** 기존 스택. 새 의존성 없음(std::fs rename/copy + 기존 trash 크레이트 + blake3 검증).

## Global Constraints

- 스펙: `docs/superpowers/specs/2026-07-10-disksage-design.md` §5(온톨로지 재배치)/§7(안전 모델, 특히 §7-2 이동)/§8 — 충돌 시 스펙 우선
- **안전 불변식 (M2/M3에서 확립, 유지 + 확장)**:
  1. 이동은 오직 `safety::move_file` 경유. organize는 계획만 생산, 이동 안 함
  2. 같은 볼륨: 원자적 `std::fs::rename`. 다른 볼륨: 복사 → 크기+blake3 해시 검증 → **검증 성공 후에만** 원본 `trash_delete`(영구 삭제 아님)
  3. 보호 경로(출발/목적지 양쪽)는 safety 계층에서 거부. 목적지가 기존 파일을 덮어쓰지 않음(충돌 시 rename 또는 skip)
  4. 모든 이동은 실행 **전** 저널 기록(op="move", src, dst). 크로스 볼륨 복사 실패/검증 실패 시 부분 결과(복사된 목적지)를 정리하고 원본 보존
  5. 실행은 UI 미리보기+확인 후에만. "되돌리기"는 저널로 역이동(dst→src)
- **M4a 사전조건(최종 리뷰 지적) — Task 0에서 처리**: `ontology.rs`의 다중 `rdfs:subClassOf`가 조용히 last-wins → 이동 목적지를 좌우하므로, 단일 부모 제약을 문서화하거나 다중 부모를 명시 처리. 또한 override-load 비대칭(읽기 불가→폴백, malformed→에러) 정리
- **조직 CI 게이트**: 리눅스 `cargo llvm-cov --all-features --fail-under-lines 100`. Tauri 래퍼 `#[cfg(not(coverage))]`. **커버리지 규율(M3 교훈)**: io 에러는 내부 헬퍼가 `?`로 전파(클로저 없음) + 공개 경계서 한 번만 map_err(기존 에러 테스트가 커버). cfg(unix)/root 의존 커버리지 회피. 로컬 `cargo llvm-cov --json`으로 신규 파일 0 미커버 라인 확인 후 커밋
- 심링크는 순회서 제외(scanner::keep_entry). 커밋: conventional commits + 트레일러 `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`
- **원격 main 직접 push 불가** — 브랜치 `feat/m4b-organize-move`, 마지막 PR (스쿼시)
- `cargo` PATH 부재: bash `export PATH="$HOME/.cargo/bin:$PATH"`. cargo test/llvm-cov 타임아웃 600000ms
- 실제 이동 테스트는 tempdir 내에서만. 크로스 볼륨은 동일 볼륨 시뮬레이션(같은 tempdir, force-copy 경로 별도 함수로 분리해 테스트). 휴지통 왕복 테스트는 M2 패턴대로 픽스처만 purge

---

### Task 0: M4a 사전조건 처리 (온톨로지 다중 부모)

**Files:**
- Modify: `src-tauri/src/ontology.rs`
- Test: 같은 파일 tests

**Interfaces:**
- Consumes: 기존 parse_ttl
- Produces: 다중 subClassOf에 대한 명시적·문서화된 동작. `resolve_target` 시그니처 불변

- [ ] **Step 1: 실패/검증 테스트 작성**

`ontology.rs` tests에 추가:

```rust
    #[test]
    fn multiple_subclassof_keeps_first_parent_deterministically() {
        // OWL은 다중 상위클래스를 허용하지만, targetFolder 상속은 결정적이어야 한다.
        // 정책: 첫 번째 subClassOf를 부모로 채택(선언 순서), 이후는 무시(문서화된 제약).
        let ttl = r#"
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix dm: <https://disksage.app/ontology#> .
dm:A a owl:Class ; dm:targetFolder "~/A" .
dm:B a owl:Class ; dm:targetFolder "~/B" .
dm:C a owl:Class ; rdfs:subClassOf dm:A ; rdfs:subClassOf dm:B .
"#;
        let onto = parse_ttl(ttl).unwrap();
        let c = onto.classes.iter().find(|c| c.id.ends_with("#C")).unwrap();
        // 첫 부모 A 채택 (last-wins가 아니라 first-wins로 결정)
        assert!(c.parent.as_deref().unwrap().ends_with("#A"));
        assert_eq!(onto.resolve_target(&c.id).as_deref(), Some("~/A"));
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cd src-tauri; cargo test ontology`
Expected: FAIL — 현재 last-wins라 B를 채택

- [ ] **Step 3: 구현 — first-wins로 변경**

`ontology.rs`의 subClassOf 처리에서 `parents.insert(s, o)`를 첫 값만 유지하도록:

```rust
            RDFS_SUBCLASS => {
                if let Some(o) = /* object IRI */ {
                    // 다중 상위클래스: 첫 선언만 부모로(결정적). 문서화된 제약.
                    parents.entry(s).or_insert(o);
                }
            }
```
(`BTreeMap`이면 `.entry(s).or_insert(o)`; 정확한 코드는 현재 parse_ttl 구조에 맞춰 조정. 핵심: insert 덮어쓰기 → entry-or_insert로 첫값 유지)

모듈 doc 주석에 한 줄 추가: `// 다중 rdfs:subClassOf는 첫 선언만 부모로 채택(단일 부모 트리 가정) — targetFolder 상속의 결정성을 위해.`

- [ ] **Step 4: override-load 비대칭 주석**

`commands.rs`의 `bundled_ontology_ttl`에 주석 한 줄: 읽기 불가 override는 폴백, malformed override는 에러 — 의도적(사용자가 편집한 잘못된 파일은 조용히 무시하지 않고 알린다).

- [ ] **Step 5: 통과 + Commit**

Run: `cd src-tauri; cargo test ontology`
Expected: 신규 포함 PASS

```powershell
git add src-tauri
git commit -m "fix(ontology): deterministic first-parent for multiple subClassOf

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 1: `safety` — 안전한 이동 연산

**Files:**
- Modify: `src-tauri/src/safety.rs`
- Test: 같은 파일 tests

**Interfaces:**
- Consumes: 기존 `is_protected`, `strip_verbatim`, `JournalEntry`, `journal_append`, `trash_delete`
- Produces:
  - `safety::same_volume(a: &Path, b: &Path) -> bool` — 두 경로가 같은 볼륨인지(순수, rename 가능 판정용). Windows: 드라이브 문자 비교; unix: `std::fs::metadata().dev()` 비교(둘 다 존재해야 하므로 목적지는 부모 디렉토리로 판정)
  - `safety::move_file(src: &Path, dst: &Path, journal_path: &Path, now_ms: u64) -> Result<(), SafetyError>` — 앱 유일 이동 경로. 보호 검사(src+dst) → pending 저널 → same_volume이면 rename, 아니면 copy+verify+trash_delete(src) → outcome 저널. dst 충돌 시 Err(호출자가 유니크 경로 생성)

- [ ] **Step 1: 실패 테스트 작성**

`safety.rs` tests에 추가:

```rust
    #[test]
    fn move_file_rejects_protected_src_or_dst() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let f = tmp.path().join("f.bin");
        std::fs::write(&f, b"x").unwrap();
        let protected = std::path::PathBuf::from(if cfg!(windows) { "C:\\Windows\\x" } else { "/usr/x" });
        // 보호된 목적지
        assert!(matches!(move_file(&f, &protected, &jp, 1), Err(SafetyError::Protected(_))));
        // 보호된 출발
        let pf = std::path::PathBuf::from(if cfg!(windows) { "C:\\Windows\\y" } else { "/usr/y" });
        assert!(matches!(move_file(&pf, &tmp.path().join("z"), &jp, 1), Err(SafetyError::Protected(_))));
        assert!(journal_recent(&jp, 10).is_empty(), "보호 거부는 저널 이전");
    }

    #[test]
    fn move_file_same_dir_renames_and_journals() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let src = tmp.path().join("a.bin");
        let dst = tmp.path().join("sub").join("a.bin");
        std::fs::create_dir(tmp.path().join("sub")).unwrap();
        std::fs::write(&src, vec![0u8; 20]).unwrap();

        move_file(&src, &dst, &jp, 7).unwrap();

        assert!(!src.exists());
        assert!(dst.exists());
        assert_eq!(std::fs::read(&dst).unwrap().len(), 20);
        let recent = journal_recent(&jp, 10);
        assert_eq!(recent[0].outcome, "ok");
        assert_eq!(recent[0].op, "move");
    }

    #[test]
    fn move_file_rejects_existing_destination() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let src = tmp.path().join("a.bin");
        let dst = tmp.path().join("b.bin");
        std::fs::write(&src, b"aa").unwrap();
        std::fs::write(&dst, b"bb").unwrap(); // 이미 존재
        assert!(move_file(&src, &dst, &jp, 1).is_err());
        // 원본과 기존 목적지 모두 보존
        assert!(src.exists());
        assert_eq!(std::fs::read(&dst).unwrap(), b"bb");
    }

    #[test]
    fn same_volume_true_within_tempdir() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a");
        let b = tmp.path().join("sub");
        std::fs::write(&a, b"x").unwrap();
        std::fs::create_dir(&b).unwrap();
        assert!(same_volume(&a, &b));
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cd src-tauri; cargo test safety`
Expected: COMPILE ERROR — `move_file`/`same_volume` not found

- [ ] **Step 3: 구현**

`safety.rs`에 추가. io 에러는 내부 헬퍼가 `?`로 전파(커버리지 규율):

```rust
/// 두 경로가 같은 볼륨인지 — rename 가능 판정. 목적지는 아직 없을 수 있어 부모로 판정.
pub fn same_volume(src: &Path, dst: &Path) -> bool {
    let dst_probe = dst.parent().unwrap_or(dst);
    #[cfg(windows)]
    {
        fn drive(p: &Path) -> Option<String> {
            p.components().next().and_then(|c| match c {
                std::path::Component::Prefix(pr) => Some(pr.as_os_str().to_string_lossy().to_lowercase()),
                _ => None,
            })
        }
        // canonicalize로 상대경로/verbatim 정규화 후 드라이브 비교(best-effort)
        let s = std::fs::canonicalize(src).unwrap_or_else(|_| src.to_path_buf());
        let d = std::fs::canonicalize(dst_probe).unwrap_or_else(|_| dst_probe.to_path_buf());
        drive(&s) == drive(&d)
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let sd = std::fs::metadata(src).map(|m| m.dev());
        let dd = std::fs::metadata(dst_probe).map(|m| m.dev());
        matches!((sd, dd), (Ok(a), Ok(b)) if a == b)
    }
}

// 크로스 볼륨 복사+검증(내부 io, ? 전파). 검증 실패 시 목적지 정리하고 io::Error.
fn copy_verified_io(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::copy(src, dst)?;
    // 크기+blake3로 무결성 검증
    let ok = std::fs::metadata(src)?.len() == std::fs::metadata(dst)?.len()
        && crate::dupes::hash_full(src).ok() == crate::dupes::hash_full(dst).ok();
    if !ok {
        let _ = std::fs::remove_file(dst); // 부분 복사 정리 (원본은 건드리지 않음)
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "복사 검증 실패"));
    }
    Ok(())
}

/// 앱 유일의 이동 경로 (스펙 §7-2). 영구 삭제 없음 — 원본 제거는 trash_delete 경유.
pub fn move_file(
    src: &Path,
    dst: &Path,
    journal_path: &Path,
    now_ms: u64,
) -> Result<(), SafetyError> {
    // 보호: src·dst 양쪽, ParentDir 거부, verbatim 정규화 — trash_delete와 동일 리거
    for p in [src, dst] {
        if p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            return Err(SafetyError::Protected(p.to_path_buf()));
        }
        let guard = strip_verbatim(&std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()));
        if is_protected(&guard) {
            return Err(SafetyError::Protected(p.to_path_buf()));
        }
    }
    // 목적지 충돌 금지 (덮어쓰기 방지)
    if dst.exists() {
        return Err(SafetyError::Trash(format!("목적지가 이미 존재: {}", dst.display())));
    }
    // 목적지 부모 디렉토리 생성
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent).map_err(|e| SafetyError::Trash(e.to_string()))?;
    }

    let mut entry = JournalEntry {
        ts_ms: now_ms,
        op: "move".into(),
        path: format!("{} -> {}", src.display(), dst.display()),
        bytes: std::fs::metadata(src).map(|m| m.len()).unwrap_or(0),
        outcome: "pending".into(),
    };
    journal_append(journal_path, &entry)?;

    let result = if same_volume(src, dst) {
        std::fs::rename(src, dst).map_err(|e| SafetyError::Trash(e.to_string()))
    } else {
        // 크로스 볼륨: 복사+검증 후 원본 휴지통
        copy_verified_io(src, dst)
            .map_err(|e| SafetyError::Trash(e.to_string()))
            .and_then(|()| {
                let bytes = std::fs::metadata(dst).map(|m| m.len()).unwrap_or(0);
                trash_delete(src, bytes, journal_path, now_ms)
            })
    };

    entry.outcome = match &result {
        Ok(()) => "ok".into(),
        Err(e) => format!("error:{e}"),
    };
    journal_append(journal_path, &entry)?;
    result
}
```

주의(구현자): `same_volume`의 cfg(windows)/cfg(unix) 분기는 플랫폼 스큐 — Windows 로컬 커버리지는 windows 분기만, 리눅스 게이트는 unix 분기만 커버(각각 100% 라인). `same_volume_true_within_tempdir` 테스트가 각 플랫폼의 자기 분기를 커버. copy_verified_io의 검증-실패 arm은 재현이 어려우니, 필요 시 크기는 같지만 내용이 다른 상황을 만들기 어렵다 — 대신 검증 성공 경로만 크로스볼륨 테스트로 커버하고, 실패 arm은 리눅스 게이트서 미커버면 카테고리 판정 후 보고(또는 copy_verified_io를 순수 검증 함수와 분리해 검증 실패를 단위 테스트). **커버리지는 Task 4에서 정합**.

- [ ] **Step 4: 통과 + Commit**

Run: `cd src-tauri; cargo test safety`
Expected: 신규 4개 포함 PASS

```powershell
git add src-tauri
git commit -m "feat(safety): journaled same/cross-volume move with verification

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: `organize` — 이동 계획 수립

**Files:**
- Create: `src-tauri/src/organize.rs`
- Modify: `src-tauri/src/lib.rs` (`mod organize;`)
- Test: `src-tauri/src/organize.rs` 내 tests

**Interfaces:**
- Consumes: `dupes::FileEntry`, `inventory::classify`, `ontology::Ontology`(resolve_target), M4a 것
- Produces:
  - `organize::MovePlan { src: String, dst: String, class_id: String }` (serde::Serialize)
  - `organize::plan_moves(files: &[FileEntry], onto: &Ontology, home: &Path) -> Vec<MovePlan>` — 각 파일을 classify → 온톨로지 클래스 → resolve_target로 목적지 폴더 계산 → `{targetFolder}/{filename}` 목적지. `~`는 home으로 치환, `{class}` 플레이스홀더는 클래스 로컬명으로 치환. 이미 목적지 폴더 안에 있는 파일은 계획서 제외. 미분류/targetFolder 없는 파일도 제외

- [ ] **Step 1: 실패 테스트 작성**

`organize.rs` tests에 추가:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::dupes::FileEntry;
    use crate::ontology::parse_ttl;
    use std::path::{Path, PathBuf};

    const ONTO: &str = r#"
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix dm: <https://disksage.app/ontology#> .
dm:Image a owl:Class ; rdfs:label "이미지"@ko ; dm:targetFolder "~/Media/{class}" .
dm:Code a owl:Class ; rdfs:label "코드"@ko .
"#;

    fn fe(p: &str, size: u64) -> FileEntry { FileEntry { path: PathBuf::from(p), size } }

    #[test]
    fn plans_move_to_resolved_target_folder() {
        let onto = parse_ttl(ONTO).unwrap();
        let home = Path::new("/home/u");
        let files = vec![fe("/downloads/pic.png", 100)];
        let plans = plan_moves(&files, &onto, home);
        assert_eq!(plans.len(), 1);
        // ~ → home, {class} → Image
        assert_eq!(plans[0].dst, "/home/u/Media/Image/pic.png");
        assert!(plans[0].class_id.ends_with("Image"));
    }

    #[test]
    fn skips_unclassified_and_targetless() {
        let onto = parse_ttl(ONTO).unwrap();
        let home = Path::new("/home/u");
        let files = vec![
            fe("/x/unknown.xyz", 10),   // 미분류 → 제외
            fe("/x/main.rs", 20),       // Code: targetFolder 없음 → 제외
        ];
        assert!(plan_moves(&files, &onto, home).is_empty());
    }

    #[test]
    fn skips_file_already_in_destination() {
        let onto = parse_ttl(ONTO).unwrap();
        let home = Path::new("/home/u");
        // 이미 목적지 폴더에 있는 파일
        let files = vec![fe("/home/u/Media/Image/pic.png", 100)];
        assert!(plan_moves(&files, &onto, home).is_empty());
    }
}
```

- [ ] **Step 2: 실패 확인**

`lib.rs`에 `#[cfg_attr(coverage, allow(dead_code))] mod organize;` 추가 후:

Run: `cd src-tauri; cargo test organize`
Expected: COMPILE ERROR

- [ ] **Step 3: 구현**

`organize.rs` 상단:

```rust
use std::path::Path;

use crate::dupes::FileEntry;
use crate::inventory::classify;
use crate::ontology::Ontology;

#[derive(Debug, Clone, serde::Serialize)]
pub struct MovePlan {
    pub src: String,
    pub dst: String,
    pub class_id: String,
}

/// 파일 → 클래스 → targetFolder → 목적지 경로. 미분류·targetFolder 없음·이미 목적지 안은 제외.
pub fn plan_moves(files: &[FileEntry], onto: &Ontology, home: &Path) -> Vec<MovePlan> {
    let mut plans = Vec::new();
    for f in files {
        let Some(local) = classify(&f.path) else { continue };
        // 로컬명 → 온톨로지 클래스
        let Some(class) = onto.classes.iter().find(|c| {
            c.id.rsplit(['#', '/']).next().unwrap_or(&c.id) == local
        }) else { continue };
        let Some(template) = onto.resolve_target(&class.id) else { continue };
        // 템플릿 치환: ~ → home, {class} → 로컬명
        let folder = template
            .replacen('~', &home.to_string_lossy(), 1)
            .replace("{class}", local);
        let Some(name) = f.path.file_name() else { continue };
        let dst = Path::new(&folder).join(name);
        // 이미 목적지 폴더에 있으면 제외
        if f.path.parent() == Some(Path::new(&folder)) {
            continue;
        }
        plans.push(MovePlan {
            src: f.path.to_string_lossy().into_owned(),
            dst: dst.to_string_lossy().into_owned(),
            class_id: class.id.clone(),
        });
    }
    plans
}
```

주의(구현자): 테스트가 `/home/u/Media/Image/pic.png`를 기대하므로 경로 구분자는 unix 스타일. Windows에서 `Path::join`은 `\`를 쓰므로 테스트가 플랫폼 의존적일 수 있다 — 테스트 기대값을 `Path::new(...).join(...)`으로 구성하거나 cfg로 분기. 로컬 Windows서 통과하도록 조정하되 로직은 동일. dst 문자열 비교 대신 `PathBuf` 비교를 쓰면 플랫폼 무관.

- [ ] **Step 4: 통과 + Commit**

Run: `cd src-tauri; cargo test organize`
Expected: 3 tests PASS

```powershell
git add src-tauri
git commit -m "feat(organize): ontology-driven move planning

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: 커맨드 계층 — 재배치 IPC

**Files:**
- Modify: `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`
- Test: commands.rs tests (순수 조율 있으면)

**Interfaces:**
- Consumes: organize, safety::move_file, M4a 로더
- Produces:
  - command `plan_organize(root: String) -> Result<Vec<MovePlan>, String>` (`#[cfg(not(coverage))]`, async) — 온톨로지 로드 + collect_files + plan_moves(home 주입)
  - command `execute_moves(plans: Vec<MovePlan>) -> Result<Vec<CleanResult>, String>` (`#[cfg(not(coverage))]`, async) — 각 MovePlan을 safety::move_file로 실행, 항목별 결과(CleanResult 재사용: {path, ok, error})
  - command `undo_last_moves(limit: usize) -> Result<Vec<CleanResult>, String>` (`#[cfg(not(coverage))]`) — 저널서 최근 move 항목 읽어 역이동(dst→src)
  - 순수 헬퍼 `moves_to_results` 등 테스트 가능한 조율 로직이 있으면 분리

- [ ] **Step 1~5**: M2 Task 6 패턴을 따라 TDD. 순수 조율 함수(예: 저널 move 항목 파싱→역이동 계획)를 분리해 테스트, Tauri 래퍼는 `#[cfg(not(coverage))]`. now_ms/home/journal_path는 래퍼가 주입. execute_moves는 항목별 실패 격리(하나 실패해도 나머지 진행). 커밋 메시지 `feat(commands): organize preview, execute, and undo IPC`.

(상세 코드는 M2 Task 6 구조를 그대로 따르되 삭제 대신 move_file 호출. 구현자는 M2 commands.rs의 clean_paths_inner/journal_file_path/now_ms 헬퍼를 참조.)

---

### Task 4: 프론트엔드 — 재배치 뷰 + 커버리지 정합

**Files:**
- Create: `src/lib/Organize.svelte`
- Modify: `src/lib/api.ts`, `src/routes/+page.svelte`
- 커버리지: safety.rs/organize.rs 신규 라인 정합

**Interfaces:**
- Consumes: Task 3 커맨드
- Produces: api.ts `planOrganize/executeMoves/undoLastMoves` + `MovePlan` 타입; `Organize.svelte` props `{ scannedRoot }`

- [ ] UI: 계획 미리보기(src → dst 목록, 클래스별 그룹), 확인 다이얼로그(휴지통 복원 안내 불필요 — 이동은 되돌리기 버튼으로), 실행, "마지막 이동 되돌리기" 버튼. 스펙 §7-6 확인 게이트. api.ts 타입은 Rust 계약과 정확히 일치.
- 커버리지: `cargo llvm-cov --json`으로 safety.rs move_file/same_volume/copy_verified_io, organize.rs plan_moves의 신규 라인 확인. same_volume은 플랫폼 분기(각 게이트서 자기 분기 100%). copy_verified_io 검증-실패 arm은 순수 검증 함수 분리 또는 카테고리 판정. io 에러 경로는 M3 교훈대로 내부 `?` + 경계 map_err 구조라 missing-path 테스트가 커버.
- 게이트: `cargo test`, `RUSTFLAGS="--cfg coverage" cargo check`(0경고), `npm test/coverage/build/check` 전부 클린.
- Commit: `feat(ui): organize preview with move execution and undo`, 필요 시 `test(coverage): close m4b gaps`.

---

### Task 5: 최종 검증 + PR

- [ ] 전체 게이트: `cargo test` 전부 PASS, `npm test/coverage/build/check` 클린, `RUSTFLAGS="--cfg coverage" cargo check` 0경고, 로컬 llvm-cov 신규 파일 100% 라인
- [ ] **안전 감사**: 프로덕션 유일 이동은 safety::move_file, 유일 삭제는 trash_delete. `std::fs::rename`/`copy`/`remove_file`이 safety.rs 밖 프로덕션 코드에 없는지 grep. 크로스볼륨 실패 시 원본 보존 확인
- [ ] 푸시 + PR: 제목 "feat: M4b ontology-driven reorganization with safe move + undo", 본문에 안전 불변식(이동 전용 경로, 저널 되돌리기, 크로스볼륨 복사-검증-후-휴지통, 보호 경로 거부) 명시
- [ ] 사람 검증 체크리스트(사용자 전달): 스캔 → 재배치 미리보기 → 실행 → 파일이 targetFolder로 이동 확인 → "되돌리기" → 원위치 복원. 보호 경로로의 이동 거부 확인. (크로스 볼륨은 실제 두 드라이브 필요 — 사용자 환경서만)
