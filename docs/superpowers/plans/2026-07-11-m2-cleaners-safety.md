# DiskSage M2: 캐시/개발 아티팩트 정리 + 안전 계층 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 알려진 캐시 위치와 오래된 개발 아티팩트를 찾아 사용자 검토 후 OS 휴지통으로 보내는, 저널링된 안전 계층 위의 첫 파괴적 기능을 완성한다.

**Architecture:** 모든 파괴적 작업은 새 `safety` 모듈 하나를 통과한다(보호 경로 거부 → 저널 기록 → `trash` 크레이트 → 결과 기록). `rules`는 주입된 베이스 디렉토리 기준의 정적 캐시 카탈로그, `dev_artifacts`는 프로젝트 마커 인접 + mtime 기준 탐지. Tauri command 래퍼는 전부 얇고 `#[cfg(not(coverage))]`, 순수 로직은 100% 라인 커버리지로 측정된다.

**Tech Stack:** 기존 스택 + `trash` 크레이트(휴지통), `serde_json`(저널 JSONL)

## Global Constraints

- 스펙: `docs/superpowers/specs/2026-07-10-disksage-design.md` §4/§7/§8 — 충돌 시 스펙 우선
- **안전 불변식 (절대 완화 금지)**: ① 영구 삭제 코드 경로 없음 — 삭제는 항상 `trash` 크레이트 경유 ② 보호 경로는 safety 계층에서 거부 (호출자가 아니라) ③ 모든 파괴적 작업은 실행 **전** 저널 기록 ④ 휴지통 이동 실패 시 해당 항목만 중단·보고, 영구 삭제 폴백 없음 ⑤ 실행은 UI 검토·확인 후에만
- **조직 CI 게이트 (M1에서 확립)**: 리눅스 러너에서 `cargo llvm-cov --all-features --fail-under-lines 100` 통과 필요. 새 Tauri command 래퍼는 `#[cfg(not(coverage))]`, 순수 로직은 테스트로 100% 라인 커버. JS는 `npm run coverage` 4개 지표 100 (vitest.config.ts의 coverage.include에 새 순수 .ts 모듈 추가 시 테스트 필수)
- 에러 arm은 한 줄 let-else 스타일(라인 커버리지 안정), cfg(unix) 폴트 인젝션 패턴은 scanner.rs 테스트 참조
- **원격 main 직접 push 불가** — 모든 작업은 `feat/m2-cleaners-safety` 브랜치, 마지막에 PR (스쿼시 머지됨)
- 커밋: conventional commits + 트레일러 `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`
- `cargo`는 새 셸 PATH에 없음: PowerShell `& "$env:USERPROFILE\.cargo\bin\cargo.exe"` / bash `export PATH="$HOME/.cargo/bin:$PATH"`
- Rust 명령은 `src-tauri/`에서, npm 명령은 루트에서. cargo test 타임아웃 600000ms
- 테스트가 휴지통에 넣은 픽스처는 테스트가 `trash::os_limited`로 정리(purge)한다 — 제품 코드에는 purge 호출이 존재하면 안 된다

---

### Task 1: `safety` — 보호 경로 가드

**Files:**
- Create: `src-tauri/src/safety.rs`
- Modify: `src-tauri/src/lib.rs` (`mod safety;` 추가)
- Test: `src-tauri/src/safety.rs` 내 `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: 없음
- Produces:
  - `safety::is_protected(path: &Path) -> bool` — 시스템/루트 경로 여부
  - `safety::SafetyError` enum: `Protected(PathBuf)`, `Trash(String)`, `Journal(String)` (derive `Debug`, `thiserror` 없이 수동 `Display`)

- [ ] **Step 1: 실패하는 테스트 작성**

`src-tauri/src/safety.rs` 생성, 하단에:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn protects_system_and_root_paths() {
        #[cfg(windows)]
        {
            assert!(is_protected(Path::new("C:\\")));
            assert!(is_protected(Path::new("C:\\Windows")));
            assert!(is_protected(Path::new("C:\\Windows\\System32")));
            assert!(is_protected(Path::new("C:\\Program Files")));
            assert!(is_protected(Path::new("C:\\Program Files (x86)\\App")));
        }
        #[cfg(unix)]
        {
            assert!(is_protected(Path::new("/")));
            assert!(is_protected(Path::new("/usr")));
            assert!(is_protected(Path::new("/usr/bin/ls")));
            assert!(is_protected(Path::new("/etc")));
            assert!(is_protected(Path::new("/bin")));
            assert!(is_protected(Path::new("/lib")));
        }
    }

    #[test]
    fn protects_home_root_but_not_home_children() {
        let home = if cfg!(windows) {
            std::env::var("USERPROFILE").unwrap()
        } else {
            std::env::var("HOME").unwrap()
        };
        assert!(is_protected(Path::new(&home)));
        assert!(!is_protected(&Path::new(&home).join("some-cache-dir")));
    }

    #[test]
    fn allows_ordinary_deep_paths() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!is_protected(&tmp.path().join("node_modules")));
    }
}
```

- [ ] **Step 2: 컴파일 실패 확인**

`src-tauri/src/lib.rs`의 `mod scanner;` 옆에 `mod safety;` 추가 후:

Run: `cd src-tauri; cargo test safety`
Expected: COMPILE ERROR — `is_protected` not found

- [ ] **Step 3: 구현**

`src-tauri/src/safety.rs` 상단:

