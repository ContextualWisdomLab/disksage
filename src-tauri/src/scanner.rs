use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, BinaryHeap, HashMap};
use std::ffi::OsStr;
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
    /// Bounded exact fallback for regular files admitted before cancellation.
    pub admitted_files: BTreeSet<PathBuf>,
    /// Order-independent evidence for every regular-file child of a completely scanned directory.
    pub(crate) directory_file_manifests: HashMap<PathBuf, DirectoryFileManifest>,
    /// 내림차순 정렬, TOP_FILES_CAP 개로 제한
    pub top_files: Vec<(PathBuf, u64)>,
    pub stats: ScanStats,
    pub cancelled: bool,
}

pub const TOP_FILES_CAP: usize = 1000;
pub const CANCELLED_FILE_ADMISSION_CAP: usize = 1000;
const CLOUD_SCAN_GUIDANCE: &str =
    "클라우드 파일은 일반 스캔 대신 클라우드 보관 화면에서 확인하세요.";
const FILE_PROVIDER_STORAGE_COMPONENT: &str = "File Provider Storage";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct DirectoryFileManifest {
    file_count: u64,
    digest_xor: [u8; 32],
}

fn file_name_bytes(name: &OsStr) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        name.as_bytes().to_vec()
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        name.encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()
    }
    #[cfg(not(any(unix, windows)))]
    {
        name.to_string_lossy().as_bytes().to_vec()
    }
}

fn admit_file(manifest: &mut DirectoryFileManifest, name: &OsStr, size: u64) {
    let name = file_name_bytes(name);
    let mut hasher = Sha256::new();
    hasher.update(b"disksage-directory-file-admission-v1\0");
    hasher.update((name.len() as u64).to_le_bytes());
    hasher.update(name);
    hasher.update(size.to_le_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    for (aggregate, value) in manifest.digest_xor.iter_mut().zip(digest) {
        *aggregate ^= value;
    }
    manifest.file_count += 1;
}

pub(crate) fn current_directory_file_manifest(path: &Path) -> Option<DirectoryFileManifest> {
    let mut manifest = DirectoryFileManifest::default();
    for entry in std::fs::read_dir(path).ok()? {
        let entry = entry.ok()?;
        let file_type = entry.file_type().ok()?;
        if file_type.is_symlink() || !file_type.is_file() {
            continue;
        }
        let metadata = entry.metadata().ok()?;
        admit_file(&mut manifest, &entry.file_name(), metadata.len());
    }
    Some(manifest)
}

fn macos_provider_managed_roots_for_home(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join("Library").join("CloudStorage"),
        home.join("Library").join("Mobile Documents"),
        home.join("Library")
            .join("Application Support")
            .join("FileProvider"),
    ]
}

fn matches_private_file_provider_layout(path: &Path, library: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(library) else {
        return false;
    };
    let components = relative.components().collect::<Vec<_>>();

    if components.len() >= 2
        && components[0].as_os_str() == std::ffi::OsStr::new("Application Support")
        && components[1].as_os_str() == std::ffi::OsStr::new("FileProvider")
    {
        return true;
    }

    if components.len() >= 4
        && components[0].as_os_str() == std::ffi::OsStr::new("Containers")
        && components[2].as_os_str() == std::ffi::OsStr::new("Data")
        && components[3..].iter().any(|component| {
            component.as_os_str() == std::ffi::OsStr::new(FILE_PROVIDER_STORAGE_COMPONENT)
        })
    {
        return true;
    }

    components.len() >= 3
        && components[0].as_os_str() == std::ffi::OsStr::new("Group Containers")
        && components[2..].iter().any(|component| {
            component.as_os_str() == std::ffi::OsStr::new(FILE_PROVIDER_STORAGE_COMPONENT)
        })
}

fn live_macos_user_library(path: &Path) -> Option<PathBuf> {
    let users_root = Path::new("/Users");
    let relative = path.strip_prefix(users_root).ok()?;
    let mut components = relative.components();
    let account = components.next()?;
    if account.as_os_str().is_empty() {
        return None;
    }
    let library = components.next()?;
    if library.as_os_str() != std::ffi::OsStr::new("Library") {
        return None;
    }
    Some(users_root.join(account.as_os_str()).join("Library"))
}

