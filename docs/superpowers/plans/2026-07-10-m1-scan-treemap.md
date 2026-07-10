# DiskSage M1: Scan + Treemap/Large-File View Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tauri 2 데스크톱 앱에서 드라이브를 병렬 스캔해 디렉토리 용량 트리맵과 대용량 파일 목록으로 시각화하고 드릴다운할 수 있게 한다.

**Architecture:** Rust 백엔드의 `scanner` 모듈이 jwalk로 병렬 순회하며 디렉토리 크기 맵 + 상위 파일 힙 + 통계만 메모리에 유지한다(파일 단위 트리는 저장하지 않음). 프론트엔드(Svelte 5)는 Tauri command로 레벨 단위 조회(`get_node`)를 하고 이벤트로 진행률을 받는다. 트리맵은 squarified 알고리즘을 canvas에 직접 렌더링한다.

**Tech Stack:** Tauri 2, Rust (jwalk, serde, tempfile[dev]), Svelte 5 + TypeScript + Vite, vitest[dev]

## Global Constraints

- 스펙: `docs/superpowers/specs/2026-07-10-disksage-design.md` — 충돌 시 스펙이 우선
- 심링크/정션(reparse point)은 **절대 따라가지 않는다**
- 스캔은 개별 항목 에러로 **절대 중단되지 않는다** — 건너뛰고 `skipped` 집계
- M1에는 삭제/이동 코드가 일절 없다 (읽기 전용 마일스톤)
- 앱 identifier: `com.contextualwisdomlab.disksage`, productName: `DiskSage`
- **원격 main 직접 push 불가** (조직 룰셋) — 모든 작업은 `feat/m1-scan-treemap` 브랜치에서 커밋하고 마지막에 PR
- 커밋 메시지는 conventional commits + 트레일러 `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`
- Rust 명령은 `src-tauri/`에서 실행, npm 명령은 저장소 루트에서 실행

---

### Task 1: Tauri 2 + Svelte 5 스캐폴드

**Files:**
- Create: 저장소 루트에 Tauri 템플릿 전체 (`src/`, `src-tauri/`, `package.json`, `vite.config.ts` 등)
- Modify: `src-tauri/tauri.conf.json` (identifier, productName)

**Interfaces:**
- Consumes: 없음 (최초 태스크)
- Produces: `npm run tauri dev`로 실행되는 빈 앱. 이후 모든 태스크의 뼈대

- [ ] **Step 1: 작업 브랜치 생성**

```powershell
git checkout -b feat/m1-scan-treemap
```

- [ ] **Step 2: 템플릿 스캐폴드 (비어있지 않은 디렉토리이므로 임시 폴더 경유)**

```powershell
npm create tauri-app@latest scaffold -- --manager npm --template svelte-ts --yes
Remove-Item scaffold\README.md
Get-ChildItem scaffold -Force | Move-Item -Destination .
Remove-Item scaffold
npm install
```

- [ ] **Step 3: 앱 아이덴티티 설정**

`src-tauri/tauri.conf.json`에서 다음 두 키를 수정:

```json
{
  "productName": "DiskSage",
  "identifier": "com.contextualwisdomlab.disksage"
}
```

- [ ] **Step 4: 빌드 확인**

Run: `npm run tauri dev`
Expected: 템플릿 기본 창이 뜬다. 확인 후 종료.

Run: `cd src-tauri; cargo test; cd ..`
Expected: 0 tests, PASS (컴파일 성공 확인)

- [ ] **Step 5: Commit**

```powershell
git add -A
git commit -m "feat: scaffold Tauri 2 + Svelte 5 app

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: `scanner` — 크기 집계 코어

**Files:**
- Create: `src-tauri/src/scanner.rs`
- Modify: `src-tauri/src/lib.rs` (`mod scanner;` 추가)
- Modify: `src-tauri/Cargo.toml` (jwalk, tempfile 추가)
- Test: `src-tauri/src/scanner.rs` 내 `#[cfg(test)] mod tests`

**Interfaces:**
- Consumes: 없음
- Produces:
  - `scanner::ScanStats { files: u64, dirs: u64, skipped: u64, bytes: u64 }` (derive `Debug, Clone, Default, serde::Serialize`)
  - `scanner::ScanResult { root: PathBuf, dir_sizes: HashMap<PathBuf, u64>, top_files: Vec<(PathBuf, u64)>, stats: ScanStats, cancelled: bool }`
  - `scanner::scan_dir(root: &Path, cancel: &AtomicBool, on_progress: impl FnMut(&ScanStats)) -> ScanResult`
  - `scanner::TOP_FILES_CAP: usize = 1000`

- [ ] **Step 1: 의존성 추가**

```powershell
cd src-tauri
cargo add jwalk@0.8
cargo add --dev tempfile
cd ..
```

- [ ] **Step 2: 실패하는 테스트 작성**