```rust
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum SafetyError {
    Protected(PathBuf),
    Trash(String),
    Journal(String),
}

impl std::fmt::Display for SafetyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SafetyError::Protected(p) => write!(f, "보호된 경로: {}", p.display()),
            SafetyError::Trash(e) => write!(f, "휴지통 이동 실패: {e}"),
            SafetyError::Journal(e) => write!(f, "저널 기록 실패: {e}"),
        }
    }
}

/// 시스템·루트 경로 하드 거부 목록 (스펙 §7-3).
/// 안전 계층의 최후 방어선 — 호출자가 무엇을 넘기든 여기서 걸러진다.
pub fn is_protected(path: &Path) -> bool {
    // 드라이브/파일시스템 루트 자체
    if path.parent().is_none() {
        return true;
    }
    // 사용자 홈 루트 자체 (하위는 허용)
    let home = std::env::var(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).ok();
    if let Some(h) = home {
        if path == Path::new(&h) {
            return true;
        }
    }
    #[cfg(windows)]
    {
        let lower = path.to_string_lossy().to_lowercase();
        let denied_prefixes = ["c:\\windows", "c:\\program files", "c:\\program files (x86)"];
        if denied_prefixes.iter().any(|d| lower.starts_with(d)) {
            return true;
        }
    }
    #[cfg(unix)]
    {
        let denied_prefixes = ["/usr", "/etc", "/bin", "/sbin", "/lib", "/boot", "/proc", "/sys", "/dev"];
        let s = path.to_string_lossy();
        if denied_prefixes
            .iter()
            .any(|d| s == *d || s.starts_with(&format!("{d}/")))
        {
            return true;
        }
    }
    false
}
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cd src-tauri; cargo test safety`
Expected: 3 tests PASS

- [ ] **Step 5: Commit**

```powershell
git add src-tauri
git commit -m "feat(safety): protected-path guard

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: `safety` — 작업 저널 (JSONL)

**Files:**
- Modify: `src-tauri/src/safety.rs`
- Modify: `src-tauri/Cargo.toml` (`serde_json` 의존성 — 이미 있으면 생략)
- Test: 같은 파일 `tests` 모듈

**Interfaces:**
- Consumes: Task 1의 `SafetyError`
- Produces:
  - `safety::JournalEntry { ts_ms: u64, op: String, path: String, bytes: u64, outcome: String }` (derive `Debug, Clone, serde::Serialize, serde::Deserialize`)
  - `safety::journal_append(journal_path: &Path, entry: &JournalEntry) -> Result<(), SafetyError>`
  - `safety::journal_recent(journal_path: &Path, limit: usize) -> Vec<JournalEntry>` — 최신순, 파일 없으면 빈 벡터

- [ ] **Step 1: 의존성 확인/추가**

Run: `cd src-tauri; cargo add serde_json`
(이미 의존성에 있으면 no-op)

- [ ] **Step 2: 실패하는 테스트 작성**

`tests` 모듈에 추가:

```rust
    #[test]
    fn journal_roundtrip_newest_first() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("journal.jsonl");
        for i in 0..3u64 {
            journal_append(
                &jp,
                &JournalEntry {
                    ts_ms: 1000 + i,
                    op: "trash_delete".into(),
                    path: format!("/x/{i}"),
                    bytes: i * 10,
                    outcome: "ok".into(),
                },
            )
            .unwrap();
        }
        let recent = journal_recent(&jp, 2);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].path, "/x/2"); // 최신이 먼저
        assert_eq!(recent[1].path, "/x/1");
    }

    #[test]
    fn journal_recent_missing_file_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(journal_recent(&tmp.path().join("none.jsonl"), 5).is_empty());
    }

    #[test]
    fn journal_append_reports_io_error() {
        let tmp = tempfile::tempdir().unwrap();
        // 디렉토리를 저널 경로로 주면 열기 실패
        let err = journal_append(
            tmp.path(),
            &JournalEntry {
                ts_ms: 1,
                op: "trash_delete".into(),
                path: "/x".into(),
                bytes: 0,
                outcome: "ok".into(),
            },
        );
        assert!(matches!(err, Err(SafetyError::Journal(_))));
    }
```

- [ ] **Step 3: 실패 확인**

Run: `cd src-tauri; cargo test safety`
Expected: COMPILE ERROR — `JournalEntry` not found

- [ ] **Step 4: 구현**

`safety.rs`에 추가 (is_protected 아래):

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JournalEntry {
    pub ts_ms: u64,
    pub op: String,
    pub path: String,
    pub bytes: u64,
    pub outcome: String,
}

/// 파괴적 작업 저널 — 실행 전 "pending"으로 먼저 기록되고 결과로 덧붙는다 (스펙 §7-4)
pub fn journal_append(journal_path: &Path, entry: &JournalEntry) -> Result<(), SafetyError> {
    use std::io::Write;
    let line = serde_json::to_string(entry).map_err(|e| SafetyError::Journal(e.to_string()))?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(journal_path)
        .map_err(|e| SafetyError::Journal(e.to_string()))?;
    writeln!(f, "{line}").map_err(|e| SafetyError::Journal(e.to_string()))
}

pub fn journal_recent(journal_path: &Path, limit: usize) -> Vec<JournalEntry> {
    let Ok(content) = std::fs::read_to_string(journal_path) else { return Vec::new() };
    let mut entries: Vec<JournalEntry> = content
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    entries.reverse();
    entries.truncate(limit);
    entries
}
```

- [ ] **Step 5: 통과 확인 + Commit**

Run: `cd src-tauri; cargo test safety`
Expected: 6 tests PASS