fn is_private_macos_file_provider_storage(path: &Path, roots: &[PathBuf]) -> bool {
    if roots
        .iter()
        .filter(|root| root.file_name() == Some(std::ffi::OsStr::new("CloudStorage")))
        .filter_map(|root| root.parent())
        .any(|library| matches_private_file_provider_layout(path, library))
    {
        return true;
    }

    live_macos_user_library(path)
        .is_some_and(|library| matches_private_file_provider_layout(path, &library))
}

fn is_live_account_public_provider_storage(path: &Path) -> bool {
    let Some(library) = live_macos_user_library(path) else {
        return false;
    };
    [
        library.join("CloudStorage"),
        library.join("Mobile Documents"),
    ]
    .iter()
    .any(|root| path == root || path.starts_with(root))
}

fn is_macos_provider_managed_path(path: &Path, roots: &[PathBuf]) -> bool {
    roots
        .iter()
        .any(|root| path == root || path.starts_with(root))
        || is_live_account_public_provider_storage(path)
        || is_private_macos_file_provider_storage(path, roots)
}

#[cfg(target_os = "macos")]
fn provider_managed_roots() -> Vec<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_absolute())
        .map(|home| macos_provider_managed_roots_for_home(&home))
        .unwrap_or_default()
}

#[cfg(not(target_os = "macos"))]
fn provider_managed_roots() -> Vec<PathBuf> {
    Vec::new()
}

#[cfg(target_os = "macos")]
fn is_provider_managed_path(path: &Path, roots: &[PathBuf]) -> bool {
    is_macos_provider_managed_path(path, roots)
}

#[cfg(not(target_os = "macos"))]
fn is_provider_managed_path(_path: &Path, _roots: &[PathBuf]) -> bool {
    false
}

fn scan_root_access_issue_with_roots(root: &Path, roots: &[PathBuf]) -> Option<&'static str> {
    let traversal_root = read_only_traversal_root(root);
    is_macos_provider_managed_path(&traversal_root, roots).then_some(CLOUD_SCAN_GUIDANCE)
}

pub(crate) fn scan_root_access_issue(root: &Path) -> Option<&'static str> {
    #[cfg(target_os = "macos")]
    {
        return scan_root_access_issue_with_roots(root, &provider_managed_roots());
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = root;
        None
    }
}

pub fn scan_dir(
    root: &Path,
    cancel: &AtomicBool,
    on_progress: impl FnMut(&ScanStats),
) -> ScanResult {
    scan_dir_with_interval(root, cancel, 8192, on_progress)
}

pub(crate) fn read_only_traversal_root(root: &Path) -> PathBuf {
    std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf())
}

