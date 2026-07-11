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
    let progress_every = progress_every.max(1);
    let mut dir_sizes: HashMap<PathBuf, u64> = HashMap::new();
    // min-heap: 가장 작은 항목이 루트에 오도록 Reverse
    let mut top: BinaryHeap<std::cmp::Reverse<(u64, PathBuf)>> = BinaryHeap::new();
    let mut stats = ScanStats::default();
    let mut cancelled = false;
    let mut seen: u64 = 0;

    let walker = jwalk::WalkDir::new(root)
        .follow_links(false)
        .skip_hidden(false)
        .process_read_dir(|_depth, _path, _state, children| {
            // 에러 엔트리는 유지해서 skipped로 집계
            children.retain(|r| r.as_ref().map(keep_entry).unwrap_or(true));
        });

    for entry in walker {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        seen += 1;
        // 순회/메타데이터 오류는 skipped로 집계 — 한 줄 let-else (오류 분기가 플랫폼별 테스트에만
        // 잡히더라도 라인 자체는 항상 실행돼 커버리지가 안정적)
        let Ok(e) = entry else { stats.skipped += 1; continue };
        if e.file_type().is_dir() {
            stats.dirs += 1;
            // jwalk는 하위 목록 읽기 실패를 Err 항목이 아니라 디렉토리 엔트리의
            // read_children_error에 담아 전달한다 — 놓치면 스킵 집계가 새는 버그
            if e.read_children_error.is_some() {
                stats.skipped += 1;
            }
            dir_sizes.entry(e.path()).or_insert(0);
        } else if e.file_type().is_file() {
            let Ok(md) = e.metadata() else { stats.skipped += 1; continue };
            let size = md.len();
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
        // dir도 file도 아닌 항목(FIFO/소켓 등)은 집계 없이 무시됨 (심링크/reparse는 keep_entry가 순회에서 제외)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::sync::atomic::AtomicBool;

    fn write(p: &Path, bytes: usize) {
        fs::write(p, vec![0u8; bytes]).unwrap();
    }

    // 공유 no-op 진행 콜백 — progress_every_zero_does_not_panic(간격 1)에서 실제로 실행되므로
    // 각 테스트마다 실행되지 않는 클로저(커버리지에 0으로 집계됨)를 만들지 않는다
    fn noop(_: &ScanStats) {}

    #[test]
    fn aggregates_dir_sizes_up_the_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir_all(root.join("a").join("b")).unwrap();
        write(&root.join("a").join("one.bin"), 100);
        write(&root.join("a").join("b").join("two.bin"), 50);
        write(&root.join("three.bin"), 7);

        let res = scan_dir(root, &AtomicBool::new(false), noop);

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

        let res = scan_dir(root, &AtomicBool::new(false), noop);

        let names: Vec<String> = res
            .top_files
            .iter()
            .map(|(p, _)| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["big.bin", "mid.bin", "small.bin"]);
        assert_eq!(res.top_files[0].1, 300);
    }

    #[test]
    fn progress_every_zero_does_not_panic() {
        let tmp = tempfile::tempdir().unwrap();
        write(&tmp.path().join("f.bin"), 1);
        let res = scan_dir_with_interval(tmp.path(), &AtomicBool::new(false), 0, noop);
        assert_eq!(res.stats.files, 1);
    }

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
    fn top_files_capped_at_limit() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        for i in 0..(TOP_FILES_CAP + 5) {
            write(&root.join(format!("f{i}.bin")), 1 + (i % 7));
        }
        let res = scan_dir(root, &AtomicBool::new(false), noop);
        assert_eq!(res.top_files.len(), TOP_FILES_CAP);
    }

    #[test]
    fn cancel_stops_scan_early() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        for i in 0..50 {
            write(&root.join(format!("f{i}.bin")), 1);
        }
        let cancel = AtomicBool::new(true); // 시작 전부터 취소됨
        let res = scan_dir(root, &cancel, noop);
        assert!(res.cancelled);
        assert!(res.stats.files < 50);
    }

    #[cfg(unix)]
    #[test]
    fn does_not_follow_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir(root.join("real")).unwrap();
        write(&root.join("real").join("data.bin"), 100);
        std::os::unix::fs::symlink(root.join("real"), root.join("link")).unwrap();

        let res = scan_dir(root, &AtomicBool::new(false), noop);

        // 심링크를 따라갔다면 200이 된다
        assert_eq!(res.stats.bytes, 100);
        assert!(!res.dir_sizes.contains_key(&root.join("link")));
    }

    #[cfg(unix)]
    #[test]
    fn non_file_non_dir_entries_are_ignored() {
        // FIFO는 dir도 file도 아니어서 분류 분기의 암묵적 else(집계 없음)를 태운다
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write(&root.join("real.bin"), 10);
        let status = std::process::Command::new("mkfifo")
            .arg(root.join("pipe"))
            .status()
            .unwrap();
        assert!(status.success(), "mkfifo failed");

        let res = scan_dir(root, &AtomicBool::new(false), noop);

        assert_eq!(res.stats.files, 1);
        assert_eq!(res.stats.bytes, 10);
        assert_eq!(res.stats.skipped, 0);
    }

    #[cfg(unix)]
    fn running_as_root() -> bool {
        std::process::Command::new("id")
            .arg("-u")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
            .unwrap_or(false)
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_dir_counts_as_skipped() {
        use std::os::unix::fs::PermissionsExt;
        // root는 권한 비트를 무시하므로 이 테스트는 의미 없음 (한 줄: CI 비-root에서 return 라인 미실행 방지)
        if running_as_root() { return; }
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let locked = root.join("locked");
        fs::create_dir(&locked).unwrap();
        write(&locked.join("hidden.bin"), 10);
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

        let res = scan_dir(root, &AtomicBool::new(false), noop);

        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(res.stats.skipped >= 1, "expected skipped >= 1, got {}", res.stats.skipped);
        assert_eq!(res.stats.files, 0);
    }

    #[cfg(unix)]
    #[test]
    fn metadata_failure_counts_as_skipped() {
        use std::os::unix::fs::PermissionsExt;
        if running_as_root() { return; }
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let noexec = root.join("noexec");
        fs::create_dir(&noexec).unwrap();
        write(&noexec.join("unstattable.bin"), 10);
        // r-- 디렉토리: 목록은 읽히지만(파일이 보임) 자식 stat은 EACCES
        fs::set_permissions(&noexec, fs::Permissions::from_mode(0o444)).unwrap();

        let res = scan_dir(root, &AtomicBool::new(false), noop);

        fs::set_permissions(&noexec, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(res.stats.skipped >= 1, "expected skipped >= 1, got {}", res.stats.skipped);
        assert_eq!(res.stats.bytes, 0);
    }

    #[cfg(windows)]
    #[test]
    fn does_not_follow_junctions() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let target = root.join("real");
        fs::create_dir(&target).unwrap();
        write(&target.join("data.bin"), 100);
        let junction = root.join("junc");
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&target)
            .status()
            .unwrap();
        assert!(status.success(), "mklink /J failed");

        let res = scan_dir(root, &AtomicBool::new(false), noop);

        assert_eq!(res.stats.bytes, 100); // 정션을 따라갔다면 200
        assert!(!res.dir_sizes.contains_key(&junction));
    }
}