```powershell
git add src-tauri
git commit -m "feat(safety): JSONL operation journal

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: `safety` — 휴지통 삭제 (저널 연동)

**Files:**
- Modify: `src-tauri/src/safety.rs`, `src-tauri/Cargo.toml` (`trash` 추가)
- Test: 같은 파일 `tests` 모듈

**Interfaces:**
- Consumes: Task 1-2 전부
- Produces:
  - `safety::trash_delete(path: &Path, bytes: u64, journal_path: &Path, now_ms: u64) -> Result<(), SafetyError>` — 보호 검사 → pending 저널 → trash → outcome 저널. **이 함수가 앱에서 유일한 삭제 경로다**

- [ ] **Step 1: 의존성 추가**

Run: `cd src-tauri; cargo add trash`

- [ ] **Step 2: 실패하는 테스트 작성**

`tests` 모듈에 추가:

```rust
    #[test]
    fn trash_delete_rejects_protected_path_without_journaling() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let root = if cfg!(windows) { "C:\\Windows" } else { "/usr" };
        let err = trash_delete(Path::new(root), 0, &jp, 1);
        assert!(matches!(err, Err(SafetyError::Protected(_))));
        assert!(journal_recent(&jp, 10).is_empty(), "보호 거부는 저널 이전에 일어나야 함");
    }

    #[test]
    fn trash_delete_missing_path_journals_error_outcome() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let missing = tmp.path().join("ghost.bin");
        let err = trash_delete(&missing, 0, &jp, 1);
        assert!(matches!(err, Err(SafetyError::Trash(_))));
        let recent = journal_recent(&jp, 10);
        assert_eq!(recent.len(), 2); // pending + error
        assert!(recent[0].outcome.starts_with("error:"));
        assert_eq!(recent[1].outcome, "pending");
    }

    // 실제 휴지통 왕복 (스펙 §9 통합 테스트 1). trash::os_limited는 win/linux 전용.
    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn trash_delete_roundtrip_lands_in_trash() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let victim = tmp.path().join("disksage-roundtrip-fixture.bin");
        std::fs::write(&victim, vec![0u8; 64]).unwrap();

        trash_delete(&victim, 64, &jp, 42).unwrap();

        assert!(!victim.exists(), "원본은 사라져야 함");
        let recent = journal_recent(&jp, 10);
        assert_eq!(recent[0].outcome, "ok");
        assert_eq!(recent[0].ts_ms, 42);

        // 휴지통에서 확인 후 테스트 픽스처만 purge (제품 코드가 아닌 테스트 정리)
        let items: Vec<_> = trash::os_limited::list()
            .unwrap()
            .into_iter()
            .filter(|i| i.name.to_string_lossy().contains("disksage-roundtrip-fixture"))
            .collect();
        assert!(!items.is_empty(), "휴지통에 있어야 함");
        trash::os_limited::purge_all(items).unwrap();
    }
```

- [ ] **Step 3: 실패 확인**

Run: `cd src-tauri; cargo test safety`
Expected: COMPILE ERROR — `trash_delete` not found

- [ ] **Step 4: 구현**

`safety.rs`에 추가:

```rust
/// 앱 유일의 삭제 경로 (스펙 §7-1). 영구 삭제 API는 이 크레이트 어디에도 없다.
pub fn trash_delete(
    path: &Path,
    bytes: u64,
    journal_path: &Path,
    now_ms: u64,
) -> Result<(), SafetyError> {
    if is_protected(path) {
        return Err(SafetyError::Protected(path.to_path_buf()));
    }
    let mut entry = JournalEntry {
        ts_ms: now_ms,
        op: "trash_delete".into(),
        path: path.to_string_lossy().into_owned(),
        bytes,
        outcome: "pending".into(),
    };
    journal_append(journal_path, &entry)?;
    match trash::delete(path) {
        Ok(()) => {
            entry.outcome = "ok".into();
            journal_append(journal_path, &entry)?;
            Ok(())
        }
        Err(e) => {
            entry.outcome = format!("error:{e}");
            journal_append(journal_path, &entry)?;
            Err(SafetyError::Trash(e.to_string()))
        }
    }
}
```

- [ ] **Step 5: 통과 확인 + Commit**

Run: `cd src-tauri; cargo test safety`
Expected: 9 tests PASS (win/linux — 왕복 테스트 포함)

```powershell
git add src-tauri
git commit -m "feat(safety): journaled trash-only delete

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 4: `rules` — 캐시 카탈로그

**Files:**
- Create: `src-tauri/src/rules.rs`
- Modify: `src-tauri/src/lib.rs` (`mod rules;`)
- Test: `src-tauri/src/rules.rs` 내 tests

**Interfaces:**
- Consumes: `scanner::scan_dir` (크기 계산)
- Produces:
  - `rules::BaseDirs { temp: PathBuf, local_data: PathBuf, home: PathBuf }`
  - `rules::BaseDirs::from_env() -> Option<BaseDirs>` (환경변수 기반; 테스트는 직접 구성)
  - `rules::CacheCandidate { id: String, label: String, path: String, bytes: u64, exists: bool }` (serde::Serialize)
  - `rules::cache_candidates(bases: &BaseDirs) -> Vec<CacheCandidate>` — 카탈로그 순서 고정, 존재하지 않는 항목도 exists=false로 포함
  - `rules::clean_targets(dir: &Path) -> Vec<PathBuf>` — 규칙 디렉토리의 **직계 자식** 목록 (캐시 디렉토리 자체는 남기고 내용물만 비우기 위함)

- [ ] **Step 1: 실패하는 테스트 작성**

`src-tauri/src/rules.rs` 생성, 하단에:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fake_bases(root: &std::path::Path) -> BaseDirs {
        BaseDirs {
            temp: root.join("tmp"),
            local_data: root.join("local"),
            home: root.join("home"),
        }
    }

    #[test]
    fn catalog_reports_sizes_and_existence() {
        let tmp = tempfile::tempdir().unwrap();
        let bases = fake_bases(tmp.path());
        // npm 캐시만 실제로 만들어 둔다
        let npm = bases.local_data.join("npm-cache");
        fs::create_dir_all(&npm).unwrap();
        fs::write(npm.join("blob.bin"), vec![0u8; 128]).unwrap();

        let cands = cache_candidates(&bases);

        let npm_c = cands.iter().find(|c| c.id == "npm-cache").unwrap();
        assert!(npm_c.exists);
        assert_eq!(npm_c.bytes, 128);
        let temp_c = cands.iter().find(|c| c.id == "os-temp").unwrap();
        assert!(!temp_c.exists);
        assert_eq!(temp_c.bytes, 0);
        // 카탈로그에 최소 4개 규칙
        assert!(cands.len() >= 4);
    }

    #[test]
    fn clean_targets_lists_immediate_children_only() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("a")).unwrap();
        fs::write(tmp.path().join("a").join("deep.bin"), b"x").unwrap();
        fs::write(tmp.path().join("b.bin"), b"y").unwrap();

        let mut names: Vec<String> = clean_targets(tmp.path())
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, vec!["a", "b.bin"]);
    }

    #[test]
    fn clean_targets_missing_dir_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(clean_targets(&tmp.path().join("nope")).is_empty());
    }
}
```

- [ ] **Step 2: 실패 확인**

`lib.rs`에 `mod rules;` 추가 후:

Run: `cd src-tauri; cargo test rules`
Expected: COMPILE ERROR

- [ ] **Step 3: 구현**

`src-tauri/src/rules.rs` 상단:

```rust
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::scanner;

