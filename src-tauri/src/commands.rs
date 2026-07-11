use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
#[cfg(not(coverage))]
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

#[cfg(not(coverage))]
use tauri::{AppHandle, Emitter, State};

#[cfg(not(coverage))]
use crate::scanner;
use crate::scanner::ScanResult;

// clean_paths_inner(순수 함수)가 쓰는 것은 무조건 import; 래퍼 전용은 cfg(not(coverage))
use crate::safety;
#[cfg(not(coverage))]
use crate::{dev_artifacts, rules};

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
    // '..'는 lexical starts_with를 우회해 루트 밖을 열람할 수 있음 — 컴포넌트 단위로 거부
    if path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err("path outside scanned root".into());
    }
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
            // 저널의 bytes는 감사 추적용 — 디렉토리는 재귀 합산 (metadata.len()은 dir 엔트리 자체 크기라 무의미).
            // 보호된 경로는 trash_delete가 저널링 전에 거부해 bytes를 쓰지 않으므로, 그런 경로(예: C:\Windows
            // 전체)를 재귀 스캔하는 낭비를 미리 걸러낸다 — 최종 판정은 여전히 trash_delete가 내린다.
            let bytes = if safety::is_protected(p) {
                0
            } else if p.is_dir() {
                crate::scanner::scan_dir(p, &std::sync::atomic::AtomicBool::new(false), |_| {})
                    .stats
                    .bytes
            } else {
                p.metadata().map(|m| m.len()).unwrap_or(0)
            };
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
        roots.extend(std::env::var("HOME").ok());
        roots
    }
}