`src-tauri/src/scanner.rs` 생성, 파일 하단에:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::sync::atomic::AtomicBool;

    fn write(p: &Path, bytes: usize) {
        fs::write(p, vec![0u8; bytes]).unwrap();
    }

    #[test]
    fn aggregates_dir_sizes_up_the_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("a").join("b")).unwrap();
        write(&root.join("a").join("one.bin"), 100);
        write(&root.join("a").join("b").join("two.bin"), 50);
        write(&root.join("three.bin"), 7);

        let res = scan_dir(root, &AtomicBool::new(false), |_| {});

        assert_eq!(res.stats.files, 3);
        assert_eq!(res.stats.bytes, 157);
        assert!(!res.cancelled);
        assert_eq!(res.dir_sizes[&root.to_path_buf()], 157);
        assert_eq!(res.dir_sizes[&root.join("a")], 150);
        assert_eq!(res.dir_sizes[&root.join("a").join("b")], 50);
    }

    #[test]
    fn top_files_sorted_desc() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join("small.bin"), 10);
        write(&root.join("big.bin"), 300);
        write(&root.join("mid.bin"), 100);

        let res = scan_dir(root, &AtomicBool::new(false), |_| {});

        let names: Vec<String> = res
            .top_files
            .iter()
            .map(|(p, _)| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["big.bin", "mid.bin", "small.bin"]);
        assert_eq!(res.top_files[0].1, 300);
    }
}
```

파일 상단에는 시그니처만 있는 빈 구현을 두지 말고, 아직 아무것도 작성하지 않는다 (컴파일 에러가 곧 "실패하는 테스트").

- [ ] **Step 3: 테스트 실패 확인**

`src-tauri/src/lib.rs` 최상단에 `mod scanner;` 추가 후:

Run: `cd src-tauri; cargo test scanner; cd ..`
Expected: COMPILE ERROR — `scan_dir` not found

- [ ] **Step 4: 최소 구현 작성**

`src-tauri/src/scanner.rs` 상단에 (테스트 모듈 위):

```rust
use std::collections::{BinaryHeap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ScanStats {
    pub files: u64,
    pub dirs: u64,
    pub skipped: u64,
    pub bytes: u64,
}

pub struct ScanResult {
    pub root: PathBuf,
    pub dir_sizes: HashMap<PathBuf, u64>,
    /// 내림차순 정렬, TOP_FILES_CAP 개로 제한
    pub top_files: Vec<(PathBuf, u64)>,
    pub stats: ScanStats,
    pub cancelled: bool,
}

pub const TOP_FILES_CAP: usize = 1000;

pub fn scan_dir(
    root: &Path,
    cancel: &AtomicBool,
    on_progress: impl FnMut(&ScanStats),
) -> ScanResult {
    scan_dir_with_interval(root, cancel, 8192, on_progress)
}

/// ponytail: progress 간격을 파라미터로 뺀 것은 테스트 주입용, 외부 API는 scan_dir
pub fn scan_dir_with_interval(
    root: &Path,
    cancel: &AtomicBool,
    progress_every: u64,
    mut on_progress: impl FnMut(&ScanStats),
) -> ScanResult {
    let mut dir_sizes: HashMap<PathBuf, u64> = HashMap::new();
    // min-heap: 가장 작은 항목이 루트에 오도록 Reverse
    let mut top: BinaryHeap<std::cmp::Reverse<(u64, PathBuf)>> = BinaryHeap::new();
    let mut stats = ScanStats::default();
    let mut cancelled = false;
    let mut seen: u64 = 0;

    let walker = jwalk::WalkDir::new(root)
        .follow_links(false)
        .skip_hidden(false);

    for entry in walker {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        seen += 1;
        match entry {
            Ok(e) => {
                if e.file_type().is_dir() {
                    stats.dirs += 1;
                    dir_sizes.entry(e.path()).or_insert(0);
                } else if e.file_type().is_file() {
                    let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                    stats.files += 1;
                    stats.bytes += size;
                    top.push(std::cmp::Reverse((size, e.path())));
                    if top.len() > TOP_FILES_CAP {
                        top.pop();
                    }
                    // 파일 크기를 root까지의 모든 조상 디렉토리에 누적
                    // ponytail: PathBuf 키 HashMap — 초대형 드라이브에서 스캔이 수십 초를
                    // 넘기면 인터닝된 디렉토리 인덱스로 교체
                    let mut anc = e.path().parent().map(|p| p.to_path_buf());
                    while let Some(d) = anc {
                        *dir_sizes.entry(d.clone()).or_insert(0) += size;
                        if d == root {
                            break;
                        }
                        anc = d.parent().map(|p| p.to_path_buf());
                    }
                }
            }
            Err(_) => stats.skipped += 1,
        }
        if seen % progress_every == 0 {
            on_progress(&stats);
        }
    }

    let mut top_files: Vec<(PathBuf, u64)> = top
        .into_iter()
        .map(|std::cmp::Reverse((size, path))| (path, size))
        .collect();
    top_files.sort_by(|a, b| b.1.cmp(&a.1));

    ScanResult {
        root: root.to_path_buf(),
        dir_sizes,
        top_files,
        stats,
        cancelled,
    }
}
```

주의: jwalk의 루트 엔트리 자신도 dir로 순회되므로 `dirs` 카운트에 루트가 포함된다 — 테스트는 files/bytes만 단언하므로 문제없다.

- [ ] **Step 5: 테스트 통과 확인**

Run: `cd src-tauri; cargo test scanner; cd ..`
Expected: `aggregates_dir_sizes_up_the_tree ... ok`, `top_files_sorted_desc ... ok`

- [ ] **Step 6: Commit**

```powershell
git add src-tauri
git commit -m "feat(scanner): parallel scan with dir-size aggregation and top-files heap

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: `scanner` — 진행률 콜백 + 취소

**Files:**
- Modify: `src-tauri/src/scanner.rs` (테스트 추가만 — 구현은 Task 2에 이미 포함됨)
- Test: 같은 파일 `tests` 모듈

**Interfaces:**
- Consumes: `scan_dir_with_interval` (Task 2)
- Produces: 검증된 취소/진행률 동작. 시그니처 변화 없음

- [ ] **Step 1: 실패하는(또는 즉시 검증하는) 테스트 작성**

`tests` 모듈에 추가:

```rust
    #[test]
    fn progress_callback_fires_at_interval() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        for i in 0..10 {
            write(&root.join(format!("f{i}.bin")), 1);
        }
        let mut calls = 0;
        scan_dir_with_interval(root, &AtomicBool::new(false), 3, |_| calls += 1);
        // 루트 dir + 10 files = 11 entries → 간격 3이면 최소 3회
        assert!(calls >= 3, "expected >=3 progress calls, got {calls}");
    }

    #[test]
    fn cancel_stops_scan_early() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        for i in 0..50 {
            write(&root.join(format!("f{i}.bin")), 1);
        }
        let cancel = AtomicBool::new(true); // 시작 전부터 취소됨
        let res = scan_dir(root, &cancel, |_| {});
        assert!(res.cancelled);
        assert!(res.stats.files < 50);
    }
```

- [ ] **Step 2: 테스트 실행**

Run: `cd src-tauri; cargo test scanner; cd ..`
Expected: 4 tests PASS (Task 2 구현이 이미 커버함 — 실패 시 구현 버그이므로 수정)

- [ ] **Step 3: Commit**

```powershell
git add src-tauri
git commit -m "test(scanner): cover progress interval and cancellation

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 4: `scanner` — 심링크/reparse point 미추적

**Files:**
- Modify: `src-tauri/src/scanner.rs`
- Test: 같은 파일 `tests` 모듈

**Interfaces:**
- Consumes: Task 2의 walker 구성부
- Produces: 심링크·정션이 결과에서 완전히 제외되는 `scan_dir`. 시그니처 변화 없음

- [ ] **Step 1: 실패하는 테스트 작성 (unix에서 검증, Windows 정션은 수동 확인)**

`tests` 모듈에 추가:

```rust
    #[cfg(unix)]
    #[test]
    fn does_not_follow_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir(root.join("real")).unwrap();
        write(&root.join("real").join("data.bin"), 100);
        std::os::unix::fs::symlink(root.join("real"), root.join("link")).unwrap();

        let res = scan_dir(root, &AtomicBool::new(false), |_| {});

        // 심링크를 따라갔다면 200이 된다
        assert_eq!(res.stats.bytes, 100);
        assert!(!res.dir_sizes.contains_key(&root.join("link")));
    }