pub struct BaseDirs {
    pub temp: PathBuf,
    pub local_data: PathBuf,
    pub home: PathBuf,
}

impl BaseDirs {
    pub fn from_env() -> Option<BaseDirs> {
        let home = std::env::var(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).ok()?;
        let home = PathBuf::from(home);
        let temp = std::env::temp_dir();
        let local_data = if cfg!(windows) {
            std::env::var("LOCALAPPDATA").map(PathBuf::from).ok()?
        } else {
            home.join(".cache")
        };
        Some(BaseDirs { temp, local_data, home })
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CacheCandidate {
    pub id: String,
    pub label: String,
    pub path: String,
    pub bytes: u64,
    pub exists: bool,
}

/// 정적 캐시 카탈로그 (스펙 §4 rules). 항목 = (id, 라벨, 베이스 기준 상대경로).
/// ponytail: 브라우저 캐시는 프로필 글롭이 필요해 M2 범위 밖 — 카탈로그에 추가만 하면 확장됨
fn catalog(bases: &BaseDirs) -> Vec<(&'static str, &'static str, PathBuf)> {
    vec![
        ("os-temp", "OS 임시 폴더", bases.temp.clone()),
        ("npm-cache", "npm 캐시", bases.local_data.join("npm-cache")),
        ("pip-cache", "pip 캐시", if cfg!(windows) {
            bases.local_data.join("pip").join("cache")
        } else {
            bases.local_data.join("pip")
        }),
        ("cargo-registry-cache", "cargo 레지스트리 캐시",
            bases.home.join(".cargo").join("registry").join("cache")),
    ]
}

pub fn cache_candidates(bases: &BaseDirs) -> Vec<CacheCandidate> {
    catalog(bases)
        .into_iter()
        .map(|(id, label, path)| {
            let exists = path.is_dir();
            let bytes = if exists {
                scanner::scan_dir(&path, &AtomicBool::new(false), |_| {}).stats.bytes
            } else {
                0
            };
            CacheCandidate {
                id: id.into(),
                label: label.into(),
                path: path.to_string_lossy().into_owned(),
                bytes,
                exists,
            }
        })
        .collect()
}

/// 캐시 디렉토리 자체는 보존하고 내용물만 비우기 위한 직계 자식 열거
pub fn clean_targets(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    rd.filter_map(|e| e.ok().map(|e| e.path())).collect()
}
```

주의: 테스트의 `|_| {}` 클로저는 M1에서 확립한 공유 `noop` 패턴을 따라도 된다 — scanner.rs tests의 `fn noop(_: &ScanStats) {}`를 pub(crate)로 승격해 재사용하거나, 이 파일에 동일한 헬퍼를 둔다 (커버리지에서 죽은 클로저가 남지 않게).

- [ ] **Step 4: 통과 확인 + Commit**

Run: `cd src-tauri; cargo test rules`
Expected: 3 tests PASS

```powershell
git add src-tauri
git commit -m "feat(rules): static cache catalog with injected base dirs

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 5: `dev_artifacts` — 오래된 개발 아티팩트 탐지

**Files:**
- Create: `src-tauri/src/dev_artifacts.rs`
- Modify: `src-tauri/src/lib.rs` (`mod dev_artifacts;`)
- Test: 같은 파일 tests

**Interfaces:**
- Consumes: `scanner::scan_dir`
- Produces:
  - `dev_artifacts::DevArtifact { path: String, kind: String, project: String, bytes: u64, age_days: u64 }` (serde::Serialize)
  - `dev_artifacts::find_artifacts(root: &Path, min_age_days: u64, now_ms: u64) -> Vec<DevArtifact>` — 마커 인접 검증 + mtime 나이 필터, 크기 내림차순

- [ ] **Step 1: 실패하는 테스트 작성**

`src-tauri/src/dev_artifacts.rs` 생성, 하단에:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn project(root: &std::path::Path, name: &str, marker: &str, artifact: &str) -> std::path::PathBuf {
        let p = root.join(name);
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join(marker), b"{}").unwrap();
        let a = p.join(artifact);
        fs::create_dir_all(&a).unwrap();
        fs::write(a.join("payload.bin"), vec![0u8; 256]).unwrap();
        a
    }

    #[test]
    fn finds_marker_adjacent_artifacts() {
        let tmp = tempfile::tempdir().unwrap();
        project(tmp.path(), "webapp", "package.json", "node_modules");
        project(tmp.path(), "cli", "Cargo.toml", "target");
        // 마커 없는 가짜 — 탐지되면 안 됨
        let orphan = tmp.path().join("random").join("node_modules");
        fs::create_dir_all(&orphan).unwrap();

        let found = find_artifacts(tmp.path(), 0, u64::MAX);

        let kinds: Vec<&str> = found.iter().map(|a| a.kind.as_str()).collect();
        assert!(kinds.contains(&"node_modules"));
        assert!(kinds.contains(&"target"));
        assert!(
            !found.iter().any(|a| a.path.contains("random")),
            "마커 없는 아티팩트는 제외"
        );
        let nm = found.iter().find(|a| a.kind == "node_modules").unwrap();
        assert_eq!(nm.project, "webapp");
        assert_eq!(nm.bytes, 256);
    }