// 아래 Tauri command 래퍼들은 coverage 빌드에서 제외 — 순수 로직(node_view 등)은 위에서 측정됨
#[cfg(not(coverage))]
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
        // 패닉으로 스레드가 죽어도 scanning 플래그는 반드시 해제
        struct ScanningReset(Arc<AtomicBool>);
        impl Drop for ScanningReset {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let _reset = ScanningReset(scanning);
        let res = scanner::scan_dir(Path::new(&root), &cancel, |s| {
            let _ = app.emit("scan://progress", s.clone());
        });
        let stats = res.stats.clone();
        *slot.lock().unwrap() = Some(res); // done 이벤트 전에 저장 (레이스 방지)
        drop(_reset); // emit 전에 scanning 플래그 해제 (원래 순서 복원, 패닉 안전성은 Drop이 유지)
        let _ = app.emit("scan://done", stats);
    });
    Ok(())
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn cancel_scan(state: State<AppState>) {
    state.cancel.store(true, Ordering::SeqCst);
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn get_node(path: String, state: State<AppState>) -> Result<NodeView, String> {
    // ponytail: lock held across read_dir I/O; snapshot dir_sizes and read outside the lock if this stalls on huge/network dirs
    let guard = state.result.lock().unwrap();
    let res = guard.as_ref().ok_or("no scan result")?;
    node_view(res, &PathBuf::from(path))
}

#[cfg(not(coverage))]
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

#[cfg(not(coverage))]
#[tauri::command]
pub fn expand_clean_targets(dir: String) -> Vec<String> {
    // 카탈로그 경로로만 스코프 — 임의 디렉토리 열람 IPC가 되지 않도록
    let Some(bases) = rules::BaseDirs::from_env() else { return Vec::new() };
    let d = Path::new(&dir);
    if !rules::is_catalog_path(&bases, d) {
        return Vec::new();
    }
    rules::clean_targets(d)
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::scan_dir_with_interval;
    use std::fs;
    use std::sync::atomic::AtomicBool;

    // 간격 1로 스캔 — 진행 콜백(클로저)도 매 엔트리마다 실행돼 커버리지에 0으로 남지 않는다
    fn scan(root: &Path) -> ScanResult {
        scan_dir_with_interval(root, &AtomicBool::new(false), 1, |_| {})
    }

    #[test]
    fn node_view_lists_entries_sorted_by_size_desc() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir(root.join("sub")).unwrap();
        fs::write(root.join("sub").join("inner.bin"), vec![0u8; 500]).unwrap();
        fs::write(root.join("small.txt"), vec![0u8; 10]).unwrap();

        let res = scan(root);
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
        let res = scan(tmp.path());
        assert!(node_view(&res, &std::env::temp_dir().join("..")).is_err());
    }

    #[test]
    fn node_view_rejects_parent_dir_components() {
        let tmp = tempfile::tempdir().unwrap();
        let res = scan(tmp.path());
        // lexical starts_with는 통과하지만 OS 해석은 루트 밖(실존 디렉토리)인 경로 — 가드 없으면 Ok
        let sneaky = tmp.path().join("..");
        assert!(node_view(&res, &sneaky).is_err());
    }

    #[test]
    fn node_view_rejects_sibling_path_outside_root() {
        // '..' 없이 루트 밖인 경로 — 두 번째 가드(starts_with)를 직접 태운다
        let tmp = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let res = scan(tmp.path());
        assert!(node_view(&res, other.path()).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn node_view_skips_junctions() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir(root.join("real")).unwrap();
        let junction = root.join("junc");
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(root.join("real"))
            .status()
            .unwrap();
        assert!(status.success(), "mklink /J failed");
        let res = scan(root);
        let view = node_view(&res, root).unwrap();
        assert!(view.entries.iter().all(|e| e.name != "junc"));
    }

    #[test]
    fn node_view_errors_on_unreadable_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let res = scan(tmp.path());
        assert!(node_view(&res, &tmp.path().join("missing")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn node_view_skips_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::write(root.join("real.bin"), vec![0u8; 5]).unwrap();
        std::os::unix::fs::symlink(root.join("real.bin"), root.join("link.bin")).unwrap();
        let res = scan(root);
        let view = node_view(&res, root).unwrap();
        assert!(view.entries.iter().all(|e| e.name != "link.bin"));
    }

    #[test]
    fn list_roots_returns_platform_roots() {
        let roots = list_roots();
        assert!(!roots.is_empty());
        #[cfg(windows)]
        assert!(roots.iter().any(|r| r.ends_with(":\\")));
        #[cfg(not(windows))]
        assert!(roots.contains(&"/".to_string()));
    }

    #[test]
    fn clean_paths_inner_reports_per_item_results() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let ok_dir = tmp.path().join("disksage-clean-fixture-dir");
        fs::create_dir(&ok_dir).unwrap();
        fs::write(ok_dir.join("inner.bin"), vec![0u8; 32]).unwrap();
        let missing = tmp.path().join("ghost");
        let protected = std::path::PathBuf::from(if cfg!(windows) { "C:\\Windows" } else { "/usr" });

        let results = clean_paths_inner(&[ok_dir.clone(), missing, protected], &jp, 7);

        assert_eq!(results.len(), 3);
        assert!(results[0].ok);
        assert!(!results[1].ok && results[1].error.contains("휴지통"));
        assert!(!results[2].ok && results[2].error.contains("보호"));
        assert!(!ok_dir.exists());

        let recent = crate::safety::journal_recent(&jp, 10);
        let ok_entry = recent
            .iter()
            .find(|e| e.outcome == "ok" && e.path.contains("disksage-clean-fixture-dir"))
            .unwrap();
        assert_eq!(ok_entry.bytes, 32, "디렉토리는 재귀 크기로 저널링");

        // 테스트 픽스처 휴지통 정리 (win/linux)
        #[cfg(any(windows, target_os = "linux"))]
        {
            let items: Vec<_> = trash::os_limited::list()
                .unwrap()
                .into_iter()
                .filter(|i| i.name.to_string_lossy().contains("disksage-clean-fixture-dir"))
                .collect();
            trash::os_limited::purge_all(items).unwrap();
        }
    }
}