```

- [ ] **Step 2: 테스트 실행으로 현재 동작 확인**

Run (Windows에서는 컴파일만 확인): `cd src-tauri; cargo test scanner; cd ..`
Expected: Windows에서는 cfg(unix) 테스트 제외하고 PASS. CI(ubuntu)에서 이 테스트가 실행된다.

- [ ] **Step 3: reparse point 필터 구현**

`scanner.rs`의 walker 구성을 다음으로 교체:

```rust
    let walker = jwalk::WalkDir::new(root)
        .follow_links(false)
        .skip_hidden(false)
        .process_read_dir(|_depth, _path, _state, children| {
            children.retain(|r| match r {
                Ok(e) => keep_entry(e),
                Err(_) => true, // 에러 엔트리는 유지해서 skipped로 집계
            });
        });
```

그리고 파일 상단(테스트 모듈 위)에 추가:

```rust
/// 심링크(전 플랫폼)와 reparse point(Windows 정션 등)를 순회에서 제외
fn keep_entry(e: &jwalk::DirEntry<((), ())>) -> bool {
    if e.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if let Ok(md) = e.metadata() {
            if md.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return false;
            }
        }
    }
    true
}
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cd src-tauri; cargo test scanner; cd ..`
Expected: 전체 PASS. (Windows 로컬 수동 확인: `New-Item -ItemType Junction`으로 정션을 만들어 스캔 결과에 없는지 Task 9에서 확인)

- [ ] **Step 5: Commit**

```powershell
git add src-tauri
git commit -m "feat(scanner): never traverse symlinks or reparse points

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 5: Tauri 상태·커맨드·이벤트