    #[test]
    fn respects_min_age() {
        let tmp = tempfile::tempdir().unwrap();
        project(tmp.path(), "fresh", "package.json", "node_modules");
        // 방금 만든 것: min_age_days=30이면 제외 (now = 실제 현재로는 나이가 0)
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        assert!(find_artifacts(tmp.path(), 30, now_ms).is_empty());
        // min_age_days=0이면 포함
        assert_eq!(find_artifacts(tmp.path(), 0, now_ms).len(), 1);
    }

    #[test]
    fn artifacts_inside_artifacts_are_not_double_counted() {
        let tmp = tempfile::tempdir().unwrap();
        let nm = project(tmp.path(), "app", "package.json", "node_modules");
        // node_modules 내부의 중첩 node_modules — 별도 항목이면 안 됨
        let nested = nm.join("dep").join("node_modules");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nm.join("dep").join("package.json"), b"{}").unwrap();

        assert_eq!(find_artifacts(tmp.path(), 0, u64::MAX).len(), 1);
    }
}
```

- [ ] **Step 2: 실패 확인**

`lib.rs`에 `mod dev_artifacts;` 추가 후:

Run: `cd src-tauri; cargo test dev_artifacts`
Expected: COMPILE ERROR

- [ ] **Step 3: 구현**

`src-tauri/src/dev_artifacts.rs` 상단:

```rust
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::scanner;

#[derive(Debug, Clone, serde::Serialize)]
pub struct DevArtifact {
    pub path: String,
    pub kind: String,
    pub project: String,
    pub bytes: u64,
    pub age_days: u64,
}

/// (아티팩트 디렉토리명, 같은 부모에 있어야 하는 프로젝트 마커들)
const ARTIFACT_KINDS: &[(&str, &[&str])] = &[
    ("node_modules", &["package.json"]),
    ("target", &["Cargo.toml"]),
    (".venv", &["pyproject.toml", "requirements.txt", "setup.py"]),
    ("venv", &["pyproject.toml", "requirements.txt", "setup.py"]),
    ("__pycache__", &[]), // 마커 불필요 — 이름 자체가 파이썬 캐시
];

fn artifact_kind(name: &str) -> Option<&'static (&'static str, &'static [&'static str])> {
    ARTIFACT_KINDS.iter().find(|(k, _)| *k == name)
}

fn age_days(path: &Path, now_ms: u64) -> u64 {
    let Ok(md) = path.metadata() else { return 0 };
    let Ok(mtime) = md.modified() else { return 0 };
    let Ok(dur) = mtime.duration_since(std::time::UNIX_EPOCH) else { return 0 };
    let mtime_ms = dur.as_millis() as u64;
    now_ms.saturating_sub(mtime_ms) / 86_400_000
}

pub fn find_artifacts(root: &Path, min_age_days: u64, now_ms: u64) -> Vec<DevArtifact> {
    let mut found = Vec::new();
    // 중첩 중복 방지는 skip_prefixes가 담당 — walker는 단순하게 유지
    let walker = jwalk::WalkDir::new(root)
        .follow_links(false)
        .skip_hidden(false);
    let mut skip_prefixes: Vec<PathBuf> = Vec::new();
    for entry in walker {
        let Ok(e) = entry else { continue };
        if !e.file_type().is_dir() {
            continue;
        }
        let path = e.path();
        if skip_prefixes.iter().any(|p| path.starts_with(p) && path != *p) {
            continue;
        }
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
        let Some((kind, markers)) = artifact_kind(&name) else { continue };
        let parent = path.parent().unwrap_or(root);
        let marker_ok =
            markers.is_empty() || markers.iter().any(|m| parent.join(m).exists());
        if !marker_ok {
            continue;
        }
        skip_prefixes.push(path.clone());
        let age = if now_ms == u64::MAX { u64::MAX } else { age_days(&path, now_ms) };
        if age < min_age_days {
            continue;
        }
        let bytes = scanner::scan_dir(&path, &AtomicBool::new(false), |_| {}).stats.bytes;
        found.push(DevArtifact {
            path: path.to_string_lossy().into_owned(),
            kind: kind.to_string(),
            project: parent
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            bytes,
            age_days: if age == u64::MAX { 0 } else { age },
        });
    }
    found.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    found
}
```

주의(구현자): jwalk의 병렬 순회는 순서를 보장하지 않으므로 `skip_prefixes` 필터가 완벽하지 않을 수 있다(부모 아티팩트보다 자식이 먼저 나올 수 있음). 테스트 `artifacts_inside_artifacts_are_not_double_counted`가 실패하면 두-패스로 전환하라: 1패스에서 모든 아티팩트 경로를 모으고, 2패스에서 다른 아티팩트의 하위인 것을 제거한 뒤 크기를 계산한다. 이 경우에도 Produces 시그니처는 유지.

- [ ] **Step 4: 통과 확인 + Commit**

Run: `cd src-tauri; cargo test dev_artifacts`
Expected: 3 tests PASS

```powershell
git add src-tauri
git commit -m "feat(dev-artifacts): marker-adjacent stale artifact detection

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 6: 커맨드 계층 — 정리 IPC

**Files:**
- Modify: `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`
- Test: commands.rs tests 모듈

**Interfaces:**
- Consumes: Tasks 1-5 전부
- Produces (프론트 계약 — Task 7이 그대로 사용):
  - command `list_cache_candidates() -> Result<Vec<CacheCandidate>, String>`
  - command `list_dev_artifacts(root: String, min_age_days: u64) -> Result<Vec<DevArtifact>, String>`
  - command `clean_paths(paths: Vec<String>) -> Vec<CleanResult>` — 항목별 결과, 실패해도 전체는 계속
  - command `recent_operations(limit: usize) -> Vec<JournalEntry>`
  - `CleanResult { path: String, ok: bool, error: String }` (serde::Serialize)
  - 순수 함수 `clean_paths_inner(paths: &[PathBuf], journal_path: &Path, now_ms: u64) -> Vec<CleanResult>` (테스트 대상)
  - `journal_file_path()` — 래퍼 전용(앱 데이터 디렉토리), `#[cfg(not(coverage))]`

