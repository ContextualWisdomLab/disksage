use std::cell::Cell;
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

pub(crate) fn read_only_traversal_root(root: &Path) -> PathBuf {
    match std::fs::symlink_metadata(root) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
        }
        _ => root.to_path_buf(),
    }
}

pub(crate) fn logical_scan_path(path: &Path, traversal_root: &Path, requested_root: &Path) -> PathBuf {
    path.strip_prefix(traversal_root)
        .map(|relative| requested_root.join(relative))
        .unwrap_or_else(|_| path.to_path_buf())
}

pub(crate) fn provider_home_root() -> Option<PathBuf> {
    ["HOME", "USERPROFILE"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .find(|home| home.is_absolute())
}

fn provider_identity_path(path: &Path, traversal_root: &Path, identity_root: &Path) -> PathBuf {
    path.strip_prefix(traversal_root)
        .map(|relative| identity_root.join(relative))
        .unwrap_or_else(|_| path.to_path_buf())
}

fn is_windows_icloud_drive_root(path: &Path, home_root: &Path) -> bool {
    path == home_root.join("iCloud Drive")
}

pub(crate) fn is_within_managed_provider_scope(path: &Path, home_root: &Path) -> bool {
    #[cfg(target_os = "macos")]
    {
        return [
            home_root.join("Library").join("CloudStorage"),
            home_root.join("Library").join("Mobile Documents"),
            home_root.join("Google Drive"),
        ]
        .iter()
        .any(|provider_root| path.starts_with(provider_root));
    }
    #[cfg(not(target_os = "macos"))]
    {
        let Ok(relative) = path.strip_prefix(home_root) else {
            return false;
        };
        let Some(first) = relative
            .components()
            .next()
            .map(|component| component.as_os_str())
        else {
            return false;
        };
        let Some(name) = first.to_str() else {
            return false;
        };
        name == "OneDrive"
            || name.starts_with("OneDrive - ")
            || name == "Google Drive"
            || (cfg!(windows) && name == "iCloud Drive")
    }
}

fn is_managed_provider_root_with_home(
    path: &Path,
    traversal_root: &Path,
    home_root: Option<&Path>,
) -> bool {
    let Ok(relative) = path.strip_prefix(traversal_root) else {
        return false;
    };
    if relative.as_os_str().is_empty() {
        return false;
    }
    let Some(home_root) = home_root else {
        return false;
    };

    #[cfg(target_os = "macos")]
    let managed_roots = [
        home_root.join("Library").join("CloudStorage"),
        home_root.join("Library").join("Mobile Documents"),
        home_root.join("Google Drive"),
    ];
    #[cfg(target_os = "macos")]
    return managed_roots
        .iter()
        .any(|managed_root| managed_root == path && managed_root.starts_with(traversal_root));

    #[cfg(not(target_os = "macos"))]
    {
        let is_known_root = [home_root.join("OneDrive"), home_root.join("Google Drive")]
            .iter()
            .any(|managed_root| managed_root == path);
        let is_named_account_root = path
            .parent()
            .is_some_and(|parent| parent == home_root)
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == "OneDrive" || name.starts_with("OneDrive - "));
        #[cfg(windows)]
        let is_windows_icloud_root = is_windows_icloud_drive_root(path, home_root);
        #[cfg(not(windows))]
        let is_windows_icloud_root = false;
        return (is_known_root || is_named_account_root || is_windows_icloud_root)
            && path.starts_with(traversal_root);
    }
}

fn keep_scan_entry(
    entry: &walkdir::DirEntry,
    traversal_root: &Path,
    provider_identity_root: &Path,
    provider_roots_skipped: &Cell<u64>,
    provider_home: Option<&Path>,
) -> bool {
    if !keep_entry(entry) {
        return false;
    }
    if entry.file_type().is_dir() {
        let is_provider_root = provider_home.is_some_and(|home| {
            let identity_path =
                provider_identity_path(entry.path(), traversal_root, provider_identity_root);
            is_managed_provider_root_with_home(
                &identity_path,
                provider_identity_root,
                Some(home),
            )
        });
        if is_provider_root {
            provider_roots_skipped.set(provider_roots_skipped.get().saturating_add(1));
            return false;
        }
    }
    true
}