**Files:**
- Create: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs` (모듈 등록, `manage`, `invoke_handler`)
- Test: `src-tauri/src/commands.rs` 내 `#[cfg(test)] mod tests` (순수 함수 `node_view` 대상)

**Interfaces:**
- Consumes: `scanner::{scan_dir, ScanResult, ScanStats}` (Task 2-4)
- Produces (프론트엔드가 호출하는 계약 — Task 6-8이 그대로 사용):
  - command `list_roots() -> Vec<String>`
  - command `start_scan(root: String) -> Result<(), String>` — 진행 중이면 Err
  - command `cancel_scan()`
  - command `get_node(path: String) -> Result<NodeView, String>`
  - command `top_files(limit: usize) -> Result<Vec<EntryView>, String>`
  - event `"scan://progress"` payload `ScanStats`, event `"scan://done"` payload `ScanStats`
  - `EntryView { name: String, path: String, size: u64, is_dir: bool }`, `NodeView { path: String, size: u64, entries: Vec<EntryView> }` (둘 다 serde::Serialize)

- [ ] **Step 1: 실패하는 테스트 작성**

`src-tauri/src/commands.rs` 생성, 하단에:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::scan_dir;
    use std::fs;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn node_view_lists_entries_sorted_by_size_desc() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir(root.join("sub")).unwrap();
        fs::write(root.join("sub").join("inner.bin"), vec![0u8; 500]).unwrap();
        fs::write(root.join("small.txt"), vec![0u8; 10]).unwrap();

        let res = scan_dir(root, &AtomicBool::new(false), |_| {});
        let view = node_view(&res, root).unwrap();

        assert_eq!(view.size, 510);
        assert_eq!(view.entries.len(), 2);
        assert_eq!(view.entries[0].name, "sub");
        assert!(view.entries[0].is_dir);
        assert_eq!(view.entries[0].size, 500);
        assert_eq!(view.entries[1].name, "small.txt");
        assert!(!view.entries[1].is_dir);
    }

    #[test]
    fn node_view_rejects_path_outside_root() {
        let tmp = tempfile::tempdir().unwrap();
        let res = scan_dir(tmp.path(), &AtomicBool::new(false), |_| {});
        assert!(node_view(&res, std::env::temp_dir().join("..")).is_err());
    }
}
```

- [ ] **Step 2: 테스트 실패 확인**

`src-tauri/src/lib.rs`에 `mod commands;` 추가 후:

Run: `cd src-tauri; cargo test commands; cd ..`
Expected: COMPILE ERROR — `node_view` not found

- [ ] **Step 3: 구현 작성**

`src-tauri/src/commands.rs` 상단에:

```rust
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, Emitter, State};

use crate::scanner::{self, ScanResult};

#[derive(Default)]
pub struct AppState {
    pub result: Arc<Mutex<Option<ScanResult>>>,
    pub cancel: Arc<AtomicBool>,
    pub scanning: Arc<AtomicBool>,
}

#[derive(serde::Serialize)]
pub struct EntryView {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub is_dir: bool,
}

#[derive(serde::Serialize)]
pub struct NodeView {
    pub path: String,
    pub size: u64,
    pub entries: Vec<EntryView>,
}

/// 스캔 결과 + 실시간 read_dir로 한 레벨을 조회 (순수 함수 — 테스트 대상)
pub fn node_view(res: &ScanResult, path: &Path) -> Result<NodeView, String> {
    if !path.starts_with(&res.root) {
        return Err("path outside scanned root".into());
    }
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
        let Ok(entry) = entry else { continue };
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        let p = entry.path();
        let (size, is_dir) = if ft.is_dir() {
            (res.dir_sizes.get(&p).copied().unwrap_or(0), true)
        } else {
            (entry.metadata().map(|m| m.len()).unwrap_or(0), false)
        };
        entries.push(EntryView {
            name: entry.file_name().to_string_lossy().into_owned(),
            path: p.to_string_lossy().into_owned(),
            size,
            is_dir,
        });
    }
    entries.sort_by(|a, b| b.size.cmp(&a.size));
    Ok(NodeView {
        path: path.to_string_lossy().into_owned(),
        size: res.dir_sizes.get(path).copied().unwrap_or(0),
        entries,
    })
}

#[tauri::command]
pub fn list_roots() -> Vec<String> {
    #[cfg(windows)]
    {
        ('A'..='Z')
            .filter_map(|c| {
                let d = format!("{c}:\\");
                Path::new(&d).exists().then_some(d)
            })
            .collect()
    }
    #[cfg(not(windows))]
    {
        let mut roots = vec!["/".to_string()];
        if let Ok(home) = std::env::var("HOME") {
            roots.push(home);
        }
        roots
    }
}