- [ ] **Step 1: 실패하는 테스트 작성**

commands.rs tests에 추가:

```rust
    #[test]
    fn clean_paths_inner_reports_per_item_results() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let ok_file = tmp.path().join("disksage-clean-fixture.bin");
        fs::write(&ok_file, vec![0u8; 32]).unwrap();
        let missing = tmp.path().join("ghost");
        let protected = std::path::PathBuf::from(if cfg!(windows) { "C:\\Windows" } else { "/usr" });

        let results = clean_paths_inner(&[ok_file.clone(), missing, protected], &jp, 7);

        assert_eq!(results.len(), 3);
        assert!(results[0].ok);
        assert!(!results[1].ok && results[1].error.contains("휴지통"));
        assert!(!results[2].ok && results[2].error.contains("보호"));
        assert!(!ok_file.exists());

        // 테스트 픽스처 휴지통 정리 (win/linux)
        #[cfg(any(windows, target_os = "linux"))]
        {
            let items: Vec<_> = trash::os_limited::list()
                .unwrap()
                .into_iter()
                .filter(|i| i.name.to_string_lossy().contains("disksage-clean-fixture"))
                .collect();
            trash::os_limited::purge_all(items).unwrap();
        }
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cd src-tauri; cargo test commands`
Expected: COMPILE ERROR

- [ ] **Step 3: 구현**

commands.rs에 추가 (기존 imports 옆에 `use crate::{dev_artifacts, rules, safety};` — cfg 주의: 순수 함수가 쓰는 것은 무조건 import, 래퍼 전용은 `#[cfg(not(coverage))]` 붙은 use로):

```rust
#[derive(serde::Serialize)]
pub struct CleanResult {
    pub path: String,
    pub ok: bool,
    pub error: String,
}

/// 정리 실행의 순수 코어 — 결과는 항목별, 하나가 실패해도 나머지는 진행 (스펙 §8)
pub fn clean_paths_inner(
    paths: &[PathBuf],
    journal_path: &Path,
    now_ms: u64,
) -> Vec<CleanResult> {
    paths
        .iter()
        .map(|p| {
            let bytes = p.metadata().map(|m| m.len()).unwrap_or(0);
            match safety::trash_delete(p, bytes, journal_path, now_ms) {
                Ok(()) => CleanResult {
                    path: p.to_string_lossy().into_owned(),
                    ok: true,
                    error: String::new(),
                },
                Err(e) => CleanResult {
                    path: p.to_string_lossy().into_owned(),
                    ok: false,
                    error: e.to_string(),
                },
            }
        })
        .collect()
}

#[cfg(not(coverage))]
fn journal_file_path(app: &AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("journal.jsonl"))
}

#[cfg(not(coverage))]
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn list_cache_candidates() -> Result<Vec<rules::CacheCandidate>, String> {
    let bases = rules::BaseDirs::from_env().ok_or("환경변수에서 기본 경로를 찾지 못함")?;
    Ok(rules::cache_candidates(&bases))
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn list_dev_artifacts(
    root: String,
    min_age_days: u64,
) -> Result<Vec<dev_artifacts::DevArtifact>, String> {
    Ok(dev_artifacts::find_artifacts(Path::new(&root), min_age_days, now_ms()))
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn clean_paths(paths: Vec<String>, app: AppHandle) -> Result<Vec<CleanResult>, String> {
    let jp = journal_file_path(&app)?;
    let pbufs: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    Ok(clean_paths_inner(&pbufs, &jp, now_ms()))
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn recent_operations(limit: usize, app: AppHandle) -> Result<Vec<safety::JournalEntry>, String> {
    Ok(safety::journal_recent(&journal_file_path(&app)?, limit))
}
```

캐시 규칙의 "내용물만 비우기": 프론트가 규칙 선택 시 `rules::clean_targets`를 노출하는 커맨드가 필요하다 — 추가:

```rust
#[cfg(not(coverage))]
#[tauri::command]
pub fn expand_clean_targets(dir: String) -> Vec<String> {
    rules::clean_targets(Path::new(&dir))
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}
```

`lib.rs`의 `invoke_handler`에 5개 커맨드 추가 (run()은 이미 `#[cfg(not(coverage))]`):
`commands::list_cache_candidates, commands::list_dev_artifacts, commands::clean_paths, commands::recent_operations, commands::expand_clean_targets`

- [ ] **Step 4: 전체 테스트 + 양쪽 cargo check**

Run: `cd src-tauri; cargo test`
Expected: 기존 + 신규 전체 PASS

Run (bash): `RUSTFLAGS="--cfg coverage" cargo check`
Expected: 경고 0 (dead_code가 나면 M1 패턴대로 모듈 수준 `cfg_attr(coverage, allow(dead_code))` 확인)

- [ ] **Step 5: Commit**

```powershell
git add src-tauri
git commit -m "feat(commands): cleanup IPC over the safety layer

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 7: 프론트엔드 — 정리 UI

**Files:**
- Create: `src/lib/Cleanup.svelte`
- Modify: `src/lib/api.ts`, `src/routes/+page.svelte`

**Interfaces:**
- Consumes: Task 6 커맨드 계약, `fmtBytes`
- Produces: api.ts 함수 `listCacheCandidates, listDevArtifacts, cleanPaths, expandCleanTargets, recentOperations` + 타입 `CacheCandidate, DevArtifact, CleanResult, JournalEntry`; `Cleanup.svelte` props `{ scannedRoot: string | null }`

- [ ] **Step 1: api.ts 추가**

`src/lib/api.ts`에 추가:

```typescript
export interface CacheCandidate {
  id: string;
  label: string;
  path: string;
  bytes: number;
  exists: boolean;
}
export interface DevArtifact {
  path: string;
  kind: string;
  project: string;
  bytes: number;
  age_days: number;
}
export interface CleanResult {
  path: string;
  ok: boolean;
  error: string;
}
export interface JournalEntry {
  ts_ms: number;
  op: string;
  path: string;
  bytes: number;
  outcome: string;
}