/// ponytail: progress 간격을 파라미터로 뺀 것은 테스트 주입용, 외부 API는 scan_dir
pub fn scan_dir_with_interval(
    root: &Path,
    cancel: &AtomicBool,
    progress_every: u64,
    on_progress: impl FnMut(&ScanStats),
) -> ScanResult {
    let provider_home = provider_home_root();
    scan_dir_with_interval_inner(
        root,
        cancel,
        progress_every,
        on_progress,
        provider_home.as_deref(),
    )
}

#[cfg(test)]
fn scan_dir_with_interval_for_home(
    root: &Path,
    cancel: &AtomicBool,
    progress_every: u64,
    on_progress: impl FnMut(&ScanStats),
    provider_home: &Path,
) -> ScanResult {
    scan_dir_with_interval_inner(
        root,
        cancel,
        progress_every,
        on_progress,
        Some(provider_home),
    )
}

fn scan_dir_with_interval_inner(
    root: &Path,
    cancel: &AtomicBool,
    progress_every: u64,
    mut on_progress: impl FnMut(&ScanStats),
    provider_home: Option<&Path>,
) -> ScanResult {
    let progress_every = progress_every.max(1);
    let mut dir_sizes: HashMap<PathBuf, u64> = HashMap::new();
    // min-heap: 가장 작은 항목이 루트에 오도록 Reverse
    let mut top: BinaryHeap<std::cmp::Reverse<(u64, PathBuf)>> = BinaryHeap::new();
    let mut stats = ScanStats::default();
    let mut cancelled = false;
    let mut seen: u64 = 0;
    let traversal_root = read_only_traversal_root(root);
    // Compare provider identities in one canonical namespace without canonicalizing each walked
    // entry. This avoids Windows `\\?\` path mismatches and macOS `/var` -> `/private/var`
    // mismatches while preserving the requested/root spelling used for user-facing scan results.
    let provider_identity_root = std::fs::canonicalize(&traversal_root)
        .unwrap_or_else(|_| traversal_root.clone());
    let normalized_provider_home = provider_home.map(|home| {
        std::fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf())
    });
    if normalized_provider_home
        .as_deref()
        .is_some_and(|home| is_within_managed_provider_scope(&provider_identity_root, home))
    {
        let stats = ScanStats {
            skipped: 1,
            ..ScanStats::default()
        };
        on_progress(&stats);
        return ScanResult {
            root: root.to_path_buf(),
            dir_sizes,
            top_files: Vec::new(),
            stats,
            cancelled: false,
        };
    }
    let provider_roots_skipped = Cell::new(0_u64);
    let mut reported_provider_roots_skipped = 0_u64;

    let walker = walkdir::WalkDir::new(&traversal_root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            keep_scan_entry(
                entry,
                &traversal_root,
                &provider_identity_root,
                &provider_roots_skipped,
                normalized_provider_home.as_deref(),
            )
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
        let logical_path = logical_scan_path(e.path(), &traversal_root, root);
        if e.file_type().is_dir() {
            stats.dirs += 1;
            dir_sizes.entry(logical_path).or_insert(0);
        } else if e.file_type().is_file() {
            let Ok(md) = e.metadata() else { stats.skipped += 1; continue };
            let size = md.len();
            stats.files += 1;
            stats.bytes += size;
            top.push(std::cmp::Reverse((size, logical_path.clone())));
            if top.len() > TOP_FILES_CAP {
                top.pop();
            }
            // 파일 크기를 root까지의 모든 조상 디렉토리에 누적
            // ponytail: PathBuf 키 HashMap — 초대형 드라이브에서 스캔이 수십 초를
            // 넘기면 인터닝된 디렉토리 인덱스로 교체
            let mut anc = logical_path.parent().map(|p| p.to_path_buf());
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
            let skipped_provider_roots = provider_roots_skipped.get();
            let mut progress = stats.clone();
            progress.skipped = progress.skipped.saturating_add(skipped_provider_roots);
            on_progress(&progress);
            reported_provider_roots_skipped = skipped_provider_roots;
        }
    }
    let final_provider_roots_skipped = provider_roots_skipped.get();
    if final_provider_roots_skipped > reported_provider_roots_skipped {
        let mut progress = stats.clone();
        progress.skipped = progress.skipped.saturating_add(final_provider_roots_skipped);
        on_progress(&progress);
    }
    stats.skipped = stats.skipped.saturating_add(final_provider_roots_skipped);

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
/// (crate 내 다른 순회 지점 — dev_artifacts 등 — 에서도 재사용)
pub(crate) fn keep_entry(e: &walkdir::DirEntry) -> bool {
    if e.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if let Ok(md) = std::fs::symlink_metadata(e.path()) {
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

    fn scan_with_home(root: &Path, provider_home: &Path) -> ScanResult {
        scan_dir_with_interval_for_home(root, &AtomicBool::new(false), 1, noop, provider_home)
    }

    #[test]
    fn provider_identity_namespace_keeps_home_and_entries_comparable() {
        let raw_root = Path::new("/raw-home");
        let identity_root = Path::new("/identity-home");
        let raw_provider = if cfg!(target_os = "macos") {
            raw_root.join("Library").join("CloudStorage")
        } else {
            raw_root.join("OneDrive")
        };
        let identity_provider = provider_identity_path(&raw_provider, raw_root, identity_root);

        assert!(is_managed_provider_root_with_home(
            &identity_provider,
            identity_root,
            Some(identity_root),
        ));
    }

    #[test]
    fn windows_icloud_drive_identity_is_home_level_only() {
        let home_root = Path::new("/synthetic-home");
        assert!(is_windows_icloud_drive_root(
            &home_root.join("iCloud Drive"),
            home_root,
        ));
        assert!(!is_windows_icloud_drive_root(
            &home_root.join("Projects").join("iCloud Drive"),
            home_root,
        ));
    }

    #[cfg(windows)]
    #[test]
    fn windows_verbatim_provider_identity_prunes_onedrive() {
        let raw_root = PathBuf::from(r"C:\Users\DiskSage");
        let identity_root = PathBuf::from(r"\\?\C:\Users\DiskSage");
        let raw_provider = raw_root.join("OneDrive");
        let identity_provider = provider_identity_path(&raw_provider, &raw_root, &identity_root);

        assert_eq!(identity_provider, identity_root.join("OneDrive"));
        assert!(is_managed_provider_root_with_home(
            &identity_provider,
            &identity_root,
            Some(&identity_root),
        ));
    }

    #[cfg(windows)]
    #[test]
    fn windows_icloud_drive_root_is_pruned() {
        let tmp = tempfile::tempdir().unwrap();
        let provider_root = tmp.path().join("iCloud Drive");
        fs::create_dir_all(&provider_root).unwrap();
        write(&provider_root.join("placeholder.bin"), 4096);
        write(&tmp.path().join("local.bin"), 7);

        let result = scan_with_home(tmp.path(), tmp.path());

        assert_eq!(result.stats.files, 1);
        assert_eq!(result.stats.bytes, 7);
        assert_eq!(result.stats.skipped, 1);
        assert!(!result.dir_sizes.contains_key(&provider_root));
    }

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

    #[test]
    fn ancestor_scan_prunes_provider_root_before_file_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        let provider_root = if cfg!(target_os = "macos") {
            tmp.path().join("Library").join("CloudStorage")
        } else {
            tmp.path().join("OneDrive")
        };
        fs::create_dir_all(&provider_root).unwrap();
        write(&provider_root.join("dataless-placeholder.bin"), 4096);
        write(&tmp.path().join("local.bin"), 7);

        let result = scan_with_home(tmp.path(), tmp.path());

        assert_eq!(result.stats.files, 1);
        assert_eq!(result.stats.bytes, 7);
        assert_eq!(result.stats.skipped, 1);
        assert!(!result
            .top_files
            .iter()
            .any(|(path, _)| path.starts_with(&provider_root)));
        assert!(!result.dir_sizes.contains_key(&provider_root));
    }

    #[test]
    fn explicitly_selected_provider_root_is_not_traversed() {
        let tmp = tempfile::tempdir().unwrap();
        let provider_root = if cfg!(target_os = "macos") {
            tmp.path().join("Library").join("CloudStorage")
        } else {
            tmp.path().join("OneDrive")
        };
        fs::create_dir_all(&provider_root).unwrap();
        write(&provider_root.join("selected.bin"), 11);

        let result = scan_with_home(&provider_root, tmp.path());

        assert_eq!(result.stats.files, 0);
        assert_eq!(result.stats.bytes, 0);
        assert_eq!(result.stats.skipped, 1);
        assert!(result.dir_sizes.is_empty());
        assert!(result.top_files.is_empty());
    }

    #[test]
    fn explicitly_selected_provider_descendant_is_not_traversed() {
        let tmp = tempfile::tempdir().unwrap();
        let provider_root = if cfg!(target_os = "macos") {
            tmp.path().join("Library/CloudStorage/ProviderAccount")
        } else {
            tmp.path().join("OneDrive/Folder")
        };
        fs::create_dir_all(&provider_root).unwrap();
        write(&provider_root.join("placeholder.bin"), 11);

        let result = scan_with_home(&provider_root, tmp.path());

        assert_eq!(result.stats.files, 0);
        assert_eq!(result.stats.bytes, 0);
        assert_eq!(result.stats.skipped, 1);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_home_scan_prunes_provider_root() {
        let tmp = tempfile::tempdir().unwrap();
        let real_home = tmp.path().join("real-home");
        fs::create_dir(&real_home).unwrap();
        let provider_root = if cfg!(target_os = "macos") {
            real_home.join("Library").join("CloudStorage")
        } else {
            real_home.join("OneDrive")
        };
        fs::create_dir_all(&provider_root).unwrap();
        write(&provider_root.join("dataless-placeholder.bin"), 4096);
        let selected_home = tmp.path().join("selected-home");
        std::os::unix::fs::symlink(&real_home, &selected_home).unwrap();

        let result = scan_with_home(&selected_home, &selected_home);

        assert_eq!(result.stats.files, 0);
        assert_eq!(result.stats.bytes, 0);
        assert_eq!(result.stats.skipped, 1);
        assert!(result.top_files.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn ancestor_of_symlinked_home_still_prunes_provider_root() {
        let tmp = tempfile::tempdir().unwrap();
        let real_home = tmp.path().join("real-home");
        fs::create_dir(&real_home).unwrap();
        let provider_root = if cfg!(target_os = "macos") {
            real_home.join("Library").join("CloudStorage")
        } else {
            real_home.join("OneDrive")
        };
        fs::create_dir_all(&provider_root).unwrap();
        write(&provider_root.join("dataless-placeholder.bin"), 4096);
        let home_alias = tmp.path().join("home-alias");
        std::os::unix::fs::symlink(&real_home, &home_alias).unwrap();

        let result = scan_with_home(tmp.path(), &home_alias);

        assert_eq!(result.stats.files, 0);
        assert_eq!(result.stats.bytes, 0);
        assert_eq!(result.stats.skipped, 1);
        assert!(result.top_files.is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn library_ancestor_scan_prunes_cloud_storage_root() {
        let tmp = tempfile::tempdir().unwrap();
        let library = tmp.path().join("Library");
        let provider_root = library.join("CloudStorage");
        fs::create_dir_all(&provider_root).unwrap();
        write(&provider_root.join("dataless-placeholder.bin"), 4096);
        write(&library.join("local.bin"), 7);

        let result = scan_with_home(&library, tmp.path());

        assert_eq!(result.stats.files, 1);
        assert_eq!(result.stats.bytes, 7);
        assert_eq!(result.stats.skipped, 1);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn legacy_google_drive_root_is_pruned() {
        let tmp = tempfile::tempdir().unwrap();
        let provider_root = tmp.path().join("Google Drive");
        fs::create_dir_all(&provider_root).unwrap();
        write(&provider_root.join("dataless-placeholder.bin"), 4096);
        write(&tmp.path().join("local.bin"), 7);

        let result = scan_with_home(tmp.path(), tmp.path());

        assert_eq!(result.stats.files, 1);
        assert_eq!(result.stats.bytes, 7);
        assert_eq!(result.stats.skipped, 1);
    }

    #[test]
    fn progress_includes_pruned_provider_roots() {
        let tmp = tempfile::tempdir().unwrap();
        let provider_root = if cfg!(target_os = "macos") {
            tmp.path().join("Library").join("CloudStorage")
        } else {
            tmp.path().join("OneDrive")
        };
        fs::create_dir_all(&provider_root).unwrap();
        write(&provider_root.join("placeholder.bin"), 4096);
        write(&tmp.path().join("local.bin"), 7);
        let observed_skipped = Cell::new(0_u64);

        let result = scan_dir_with_interval_for_home(
            tmp.path(),
            &AtomicBool::new(false),
            1,
            |progress| observed_skipped.set(progress.skipped),
            tmp.path(),
        );

        assert_eq!(result.stats.skipped, 1);
        assert_eq!(observed_skipped.get(), 1);
    }

    #[test]
    fn final_progress_reports_provider_root_skip_without_followup_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let provider_root = if cfg!(target_os = "macos") {
            tmp.path().join("Library").join("CloudStorage")
        } else {
            tmp.path().join("OneDrive")
        };
        fs::create_dir_all(&provider_root).unwrap();
        let observed_skipped = Cell::new(0_u64);

        let result = scan_dir_with_interval_for_home(
            tmp.path(),
            &AtomicBool::new(false),
            1,
            |progress| observed_skipped.set(progress.skipped),
            tmp.path(),
        );

        assert_eq!(result.stats.skipped, 1);
        assert_eq!(observed_skipped.get(), 1);
    }

    #[test]
    fn nested_provider_named_directory_is_scanned() {
        let tmp = tempfile::tempdir().unwrap();
        let nested = tmp.path().join("Projects");
        let nested_provider = nested.join(if cfg!(target_os = "macos") {
            "Google Drive"
        } else {
            "OneDrive"
        });
        fs::create_dir_all(&nested_provider).unwrap();
        write(&nested_provider.join("local.bin"), 13);

        let result = scan_with_home(tmp.path(), tmp.path());

        assert_eq!(result.stats.files, 1);
        assert_eq!(result.stats.bytes, 13);
        assert_eq!(result.stats.skipped, 0);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn named_onedrive_account_root_is_pruned() {
        let tmp = tempfile::tempdir().unwrap();
        let provider_root = tmp.path().join("OneDrive - Example");
        fs::create_dir_all(&provider_root).unwrap();
        write(&provider_root.join("placeholder.bin"), 4096);
        write(&tmp.path().join("local.bin"), 7);

        let result = scan_with_home(tmp.path(), tmp.path());

        assert_eq!(result.stats.files, 1);
        assert_eq!(result.stats.bytes, 7);
        assert_eq!(result.stats.skipped, 1);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_scan_root_still_prunes_provider_root() {
        let tmp = tempfile::tempdir().unwrap();
        let real_home = tmp.path().join("real-home");
        let alias = tmp.path().join("home-alias");
        let provider_root = if cfg!(target_os = "macos") {
            real_home.join("Library").join("CloudStorage")
        } else {
            real_home.join("OneDrive")
        };
        fs::create_dir_all(&provider_root).unwrap();
        write(&provider_root.join("placeholder.bin"), 4096);
        write(&real_home.join("local.bin"), 7);
        std::os::unix::fs::symlink(&real_home, &alias).unwrap();

        let result = scan_with_home(&alias, &real_home);

        assert_eq!(result.stats.files, 1);
        assert_eq!(result.stats.bytes, 7);
        assert_eq!(result.stats.skipped, 1);
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
    fn symlinked_root_scans_target_but_not_nested_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("real");
        let outside = tmp.path().join("outside");
        fs::create_dir(&real).unwrap();
        fs::create_dir(&outside).unwrap();
        write(&real.join("inside.bin"), 11);
        write(&outside.join("outside.bin"), 37);
        std::os::unix::fs::symlink(&outside, real.join("nested-link")).unwrap();
        let selected = tmp.path().join("selected-root");
        std::os::unix::fs::symlink(&real, &selected).unwrap();

        let res = scan_dir(&selected, &AtomicBool::new(false), noop);

        assert_eq!(res.root, selected);
        assert_eq!(res.stats.files, 1);
        assert_eq!(res.stats.bytes, 11);
        assert_eq!(res.dir_sizes[&selected], 11);
        assert_eq!(res.top_files[0].0, selected.join("inside.bin"));
        assert!(!res.dir_sizes.contains_key(&selected.join("nested-link")));
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