pub(crate) fn logical_scan_path(
    path: &Path,
    traversal_root: &Path,
    requested_root: &Path,
) -> PathBuf {
    path.strip_prefix(traversal_root)
        .map(|relative| requested_root.join(relative))
        .unwrap_or_else(|_| path.to_path_buf())
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
    let mut admitted_files = BTreeSet::new();
    let mut directory_file_manifests = HashMap::new();
    // min-heap: 가장 작은 항목이 루트에 오도록 Reverse
    let mut top: BinaryHeap<std::cmp::Reverse<(u64, PathBuf)>> = BinaryHeap::new();
    let mut stats = ScanStats::default();
    let mut cancelled = false;
    let mut seen: u64 = 0;
    let traversal_root = read_only_traversal_root(root);
    let provider_roots = provider_managed_roots();

    if is_provider_managed_path(&traversal_root, &provider_roots) {
        stats.skipped = 1;
        return ScanResult {
            root: root.to_path_buf(),
            dir_sizes,
            admitted_files,
            directory_file_manifests,
            top_files: Vec::new(),
            stats,
            cancelled,
        };
    }

    let walker = walkdir::WalkDir::new(&traversal_root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            keep_entry(entry) && !is_provider_managed_path(entry.path(), &provider_roots)
        });

    for entry in walker {
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        seen += 1;
        // 순회/메타데이터 오류는 skipped로 집계 — 한 줄 let-else (오류 분기가 플랫폼별 테스트에만
        // 잡히더라도 라인 자체는 항상 실행돼 커버리지가 안정적)
        let Ok(e) = entry else {
            stats.skipped += 1;
            continue;
        };
        let logical_path = logical_scan_path(e.path(), &traversal_root, root);
        if e.file_type().is_dir() {
            stats.dirs += 1;
            dir_sizes.entry(logical_path.clone()).or_insert(0);
            directory_file_manifests
                .entry(logical_path)
                .or_insert_with(DirectoryFileManifest::default);
        } else if e.file_type().is_file() {
            let Ok(md) = e.metadata() else {
                stats.skipped += 1;
                continue;
            };
            let size = md.len();
            stats.files += 1;
            stats.bytes += size;
            if admitted_files.len() < CANCELLED_FILE_ADMISSION_CAP {
                admitted_files.insert(logical_path.clone());
            }
            if let (Some(parent), Some(name)) = (logical_path.parent(), logical_path.file_name()) {
                admit_file(
                    directory_file_manifests
                        .entry(parent.to_path_buf())
                        .or_default(),
                    name,
                    size,
                );
            }
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
            on_progress(&stats);
        }
    }

    let mut top_files: Vec<(PathBuf, u64)> = top
        .into_iter()
        .map(|std::cmp::Reverse((size, path))| (path, size))
        .collect();
    top_files.sort_by(|a, b| b.1.cmp(&a.1));

    if cancelled {
        directory_file_manifests.clear();
    }

    ScanResult {
        root: root.to_path_buf(),
        dir_sizes,
        admitted_files,
        directory_file_manifests,
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
    fn provider_managed_path_matching_is_component_aware() {
        let root = PathBuf::from("/home/customer/Library/CloudStorage");
        assert!(is_macos_provider_managed_path(
            &root.join("provider/item.bin"),
            std::slice::from_ref(&root),
        ));
        assert!(!is_macos_provider_managed_path(
            Path::new("/home/customer/Library/CloudStorage-backup"),
            std::slice::from_ref(&root),
        ));
    }

    #[test]
    fn macos_provider_policy_covers_private_fileprovider_forms() {
        let home = PathBuf::from("/Users/customer");
        let roots = macos_provider_managed_roots_for_home(&home);
        assert!(is_macos_provider_managed_path(
            &home.join("Library/Application Support/FileProvider/com.example.provider/data"),
            &roots,
        ));
        assert!(is_macos_provider_managed_path(
            &home.join("Library/Containers/com.example/Data/File Provider Storage/account/item"),
            &roots,
        ));
        assert!(!is_macos_provider_managed_path(
            &home.join("Library/Containers/com.example/Data/File Provider Storage Backup/item"),
            &roots,
        ));
    }

    #[test]
    fn macos_file_provider_storage_marker_is_scoped_to_container_layouts() {
        let home = PathBuf::from("/Users/customer");
        let roots = macos_provider_managed_roots_for_home(&home);
        assert!(is_macos_provider_managed_path(
            &home.join("Library/Containers/com.example/Data/File Provider Storage/account/item"),
            &roots,
        ));
        assert!(is_macos_provider_managed_path(
            &home.join("Library/Group Containers/group.example/File Provider Storage/account/item"),
            &roots,
        ));
        assert!(!is_macos_provider_managed_path(
            &home.join("Documents/File Provider Storage/customer-local/item"),
            &roots,
        ));
        assert!(!is_macos_provider_managed_path(
            Path::new("/Volumes/Data/File Provider Storage/customer-local/item"),
            &roots,
        ));
    }

    #[test]
    fn macos_private_provider_layout_is_not_bound_to_current_home() {
        let current_home = PathBuf::from("/Users/current");
        let roots = macos_provider_managed_roots_for_home(&current_home);
        let other_home = PathBuf::from("/Users/other");

        assert!(is_macos_provider_managed_path(
            &other_home
                .join("Library/Containers/com.vendor/Data/File Provider Storage/account/item"),
            &roots,
        ));
        assert!(is_macos_provider_managed_path(
            &other_home
                .join("Library/Group Containers/group.vendor/File Provider Storage/account/item"),
            &roots,
        ));
        assert!(is_macos_provider_managed_path(
            &other_home.join("Library/Application Support/FileProvider/com.vendor/data"),
            &roots,
        ));
    }

    #[test]
    fn macos_public_provider_roots_are_excluded_for_every_live_account() {
        let roots = macos_provider_managed_roots_for_home(Path::new("/Users/current"));
        for managed in [
            "/Users/other/Library/CloudStorage/OneDrive/item.bin",
            "/Users/other/Library/Mobile Documents/com~apple~CloudDocs/item.bin",
        ] {
            assert!(is_macos_provider_managed_path(Path::new(managed), &roots));
        }
        assert!(!is_macos_provider_managed_path(
            Path::new("/Volumes/Backup/Users/other/Library/CloudStorage/archive.bin"),
            &roots,
        ));
        assert!(!is_macos_provider_managed_path(
            Path::new("/Users/current/project/Users/other/Library/CloudStorage/archive.bin"),
            &roots,
        ));
    }

    #[test]
    fn macos_cross_account_fallback_rejects_nested_and_mounted_home_copies() {
        let current_home = PathBuf::from("/Users/current");
        let roots = macos_provider_managed_roots_for_home(&current_home);
        let private_suffix =
            "Library/Containers/com.vendor/Data/File Provider Storage/account/item";

        assert!(!is_macos_provider_managed_path(
            &current_home.join("project/Users/demo").join(private_suffix),
            &roots,
        ));
        assert!(!is_macos_provider_managed_path(
            &PathBuf::from("/Volumes/Backup/Users/demo").join(private_suffix),
            &roots,
        ));
        assert!(!is_macos_provider_managed_path(
            &PathBuf::from("/Volumes/Data/Users/demo").join(private_suffix),
            &roots,
        ));
    }

    #[cfg(unix)]
    #[test]
    fn provider_root_alias_is_resolved_before_access_guidance() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let managed = home.join("Library/CloudStorage/account");
        fs::create_dir_all(&managed).unwrap();
        let selected = tmp.path().join("selected-cloud-root");
        std::os::unix::fs::symlink(&managed, &selected).unwrap();
        let roots = macos_provider_managed_roots_for_home(&home);

        assert!(scan_root_access_issue_with_roots(&selected, &roots).is_some());
    }

    #[cfg(unix)]
    #[test]
    fn provider_root_below_ancestor_alias_is_resolved_before_access_guidance() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let managed = home.join("Library/CloudStorage/account");
        let managed_child = managed.join("nested");
        fs::create_dir_all(&managed_child).unwrap();
        let alias = tmp.path().join("cloud-alias");
        std::os::unix::fs::symlink(&managed, &alias).unwrap();
        let selected = alias.join("nested");
        let roots = macos_provider_managed_roots_for_home(&home);

        assert!(scan_root_access_issue_with_roots(&selected, &roots).is_some());
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
    fn completed_large_flat_scan_retains_bounded_navigation_evidence() {
        let tmp = tempfile::tempdir().unwrap();
        for index in 0..10_000 {
            write(&tmp.path().join(format!("file-{index:05}.bin")), 1);
        }

        let result = scan_dir(tmp.path(), &AtomicBool::new(false), noop);

        assert_eq!(result.stats.files, 10_000);
        assert_eq!(result.admitted_files.len(), CANCELLED_FILE_ADMISSION_CAP);
        assert_eq!(result.directory_file_manifests.len(), 1);
        assert_eq!(
            result.directory_file_manifests.get(tmp.path()),
            current_directory_file_manifest(tmp.path()).as_ref()
        );
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
        if running_as_root() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let locked = root.join("locked");
        fs::create_dir(&locked).unwrap();
        write(&locked.join("hidden.bin"), 10);
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();

        let res = scan_dir(root, &AtomicBool::new(false), noop);

        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(
            res.stats.skipped >= 1,
            "expected skipped >= 1, got {}",
            res.stats.skipped
        );
        assert_eq!(res.stats.files, 0);
    }

    #[cfg(unix)]
    #[test]
    fn metadata_failure_counts_as_skipped() {
        use std::os::unix::fs::PermissionsExt;
        if running_as_root() {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let noexec = root.join("noexec");
        fs::create_dir(&noexec).unwrap();
        write(&noexec.join("unstattable.bin"), 10);
        // r-- 디렉토리: 목록은 읽히지만(파일이 보임) 자식 stat은 EACCES
        fs::set_permissions(&noexec, fs::Permissions::from_mode(0o444)).unwrap();

        let res = scan_dir(root, &AtomicBool::new(false), noop);

        fs::set_permissions(&noexec, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(
            res.stats.skipped >= 1,
            "expected skipped >= 1, got {}",
            res.stats.skipped
        );
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