export const listCacheCandidates = () => invoke<CacheCandidate[]>("list_cache_candidates");
export const listDevArtifacts = (root: string, minAgeDays = 30) =>
  invoke<DevArtifact[]>("list_dev_artifacts", { root, minAgeDays });
export const cleanPaths = (paths: string[]) => invoke<CleanResult[]>("clean_paths", { paths });
export const expandCleanTargets = (dir: string) =>
  invoke<string[]>("expand_clean_targets", { dir });
export const recentOperations = (limit = 20) =>
  invoke<JournalEntry[]>("recent_operations", { limit });
```

(주의: Tauri 2는 JS 카멜케이스 인자 `minAgeDays`를 Rust `min_age_days`로 자동 매핑한다 — M1의 `top_files(limit)`와 동일 규칙.)

- [ ] **Step 2: Cleanup.svelte 작성**

`src/lib/Cleanup.svelte`:

```svelte
<script lang="ts">
  import * as api from "./api";
  import { fmtBytes } from "./fmt";

  let { scannedRoot }: { scannedRoot: string | null } = $props();

  let caches: api.CacheCandidate[] = $state([]);
  let artifacts: api.DevArtifact[] = $state([]);
  let selected: Set<string> = $state(new Set());
  let selectedRules: Set<string> = $state(new Set());
  let results: api.CleanResult[] = $state([]);
  let busy = $state(false);
  let loadError = $state("");

  async function load() {
    loadError = "";
    try {
      caches = await api.listCacheCandidates();
      artifacts = scannedRoot ? await api.listDevArtifacts(scannedRoot) : [];
    } catch (e) {
      loadError = String(e);
    }
  }

  function toggle(set: Set<string>, key: string) {
    const next = new Set(set);
    next.has(key) ? next.delete(key) : next.add(key);
    return next;
  }

  let totalSelected = $derived(
    caches.filter((c) => selectedRules.has(c.id)).reduce((s, c) => s + c.bytes, 0) +
      artifacts.filter((a) => selected.has(a.path)).reduce((s, a) => s + a.bytes, 0),
  );

  async function executeClean() {
    // 검토·확인 (스펙 §7-6): 명시적 승인 없이는 아무것도 실행되지 않는다
    const ruleDirs = caches.filter((c) => selectedRules.has(c.id) && c.exists);
    const artifactPaths = artifacts.filter((a) => selected.has(a.path)).map((a) => a.path);
    const summary = [
      ...ruleDirs.map((c) => `${c.label} (${fmtBytes(c.bytes)}) — 내용물 비우기`),
      ...artifactPaths,
    ];
    if (summary.length === 0) return;
    const okay = confirm(
      `다음 ${summary.length}개 항목을 휴지통으로 보냅니다 (총 ${fmtBytes(totalSelected)}):\n\n` +
        summary.slice(0, 15).join("\n") +
        (summary.length > 15 ? `\n… 외 ${summary.length - 15}개` : "") +
        "\n\n휴지통에서 언제든 복원할 수 있습니다.",
    );
    if (!okay) return;

    busy = true;
    try {
      const paths: string[] = [...artifactPaths];
      for (const c of ruleDirs) {
        paths.push(...(await api.expandCleanTargets(c.path)));
      }
      results = await api.cleanPaths(paths);
      selected = new Set();
      selectedRules = new Set();
      await load();
    } catch (e) {
      loadError = String(e);
    } finally {
      busy = false;
    }
  }

  let failedResults = $derived(results.filter((r) => !r.ok));
</script>