#[tauri::command]
pub fn start_scan(root: String, app: AppHandle, state: State<AppState>) -> Result<(), String> {
    if state.scanning.swap(true, Ordering::SeqCst) {
        return Err("scan already running".into());
    }
    state.cancel.store(false, Ordering::SeqCst);
    let cancel = state.cancel.clone();
    let slot = state.result.clone();
    let scanning = state.scanning.clone();
    std::thread::spawn(move || {
        let res = scanner::scan_dir(Path::new(&root), &cancel, |s| {
            let _ = app.emit("scan://progress", s.clone());
        });
        let stats = res.stats.clone();
        *slot.lock().unwrap() = Some(res); // done 이벤트 전에 저장 (레이스 방지)
        scanning.store(false, Ordering::SeqCst);
        let _ = app.emit("scan://done", stats);
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_scan(state: State<AppState>) {
    state.cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub fn get_node(path: String, state: State<AppState>) -> Result<NodeView, String> {
    let guard = state.result.lock().unwrap();
    let res = guard.as_ref().ok_or("no scan result")?;
    node_view(res, &PathBuf::from(path))
}

#[tauri::command]
pub fn top_files(limit: usize, state: State<AppState>) -> Result<Vec<EntryView>, String> {
    let guard = state.result.lock().unwrap();
    let res = guard.as_ref().ok_or("no scan result")?;
    Ok(res
        .top_files
        .iter()
        .take(limit)
        .map(|(p, size)| EntryView {
            name: p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path: p.to_string_lossy().into_owned(),
            size: *size,
            is_dir: false,
        })
        .collect())
}
```

`src-tauri/src/lib.rs`를 다음 형태로 수정 (템플릿의 `greet` 제거):

```rust
mod commands;
mod scanner;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(commands::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::list_roots,
            commands::start_scan,
            commands::cancel_scan,
            commands::get_node,
            commands::top_files
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `cd src-tauri; cargo test; cd ..`
Expected: scanner 4~5개 + commands 2개 전체 PASS

- [ ] **Step 5: Commit**

```powershell
git add src-tauri
git commit -m "feat(commands): scan lifecycle, per-level node view, top files over IPC

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 6: 프론트엔드 — API 래퍼 + 스캔 컨트롤/진행률

**Files:**
- Create: `src/lib/api.ts`, `src/lib/fmt.ts`
- Modify: `src/App.svelte` (템플릿 내용 전체 교체)

**Interfaces:**
- Consumes: Task 5의 command/event 계약
- Produces:
  - `api.ts`: `listRoots, startScan, cancelScan, getNode, topFiles, onScanProgress, onScanDone` + 타입 `ScanStats, EntryView, NodeView`
  - `fmt.ts`: `fmtBytes(n: number): string`
  - `App.svelte`의 상태/콜백: `open(path: string)`, `jump(i: number)` — Task 7·8 컴포넌트가 사용

- [ ] **Step 1: API 래퍼 작성**

`src/lib/api.ts`:

```typescript
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface ScanStats {
  files: number;
  dirs: number;
  skipped: number;
  bytes: number;
}
export interface EntryView {
  name: string;
  path: string;
  size: number;
  is_dir: boolean;
}
export interface NodeView {
  path: string;
  size: number;
  entries: EntryView[];
}

export const listRoots = () => invoke<string[]>("list_roots");
export const startScan = (root: string) => invoke<void>("start_scan", { root });
export const cancelScan = () => invoke<void>("cancel_scan");
export const getNode = (path: string) => invoke<NodeView>("get_node", { path });
export const topFiles = (limit = 200) => invoke<EntryView[]>("top_files", { limit });

export const onScanProgress = (cb: (s: ScanStats) => void) =>
  listen<ScanStats>("scan://progress", (e) => cb(e.payload));
export const onScanDone = (cb: (s: ScanStats) => void) =>
  listen<ScanStats>("scan://done", (e) => cb(e.payload));
```

`src/lib/fmt.ts`:

```typescript
export function fmtBytes(n: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let v = n;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(v >= 10 || i === 0 ? 0 : 1)} ${units[i]}`;
}
```

- [ ] **Step 2: App.svelte 교체**

`src/App.svelte` 전체를 다음으로 교체 (Treemap/TopFiles는 Task 7·8에서 추가되므로 이 시점엔 주석 처리된 자리 표시 없이 목록만):

```svelte
<script lang="ts">
  import { onMount } from "svelte";
  import * as api from "./lib/api";
  import { fmtBytes } from "./lib/fmt";

  let roots: string[] = $state([]);
  let selectedRoot = $state("");
  let scanning = $state(false);
  let stats: api.ScanStats | null = $state(null);
  let node: api.NodeView | null = $state(null);
  let crumbs: string[] = $state([]);
  let top: api.EntryView[] = $state([]);

  onMount(async () => {
    roots = await api.listRoots();
    selectedRoot = roots[0] ?? "";
    await api.onScanProgress((s) => (stats = s));
    await api.onScanDone(async (s) => {
      stats = s;
      scanning = false;
      crumbs = [selectedRoot];
      node = await api.getNode(selectedRoot);
      top = await api.topFiles(200);
    });
  });

  async function scan() {
    scanning = true;
    node = null;
    top = [];
    await api.startScan(selectedRoot);
  }

  async function open(path: string) {
    crumbs = [...crumbs, path];
    node = await api.getNode(path);
  }

  async function jump(i: number) {
    crumbs = crumbs.slice(0, i + 1);
    node = await api.getNode(crumbs[i]);
  }
</script>

<main>
  <h1>DiskSage</h1>
  <div class="controls">
    <select bind:value={selectedRoot} disabled={scanning}>
      {#each roots as r}<option value={r}>{r}</option>{/each}
    </select>
    {#if scanning}
      <button onclick={() => api.cancelScan()}>취소</button>
    {:else}
      <button onclick={scan}>스캔</button>
    {/if}
    {#if stats}
      <span class="stats">
        파일 {stats.files.toLocaleString()} · {fmtBytes(stats.bytes)}
        {#if stats.skipped > 0}· 스킵 {stats.skipped.toLocaleString()}건{/if}
      </span>
    {/if}
  </div>

  {#if node}
    <nav class="crumbs">
      {#each crumbs as c, i}
        <button class="crumb" onclick={() => jump(i)}>{c}</button>
        {#if i < crumbs.length - 1}<span>›</span>{/if}
      {/each}
    </nav>
    <ul class="entries">
      {#each node.entries as e}
        <li>
          {#if e.is_dir}
            <button class="dir" onclick={() => open(e.path)}>📁 {e.name}</button>
          {:else}
            <span>📄 {e.name}</span>
          {/if}
          <span class="size">{fmtBytes(e.size)}</span>
        </li>
      {/each}
    </ul>
  {/if}
</main>

<style>
  main { font-family: system-ui, sans-serif; padding: 1rem; }
  .controls { display: flex; gap: 0.5rem; align-items: center; }
  .stats { color: #666; font-size: 0.9rem; }
  .crumbs { margin: 0.75rem 0; display: flex; gap: 0.25rem; flex-wrap: wrap; }
  .crumb { background: none; border: none; color: #06c; cursor: pointer; padding: 0; }
  .entries { list-style: none; padding: 0; max-height: 40vh; overflow-y: auto; }
  .entries li { display: flex; justify-content: space-between; padding: 2px 0; }
  .dir { background: none; border: none; cursor: pointer; font: inherit; padding: 0; }
  .size { color: #666; font-variant-numeric: tabular-nums; }
</style>
```

- [ ] **Step 3: 타입 체크 + 수동 확인**

Run: `npm run build`
Expected: 타입 에러 없이 빌드 성공

Run: `npm run tauri dev`
Expected: 루트 드롭다운에 드라이브 목록. 작은 폴더 하나로 스캔 → 진행률 → 완료 후 크기 내림차순 목록, 폴더 클릭 드릴다운, 브레드크럼 동작.

- [ ] **Step 4: Commit**

```powershell
git add src
git commit -m "feat(ui): scan controls, progress, size-sorted entry list with drill-down

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 7: 프론트엔드 — 대용량 파일 테이블

**Files:**
- Create: `src/lib/TopFiles.svelte`
- Modify: `src/App.svelte` (컴포넌트 마운트)

**Interfaces:**
- Consumes: `api.EntryView`, `fmtBytes`, App의 `top` 상태 (Task 6)
- Produces: `TopFiles.svelte` — props `{ files: EntryView[] }`

- [ ] **Step 1: 컴포넌트 작성**

`src/lib/TopFiles.svelte`:

```svelte
<script lang="ts">
  import type { EntryView } from "./api";
  import { fmtBytes } from "./fmt";

  let { files }: { files: EntryView[] } = $props();
</script>

<section>
  <h2>대용량 파일 Top {files.length}</h2>
  <table>
    <thead><tr><th>크기</th><th>경로</th></tr></thead>
    <tbody>
      {#each files as f}
        <tr>
          <td class="size">{fmtBytes(f.size)}</td>
          <td class="path" title={f.path}>{f.path}</td>
        </tr>
      {/each}
    </tbody>
  </table>
</section>

<style>
  section { max-height: 40vh; overflow-y: auto; }
  table { width: 100%; border-collapse: collapse; font-size: 0.85rem; }
  th { text-align: left; position: sticky; top: 0; background: #fff; }
  td { padding: 2px 8px 2px 0; }
  .size { white-space: nowrap; font-variant-numeric: tabular-nums; }
  .path { overflow-wrap: anywhere; color: #444; }
</style>
```

`src/App.svelte`의 script에 import 추가:

```typescript
  import TopFiles from "./lib/TopFiles.svelte";
```

markup의 `{#if node} ... {/if}` 블록 바로 아래에 추가:

```svelte
  {#if top.length > 0}
    <TopFiles files={top} />
  {/if}
```

- [ ] **Step 2: 타입 체크 + 수동 확인**

Run: `npm run build`
Expected: 빌드 성공

Run: `npm run tauri dev` → 스캔 완료 후 테이블에 크기 내림차순 파일 목록 표시

- [ ] **Step 3: Commit**

```powershell
git add src
git commit -m "feat(ui): top large files table

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 8: 트리맵 — squarified 레이아웃 + 캔버스 렌더링

**Files:**
- Create: `src/lib/treemap.ts`, `src/lib/treemap.test.ts`, `src/lib/Treemap.svelte`
- Modify: `src/App.svelte` (컴포넌트 마운트), `package.json` (vitest)

**Interfaces:**
- Consumes: `api.NodeView`, App의 `open(path)` (Task 6)
- Produces:
  - `squarify(items: TreemapItem[], x: number, y: number, w: number, h: number): TreemapRect[]`
  - `TreemapItem { key: string; value: number }`, `TreemapRect extends TreemapItem { x, y, w, h: number }`
  - `Treemap.svelte` — props `{ node: NodeView; onOpen: (path: string) => void }`

- [ ] **Step 1: vitest 추가**

```powershell
npm install -D vitest
```

`package.json`의 `scripts`에 추가: `"test": "vitest run"`

- [ ] **Step 2: 실패하는 테스트 작성**

`src/lib/treemap.test.ts`:

```typescript
import { describe, expect, it } from "vitest";
import { squarify } from "./treemap";

const area = (r: { w: number; h: number }) => r.w * r.h;

describe("squarify", () => {
  it("fills the container, areas proportional to values", () => {
    const rects = squarify(
      [
        { key: "a", value: 6 },
        { key: "b", value: 3 },
        { key: "c", value: 1 },
      ],
      0, 0, 100, 100,
    );
    expect(rects).toHaveLength(3);
    expect(rects.reduce((s, r) => s + area(r), 0)).toBeCloseTo(10000, 3);
    expect(area(rects.find((r) => r.key === "a")!)).toBeCloseTo(6000, 3);
  });

  it("keeps every rect inside the container", () => {
    const items = Array.from({ length: 20 }, (_, i) => ({ key: String(i), value: i + 1 }));
    for (const r of squarify(items, 0, 0, 300, 200)) {
      expect(r.x).toBeGreaterThanOrEqual(-1e-6);
      expect(r.y).toBeGreaterThanOrEqual(-1e-6);
      expect(r.x + r.w).toBeLessThanOrEqual(300 + 1e-6);
      expect(r.y + r.h).toBeLessThanOrEqual(200 + 1e-6);
    }
  });

  it("drops zero/negative values and handles empty input", () => {
    expect(squarify([{ key: "z", value: 0 }, { key: "n", value: -5 }], 0, 0, 10, 10)).toEqual([]);
    expect(squarify([], 0, 0, 10, 10)).toEqual([]);
  });
});
```

- [ ] **Step 3: 테스트 실패 확인**

Run: `npm test`
Expected: FAIL — `./treemap` 모듈 없음

- [ ] **Step 4: 레이아웃 구현**

`src/lib/treemap.ts`:

```typescript
export interface TreemapItem {
  key: string;
  value: number;
}
export interface TreemapRect extends TreemapItem {
  x: number;
  y: number;
  w: number;
  h: number;
}

// Squarified treemap (Bruls, Huizing, van Wijk). value <= 0 항목은 제외.
export function squarify(
  items: TreemapItem[],
  x0: number,
  y0: number,
  w0: number,
  h0: number,
): TreemapRect[] {
  const src = items.filter((i) => i.value > 0).sort((a, b) => b.value - a.value);
  const total = src.reduce((s, i) => s + i.value, 0);
  const out: TreemapRect[] = [];
  if (total === 0 || w0 <= 0 || h0 <= 0) return out;

  const scale = (w0 * h0) / total;
  let x = x0, y = y0, w = w0, h = h0;
  type Scaled = { key: string; value: number; area: number };
  let row: Scaled[] = [];

  const rowSum = (r: Scaled[]) => r.reduce((s, i) => s + i.area, 0);
  const worst = (r: Scaled[], side: number) => {
    const s = rowSum(r);
    const s2 = s * s;
    const side2 = side * side;
    let max = -Infinity, min = Infinity;
    for (const i of r) {
      if (i.area > max) max = i.area;
      if (i.area < min) min = i.area;
    }
    return Math.max((side2 * max) / s2, s2 / (side2 * min));
  };
  const layoutRow = (r: Scaled[]) => {
    const s = rowSum(r);
    if (w >= h) {
      const thick = s / h;
      let yy = y;
      for (const i of r) {
        const hh = i.area / thick;
        out.push({ key: i.key, value: i.value, x, y: yy, w: thick, h: hh });
        yy += hh;
      }
      x += thick;
      w -= thick;
    } else {
      const thick = s / w;
      let xx = x;
      for (const i of r) {
        const ww = i.area / thick;
        out.push({ key: i.key, value: i.value, x: xx, y, w: ww, h: thick });
        xx += ww;
      }
      y += thick;
      h -= thick;
    }
  };

  for (const it of src) {
    const item: Scaled = { key: it.key, value: it.value, area: it.value * scale };
    const side = Math.min(w, h);
    if (row.length === 0 || worst([...row, item], side) <= worst(row, side)) {
      row.push(item);
    } else {
      layoutRow(row);
      row = [item];
    }
  }
  if (row.length > 0) layoutRow(row);
  return out;
}
```

- [ ] **Step 5: 테스트 통과 확인**

Run: `npm test`
Expected: 3 tests PASS

- [ ] **Step 6: 캔버스 컴포넌트 작성**

`src/lib/Treemap.svelte`:

```svelte
<script lang="ts">
  import { squarify, type TreemapRect } from "./treemap";
  import { fmtBytes } from "./fmt";
  import type { NodeView } from "./api";

  let { node, onOpen }: { node: NodeView; onOpen: (path: string) => void } = $props();

  const W = 920;
  const H = 420;
  let canvas: HTMLCanvasElement;
  let rects: TreemapRect[] = [];

  $effect(() => {
    if (canvas) draw(node);
  });

  function draw(n: NodeView) {
    const ctx = canvas.getContext("2d")!;
    ctx.clearRect(0, 0, W, H);
    rects = squarify(
      n.entries.map((e) => ({ key: e.path, value: e.size })),
      0, 0, W, H,
    );
    rects.forEach((r, i) => {
      const e = n.entries.find((x) => x.path === r.key)!;
      ctx.fillStyle = e.is_dir
        ? `hsl(${(i * 47) % 360} 55% 52%)`
        : `hsl(${(i * 47) % 360} 15% 62%)`;
      ctx.fillRect(r.x + 1, r.y + 1, Math.max(r.w - 2, 0), Math.max(r.h - 2, 0));
      if (r.w > 70 && r.h > 20) {
        ctx.fillStyle = "#fff";
        ctx.font = "12px system-ui";
        ctx.fillText(`${e.name} ${fmtBytes(e.size)}`, r.x + 5, r.y + 15, r.w - 10);
      }
    });
  }

  function click(ev: MouseEvent) {
    const b = canvas.getBoundingClientRect();
    const px = ev.clientX - b.left;
    const py = ev.clientY - b.top;
    const hit = rects.find(
      (r) => px >= r.x && px < r.x + r.w && py >= r.y && py < r.y + r.h,
    );
    if (!hit) return;
    const e = node.entries.find((en) => en.path === hit.key);
    if (e?.is_dir) onOpen(e.path);
  }
</script>

<canvas bind:this={canvas} width={W} height={H} onclick={click}></canvas>

<style>
  canvas { max-width: 100%; cursor: pointer; }
</style>
```

`src/App.svelte`의 script에 import 추가:

```typescript
  import Treemap from "./lib/Treemap.svelte";
```

markup에서 `<nav class="crumbs">...</nav>` 바로 아래(entries 목록 위)에 추가:

```svelte
    <Treemap {node} onOpen={open} />
```

- [ ] **Step 7: 타입 체크 + 수동 확인**

Run: `npm run build`
Expected: 빌드 성공

Run: `npm run tauri dev` → 스캔 후 트리맵 표시, 디렉토리 사각형(채도 높은 색) 클릭 시 드릴다운, 브레드크럼으로 상위 이동, 목록과 트리맵이 같은 노드를 보여줌

- [ ] **Step 8: Commit**

```powershell
git add src package.json package-lock.json
git commit -m "feat(ui): squarified treemap with click drill-down

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 9: 최종 검증 + PR

**Files:**
- 없음 (검증·푸시만)

**Interfaces:**
- Consumes: Task 1-8 전체
- Produces: `ContextualWisdomLab/disksage`에 M1 PR

- [ ] **Step 1: 전체 테스트**

Run: `cd src-tauri; cargo test; cd ..`
Expected: 전체 PASS

Run: `npm test`
Expected: 3 tests PASS

Run: `npm run build`
Expected: 빌드 성공

- [ ] **Step 2: 실사용 검증 (Windows)**

Run: `npm run tauri dev`

체크리스트:
1. 실제 드라이브(예: `D:\`) 스캔 → 진행률 갱신 → 완료
2. 스캔 중 취소 버튼 → 즉시 멈춤, `skipped` 표시 확인
3. 정션 미추적 확인: `New-Item -ItemType Junction -Path $env:TEMP\dsj -Target C:\Windows` 만든 뒤 `$env:TEMP` 스캔 → 결과에 `dsj` 하위 용량이 집계되지 않음. 확인 후 `Remove-Item $env:TEMP\dsj`
4. 트리맵 드릴다운/브레드크럼/대용량 테이블 동작

- [ ] **Step 3: 푸시 + PR**

```powershell
git push -u origin feat/m1-scan-treemap
gh pr create --repo ContextualWisdomLab/disksage --base main --head feat/m1-scan-treemap --title "feat: M1 scan + treemap/large-file view" --body "M1 milestone per docs/superpowers/specs/2026-07-10-disksage-design.md: parallel scanner (jwalk) with dir-size aggregation, top-files heap, cancellation, symlink/reparse exclusion; Tauri commands + progress events; Svelte UI with squarified treemap, drill-down, large-file table.

🤖 Generated with [Claude Code](https://claude.com/claude-code)"
```

Expected: PR URL 출력. 조직 필수 워크플로(OpenCode Review 등)가 PR에서 실행됨.