<section>
  <h2>정리 <button onclick={load} disabled={busy}>새로고침</button></h2>
  {#if loadError}<p class="error">{loadError}</p>{/if}

  <h3>캐시</h3>
  <ul class="list">
    {#each caches as c (c.id)}
      <li>
        <label class:disabled={!c.exists}>
          <input
            type="checkbox"
            disabled={!c.exists || busy}
            checked={selectedRules.has(c.id)}
            onchange={() => (selectedRules = toggle(selectedRules, c.id))}
          />
          {c.label}
          <span class="size">{c.exists ? fmtBytes(c.bytes) : "없음"}</span>
        </label>
        <span class="path" title={c.path}>{c.path}</span>
      </li>
    {/each}
  </ul>

  <h3>오래된 개발 아티팩트 {scannedRoot ? `(${scannedRoot}, 30일+)` : "(먼저 스캔하세요)"}</h3>
  <ul class="list">
    {#each artifacts as a (a.path)}
      <li>
        <label>
          <input
            type="checkbox"
            disabled={busy}
            checked={selected.has(a.path)}
            onchange={() => (selected = toggle(selected, a.path))}
          />
          {a.kind} <em>({a.project}, {a.age_days}일)</em>
          <span class="size">{fmtBytes(a.bytes)}</span>
        </label>
        <span class="path" title={a.path}>{a.path}</span>
      </li>
    {/each}
  </ul>

  <div class="actions">
    <button onclick={executeClean} disabled={busy || totalSelected === 0}>
      {busy ? "정리 중…" : `선택 항목 휴지통으로 (${fmtBytes(totalSelected)})`}
    </button>
  </div>

  {#if results.length > 0}
    <p>
      {results.filter((r) => r.ok).length}/{results.length}개 휴지통으로 이동 완료 —
      휴지통에서 복원할 수 있습니다.
    </p>
    {#if failedResults.length > 0}
      <ul class="errors">
        {#each failedResults as r (r.path)}
          <li title={r.path}>⚠ {r.path} — {r.error}</li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>

<style>
  section { margin-top: 1.5rem; border-top: 1px solid #ddd; padding-top: 1rem; }
  h2 { display: flex; gap: 0.75rem; align-items: center; }
  .list { list-style: none; padding: 0; max-height: 30vh; overflow-y: auto; }
  .list li { display: flex; justify-content: space-between; gap: 1rem; padding: 2px 0; }
  .size { color: #666; font-variant-numeric: tabular-nums; margin-left: 0.5rem; }
  .path { color: #999; font-size: 0.8rem; overflow-wrap: anywhere; text-align: right; }
  .disabled { color: #aaa; }
  .error, .errors { color: #b00; }
  .errors { font-size: 0.85rem; }
</style>
```

- [ ] **Step 3: +page.svelte 연결**

script에 추가:

```typescript
  import Cleanup from "$lib/Cleanup.svelte";
```

`{#if top.length > 0} <TopFiles files={top} /> {/if}` 바로 아래에 추가:

```svelte
  <Cleanup scannedRoot={crumbs.length > 0 ? crumbs[0] : null} />
```

Cleanup의 초기 로드는 사용자가 "새로고침"을 누를 때만 일어난다 — 캐시 크기 계산이 스캔급 비용이므로 onMount 자동 로드는 하지 않는다 (의도된 설계).

- [ ] **Step 4: 게이트 + 수동 확인 생략 규칙**

Run: `npm run build`, `npm run check`, `npm test`, `npm run coverage`
Expected: 모두 클린, 커버리지 100 유지 (Cleanup.svelte는 coverage.include 밖 — 순수 .ts를 새로 만들지 않았으므로 vitest.config.ts 변경 없음)

GUI 확인(`npm run tauri dev`)은 서브에이전트 환경에서 불가 — 사람 검증 체크리스트로 이월.

- [ ] **Step 5: Commit**

```powershell
git add src
git commit -m "feat(ui): cleanup panel with review-and-confirm trash flow

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 8: 커버리지 정합 패스

**Files:**
- Modify: 필요한 곳 (측정 결과에 따라 테스트 추가)

**Interfaces:**
- Consumes: Tasks 1-7 전체
- Produces: 리눅스 게이트 기준 100% 라인 커버리지

- [ ] **Step 1: 로컬 측정**

Run (bash, src-tauri): `cargo llvm-cov --all-features --fail-under-lines 100 --show-missing-lines`

- [ ] **Step 2: 갭 분석 및 폐쇄**

허용되는 로컬 미커버: ① cfg(unix) 전용 테스트만이 커버하는 arm ② keep_entry의 cfg(windows) reparse 블록 ③ cfg(windows) 전용 경로(리눅스 게이트엔 없음). 그 외 모든 미커버 라인은 테스트를 추가해 닫는다. 특히:
- `SafetyError::Display`의 세 arm — 세 에러 케이스 테스트가 이미 `.to_string()`을 거치면 커버됨; 아니면 `assert!(err.to_string().contains(...))` 추가
- `rules::BaseDirs::from_env` — 환경변수 기반이라 리눅스 러너에서 실행 가능: `assert!(BaseDirs::from_env().is_some())` 테스트 추가 (HOME 설정된 러너)
- `dev_artifacts::age_days`의 metadata-실패 arm — 리눅스에서 존재하지 않는 경로로 `age_days` 간접 실행이 안 되면 한 줄 let-else로 재구성
- 신규 `#[cfg(not(coverage))]` 항목이 리눅스 커버리지 빌드에서 완전히 사라지는지 `RUSTFLAGS="--cfg coverage" cargo check`로 재확인

- [ ] **Step 3: Commit**

```powershell
git add src-tauri
git commit -m "test(coverage): close m2 line-coverage gaps for the linux gate

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 9: 최종 검증 + PR

**Files:** 없음 (검증·푸시만)

**Interfaces:**
- Consumes: Tasks 1-8
- Produces: `ContextualWisdomLab/disksage`에 M2 PR

- [ ] **Step 1: 전체 게이트**

Run: `cd src-tauri; cargo test` → 전체 PASS
Run: `npm test; npm run coverage; npm run build; npm run check` → 전체 클린
Run (bash): `RUSTFLAGS="--cfg coverage" cargo check` → 경고 0

- [ ] **Step 2: 푸시 + PR**

```powershell
git push -u origin feat/m2-cleaners-safety
gh pr create --repo ContextualWisdomLab/disksage --base main --head feat/m2-cleaners-safety --title "feat: M2 cache/dev-artifact cleanup over a journaled safety layer" --body "M2 milestone per docs/superpowers/specs/2026-07-10-disksage-design.md: trash-only safety layer (protected-path denylist, JSONL journal written before every operation), static cache catalog with injected base dirs, marker-adjacent stale dev-artifact detection, review-and-confirm cleanup UI. No permanent-delete code path exists; failures are reported per item and never fall back to permanent deletion.

🤖 Generated with [Claude Code](https://claude.com/claude-code)"
```

Expected: PR URL. 이후 조직 파이프라인 (리뷰봇 → 승인 → auto-merge). 스테일 리뷰가 쌓이면 죽은 헤드 리뷰는 dismiss, 스케줄러가 잠들면 `gh pr merge --auto --squash`.

- [ ] **Step 3: 사람 검증 체크리스트 (PR 본문 아님 — 사용자 전달용)**

1. `npm run tauri dev` → 스캔 후 "정리" 섹션 → 새로고침 → 캐시 카탈로그에 실제 크기 표시
2. 임시 폴더에 테스트 프로젝트(`mkdir t; cd t; echo {} > package.json; mkdir node_modules`) 만들고 해당 루트 스캔 → 아티팩트 목록에 등장(30일 필터 때문에 기본적으로는 안 보임 — min_age_days=0 확인용 임시 조정 또는 오래된 실제 프로젝트 사용)
3. 선택 → 확인 다이얼로그 → 실행 → 휴지통에 실제로 들어갔는지 확인 → 복원해보기
4. C:\Windows 같은 보호 경로를 개발자 도구로 강제 호출 시 거부되는지 (safety 계층 방어 확인)
