use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// An explicit manual keep boundary inherited by every descendant.
pub const PROTECTED_PATH_MARKER: &str = ".disksage-protected";
/// Binds a tree to a class IRI in the bundled safety ontology; retained classes veto cleanup.
pub const ONTOLOGY_CLASS_MARKER: &str = ".disksage-ontology-class";

fn sidecar(path: &Path, suffix: &str) -> Option<PathBuf> {
    let mut name = path.file_name()?.to_os_string();
    name.push(suffix);
    Some(path.with_file_name(name))
}

/// Adds an ontology-backed deletion veto to one existing file or directory.
pub fn bind_retained_ontology_class(path: &Path, class_id: &str) -> Result<PathBuf, String> {
    if !path.is_absolute() || class_id.is_empty() || class_id.len() > 2_048 {
        return Err("ontology-protection-binding-invalid".into());
    }
    if !crate::ontology::bundled_class_requires_retention(class_id)
        .map_err(|_| "ontology-protection-class-unknown".to_string())?
    {
        return Err("ontology-protection-class-not-retained".into());
    }
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "ontology-protection-target-unavailable".to_string())?;
    if metadata.file_type().is_symlink() || (!metadata.is_file() && !metadata.is_dir()) {
        return Err("ontology-protection-target-unsafe".into());
    }
    let marker = if metadata.is_dir() {
        path.join(ONTOLOGY_CLASS_MARKER)
    } else {
        sidecar(path, ONTOLOGY_CLASS_MARKER)
            .ok_or_else(|| "ontology-protection-target-unsafe".to_string())?
    };
    if marker.exists() {
        let existing = std::fs::read_to_string(&marker)
            .map_err(|_| "ontology-protection-binding-unreadable".to_string())?;
        return (existing.trim() == class_id)
            .then_some(marker)
            .ok_or_else(|| "ontology-protection-binding-conflict".to_string());
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    use std::io::Write as _;
    let mut file = options
        .open(&marker)
        .map_err(|_| "ontology-protection-binding-create-failed".to_string())?;
    file.write_all(class_id.as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .map_err(|_| "ontology-protection-binding-write-failed".to_string())?;
    Ok(marker)
}

pub fn is_explicitly_protected(path: &Path) -> bool {
    if sidecar(path, PROTECTED_PATH_MARKER).is_some_and(|marker| marker.is_file()) {
        return true;
    }
    if let Some(binding) = sidecar(path, ONTOLOGY_CLASS_MARKER).filter(|marker| marker.exists()) {
        return std::fs::read_to_string(binding)
            .map_err(|_| ())
            .and_then(|class_id| {
                crate::ontology::bundled_class_requires_retention(class_id.trim()).map_err(|_| ())
            })
            .unwrap_or(true);
    }
    path.ancestors().any(|ancestor| {
        if ancestor.join(PROTECTED_PATH_MARKER).is_file() {
            return true;
        }
        let binding = ancestor.join(ONTOLOGY_CLASS_MARKER);
        if !binding.exists() {
            return false;
        }
        std::fs::read_to_string(binding)
            .map_err(|_| ())
            .and_then(|class_id| {
                crate::ontology::bundled_class_requires_retention(class_id.trim()).map_err(|_| ())
            })
            .unwrap_or(true)
    })
}

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

/// HOME/USERPROFILE이 설정돼 있을 때만 정확히 그 경로와 일치하는지 (없으면 이 계층은 생략).
/// 실제 프로세스 환경변수를 건드리지 않고 부재 케이스를 테스트하기 위해 분리된 순수 함수.
fn is_home_root(path: &Path, home: Option<&str>) -> bool {
    match home {
        Some(h) => path == Path::new(h),
        None => false,
    }
}

#[cfg(target_os = "macos")]
fn is_macos_user_temp_descendant(path: &Path) -> bool {
    let Ok(temp_root) = std::fs::canonicalize(std::env::temp_dir()) else {
        return false;
    };
    let platform_temp_parent = Path::new("/private/var/folders");
    temp_root != platform_temp_parent
        && temp_root.starts_with(platform_temp_parent)
        && path != temp_root
        && path.starts_with(temp_root)
}

#[cfg(target_os = "macos")]
fn shared_temp_root_path() -> &'static Path {
    Path::new("/private/tmp")
}

#[cfg(all(unix, not(target_os = "macos")))]
fn shared_temp_root_path() -> &'static Path {
    Path::new("/tmp")
}

#[cfg(unix)]
pub(crate) fn is_shared_temp_path(path: &Path) -> bool {
    let Ok(root) = std::fs::canonicalize(shared_temp_root_path()) else {
        return false;
    };
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if metadata.file_type().is_symlink() {
        return false;
    }
    let Ok(canonical) = std::fs::canonicalize(path) else {
        return false;
    };
    canonical != root && canonical.starts_with(root)
}

#[cfg(not(unix))]
pub(crate) fn is_shared_temp_path(_path: &Path) -> bool {
    false
}

/// Returns true only when every object below a shared temporary child belongs to this user.
/// Symlink roots and unreadable trees fail closed so a shared system directory cannot become a
/// broad deletion authority. Owned symlink children are safe because traversal uses
/// `symlink_metadata` and never descends through them.
#[cfg(unix)]
pub(crate) fn is_user_owned_shared_temp_tree(path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;

    if !is_shared_temp_path(path) {
        return false;
    }
    const MAX_OWNERSHIP_ENTRIES: usize = 1_000_000;
    let expected_uid = unsafe { libc::geteuid() };
    let mut pending = vec![path.to_path_buf()];
    let mut inspected = 0usize;
    while let Some(current) = pending.pop() {
        inspected = inspected.saturating_add(1);
        if inspected > MAX_OWNERSHIP_ENTRIES {
            return false;
        }
        let Ok(metadata) = std::fs::symlink_metadata(&current) else {
            return false;
        };
        if metadata.uid() != expected_uid {
            return false;
        }
        if metadata.is_dir() {
            let Ok(entries) = std::fs::read_dir(&current) else {
                return false;
            };
            for entry in entries {
                let Ok(entry) = entry else {
                    return false;
                };
                pending.push(entry.path());
            }
        }
    }
    true
}

#[cfg(not(unix))]
pub(crate) fn is_user_owned_shared_temp_tree(_path: &Path) -> bool {
    false
}

/// 시스템·루트 경로 하드 거부 목록 (스펙 §7-3).
/// 안전 계층의 최후 방어선 — 호출자가 무엇을 넘기든 여기서 걸러진다.
pub fn is_protected(path: &Path) -> bool {
    // 드라이브/파일시스템 루트 자체
    if path.parent().is_none() {
        return true;
    }
    // 사용자 홈 루트 자체 (하위는 허용). 데스크톱 앱은 항상 사용자 세션에서 실행되므로
    // USERPROFILE/HOME 부재는 상정하지 않는다 — 없으면 이 계층만 생략되고
    // 루트/시스템 프리픽스 검사는 그대로 적용된다.
    let home = std::env::var(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).ok();
    if is_home_root(path, home.as_deref()) {
        return true;
    }
    if is_explicitly_protected(path) {
        return true;
    }
    #[cfg(windows)]
    {
        // 컴포넌트 단위 비교: '/'와 '\\' 모두 구분자로 파싱되고(C:/Windows 우회 차단),
        // 경계가 정확해 C:\WindowsBackup 같은 형제 폴더를 오차단하지 않는다
        fn lower_components(p: &Path) -> Vec<String> {
            p.components()
                .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
                .collect()
        }
        // 시스템 드라이브가 C:가 아닌 머신도 보호 — env에서 유도, 실패 시 C: 폴백
        let denied_roots: Vec<String> = {
            let mut roots = Vec::new();
            if let Ok(w) = std::env::var("SystemRoot") {
                roots.push(w); // 예: C:\Windows, D:\Windows
            } else {
                roots.push(r"C:\Windows".to_string());
            }
            match std::env::var("ProgramFiles") {
                Ok(p) => roots.push(p),
                Err(_) => roots.push(r"C:\Program Files".to_string()),
            }
            match std::env::var("ProgramFiles(x86)") {
                Ok(p) => roots.push(p),
                Err(_) => roots.push(r"C:\Program Files (x86)".to_string()),
            }
            roots
        };
        let pc = lower_components(path);
        for d in denied_roots {
            let dc = lower_components(Path::new(&d));
            if pc.len() >= dc.len() && pc[..dc.len()] == dc[..] {
                return true;
            }
        }
    }
    #[cfg(unix)]
    {
        if std::fs::canonicalize(shared_temp_root_path())
            .ok()
            .zip(std::fs::canonicalize(path).ok())
            .is_some_and(|(root, canonical)| root == canonical)
        {
            return true;
        }
        // macOS의 사용자별 임시 디렉터리는 /private 아래로 canonicalize된다. 그 하위만
        // 허용하되 임시 루트 자체와 그 밖의 /private 트리는 계속 보호한다. 보호 경로를
        // 가리키는 심링크는 호출부에서 먼저 canonicalize되므로 이 예외를 우회할 수 없다.
        #[cfg(target_os = "macos")]
        if is_macos_user_temp_descendant(path) {
            return false;
        }
        // Shared system temporary trees stay globally protected. Current-user ownership is a
        // purpose-bound deletion authority checked only by the two Trash entry points below;
        // it must not widen cloud eviction, clone reclaim, or other callers of this guard.
        if is_shared_temp_path(path) {
            return true;
        }
        // macOS는 extend로 시스템 경로를 더 넣는다 — 다른 unix에선 그 라인이 cfg-out되어 mut가
        // 미사용이므로 allow(unused_mut). Linux 게이트는 macOS 전용 라인을 컴파일하지 않아 커버 불필요.
        #[allow(unused_mut)]
        let mut denied_prefixes: Vec<&str> = vec![
            "/usr", "/etc", "/bin", "/sbin", "/lib", "/boot", "/proc", "/sys", "/dev",
        ];
        #[cfg(target_os = "macos")]
        denied_prefixes.extend_from_slice(&[
            "/System",
            "/Library",
            "/Applications",
            "/private",
            "/Volumes",
            "/cores",
            "/Network",
        ]);
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

/// Stable identity for one filesystem object. Metadata fingerprints describe a tree, while this
/// identity binds the later trash operation to the exact directory entry observed at review time.
/// Unix uses the device/inode pair. Windows obtains the equivalent volume/file-index identity from
/// an open handle in [`filesystem_object_id`], because the stable `std` metadata accessors are not
/// available on the supported Rust toolchains. Unsupported platforms fail closed because a
/// path-only fallback would reintroduce a replacement race.
pub fn object_id_from_metadata(metadata: &std::fs::Metadata) -> Option<String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        return Some(format!("unix:{}:{}", metadata.dev(), metadata.ino()));
    }
    #[cfg(windows)]
    {
        let _ = metadata;
        return None;
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = metadata;
        None
    }
}

pub fn filesystem_object_id(path: &Path) -> std::io::Result<String> {
    #[cfg(windows)]
    {
        let handle = winapi_util::Handle::from_path_any(path)?;
        let info = winapi_util::file::information(&handle)?;
        return Ok(format!(
            "windows:{}:{}",
            info.volume_serial_number(),
            info.file_index()
        ));
    }

    #[cfg(not(windows))]
    {
        let metadata = std::fs::symlink_metadata(path)?;
        object_id_from_metadata(&metadata).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "filesystem object identity is unavailable on this platform",
            )
        })
    }
}

#[cfg(target_os = "linux")]
fn ensure_identity_bound_final_mutation_supported() -> Result<(), SafetyError> {
    Err(SafetyError::Trash(
        "identity-bound final mutation is unavailable on Linux until an exact-object primitive is proven"
            .into(),
    ))
}

#[cfg(not(target_os = "linux"))]
fn ensure_identity_bound_final_mutation_supported() -> Result<(), SafetyError> {
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JournalEntry {
    pub ts_ms: u64,
    pub op: String,
    pub path: String,
    pub bytes: u64,
    pub outcome: String,
}

fn journal_io_err(e: std::io::Error) -> SafetyError {
    SafetyError::Journal(e.to_string())
}

fn journal_serde_err(e: serde_json::Error) -> SafetyError {
    SafetyError::Journal(e.to_string())
}

pub fn journal_append(journal_path: &Path, entry: &JournalEntry) -> Result<(), SafetyError> {
    use std::io::{Read, Seek, SeekFrom, Write};
    let line = serde_json::to_string(entry).map_err(journal_serde_err)?;
    let mut create_options = std::fs::OpenOptions::new();
    create_options.create_new(true).read(true).append(true);
    let (mut f, created) = match create_options.open(journal_path) {
        Ok(file) => (file, true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let file = std::fs::OpenOptions::new()
                .read(true)
                .append(true)
                .open(journal_path)
                .map_err(journal_io_err)?;
            (file, false)
        }
        Err(error) => return Err(journal_io_err(error)),
    };
    let mut healing = String::new();
    let len = f.seek(SeekFrom::End(0)).map_err(journal_io_err)?;
    if len > 0 {
        f.seek(SeekFrom::End(-1)).map_err(journal_io_err)?;
        let mut last = [0u8; 1];
        f.read_exact(&mut last).map_err(journal_io_err)?;
        if last[0] != b'\n' {
            healing.push('\n');
        }
    }
    f.write_all(format!("{healing}{line}\n").as_bytes())
        .and_then(|_| f.sync_all())
        .map_err(journal_io_err)?;
    #[cfg(unix)]
    if created {
        let parent = journal_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(journal_io_err)?;
    }
    #[cfg(not(unix))]
    let _ = created;
    Ok(())
}

pub fn journal_recent(journal_path: &Path, limit: usize) -> Vec<JournalEntry> {
    let Ok(content) = std::fs::read_to_string(journal_path) else {
        return Vec::new();
    };
    let mut entries: Vec<JournalEntry> = content
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    entries.reverse();
    entries.truncate(limit);
    entries
}

const STAGING_RECOVERY_JOURNAL_SUFFIX: &str = ".staging-recovery.jsonl";
const STAGING_AUTHORITY_JOURNAL_SUFFIX: &str = ".staging-authority.jsonl";

fn staging_recovery_journal_path(journal_path: &Path) -> Result<PathBuf, SafetyError> {
    sidecar(journal_path, STAGING_RECOVERY_JOURNAL_SUFFIX)
        .ok_or_else(|| SafetyError::Journal("staging recovery journal path is unavailable".into()))
}

fn staging_authority_journal_path(journal_path: &Path) -> Result<PathBuf, SafetyError> {
    sidecar(journal_path, STAGING_AUTHORITY_JOURNAL_SUFFIX)
        .ok_or_else(|| SafetyError::Journal("staging authority journal path is unavailable".into()))
}

/// Publishes identity-bound staging recovery even when the primary journal object is unusable.
/// The fallback has a deterministic path beside the configured journal, so recovery never needs
/// to discover private staging directories by scanning their names.
fn journal_append_staging_recovery(
    journal_path: &Path,
    entry: &JournalEntry,
) -> Result<(), SafetyError> {
    match journal_append(journal_path, entry) {
        Ok(()) => Ok(()),
        Err(primary_error) => {
            let recovery_journal = staging_recovery_journal_path(journal_path)?;
            journal_append(&recovery_journal, entry).map_err(|recovery_error| {
                SafetyError::Journal(format!(
                    "primary journal failed: {primary_error}; staging recovery journal failed: {recovery_error}"
                ))
            })
        }
    }
}

/// Arms staging recovery in a dedicated sidecar before a newly-created staging object is exposed
/// to post-create validation. This authority is independent of the primary and fallback journals
/// that may both be unavailable by the time a later rollback fails.
fn journal_append_staging_authority(
    journal_path: &Path,
    entry: &JournalEntry,
) -> Result<(), SafetyError> {
    journal_append(&staging_authority_journal_path(journal_path)?, entry)
}

fn staging_recovery_journal_recent(journal_path: &Path) -> Vec<JournalEntry> {
    let mut entries = staging_authority_journal_path(journal_path)
        .map(|path| journal_recent(&path, usize::MAX))
        .unwrap_or_default();
    entries.extend(
        staging_recovery_journal_path(journal_path)
            .map(|path| journal_recent(&path, usize::MAX))
            .unwrap_or_default(),
    );
    entries
}

#[cfg(windows)]
fn strip_verbatim(p: &Path) -> PathBuf {
    use std::path::{Component, Prefix};
    let mut comps = p.components();
    let Some(Component::Prefix(pr)) = comps.next() else {
        return p.to_path_buf();
    };
    match pr.kind() {
        Prefix::VerbatimDisk(d) => {
            let mut out = PathBuf::from(format!("{}:\\", d as char));
            out.extend(comps.filter(|c| !matches!(c, Component::RootDir)));
            out
        }
        Prefix::VerbatimUNC(server, share) => {
            let mut out = PathBuf::from(r"\\");
            out.push(server);
            out.push(share);
            out.extend(comps.filter(|c| !matches!(c, Component::RootDir)));
            out
        }
        _ => p.to_path_buf(),
    }
}

#[cfg(not(windows))]
fn strip_verbatim(p: &Path) -> PathBuf {
    p.to_path_buf()
}

fn normalize_for_guard(p: &Path) -> PathBuf {
    if let Ok(c) = std::fs::canonicalize(p) {
        return strip_verbatim(&c);
    }
    let mut suffix: Vec<std::ffi::OsString> = Vec::new();
    let mut cur = p;
    loop {
        match cur.parent() {
            Some(parent) => {
                suffix.extend(cur.file_name().map(|n| n.to_os_string()));
                if let Ok(c) = std::fs::canonicalize(parent) {
                    let mut base = strip_verbatim(&c);
                    for part in suffix.iter().rev() {
                        base.push(part);
                    }
                    return base;
                }
                cur = parent;
            }
            None => return strip_verbatim(p),
        }
    }
}

pub fn trash_delete(
    path: &Path,
    bytes: u64,
    journal_path: &Path,
    now_ms: u64,
) -> Result<(), SafetyError> {
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(SafetyError::Protected(path.to_path_buf()));
    }
    let guard_path =
        strip_verbatim(&std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()));
    let shared_temp = is_shared_temp_path(&guard_path);
    let shared_temp_authorized = shared_temp && is_user_owned_shared_temp_tree(&guard_path);
    if shared_temp && !shared_temp_authorized {
        return Err(SafetyError::Protected(path.to_path_buf()));
    }
    if !shared_temp_authorized && is_protected(&guard_path) {
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
    match platform_trash_delete(path) {
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

#[cfg(target_os = "macos")]
fn platform_trash_delete(path: &Path) -> Result<(), trash::Error> {
    use trash::macos::{DeleteMethod, TrashContextExtMacos};
    let mut context = trash::TrashContext::new();
    context.set_delete_method(DeleteMethod::NsFileManager);
    context.delete(path)
}

#[cfg(not(target_os = "macos"))]
fn platform_trash_delete(path: &Path) -> Result<(), trash::Error> {
    trash::delete(path)
}

static STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
thread_local! {
    static CATALOG_ROOT_AUTHORIZATION_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        std::cell::RefCell::new(None);
    static STAGING_CREATION_INTENT_PUBLISHED_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        std::cell::RefCell::new(None);
    static STAGING_CREATION_INTENT_COMPLETION_FAILURE: std::cell::Cell<bool> =
        std::cell::Cell::new(false);
    static STAGING_CREATED_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        std::cell::RefCell::new(None);
}

#[cfg(test)]
fn set_catalog_root_authorization_hook<F>(hook: F)
where
    F: FnOnce() + 'static,
{
    CATALOG_ROOT_AUTHORIZATION_HOOK.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(hook));
    });
}

#[cfg(test)]
fn run_catalog_root_authorization_hook() {
    CATALOG_ROOT_AUTHORIZATION_HOOK.with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook();
        }
    });
}

#[cfg(test)]
fn set_staging_creation_intent_published_hook<F>(hook: F)
where
    F: FnOnce() + 'static,
{
    STAGING_CREATION_INTENT_PUBLISHED_HOOK.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(hook));
    });
}

#[cfg(test)]
fn run_staging_creation_intent_published_hook() {
    STAGING_CREATION_INTENT_PUBLISHED_HOOK.with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook();
        }
    });
}

#[cfg(test)]
fn set_staging_creation_intent_completion_failure(fail: bool) {
    STAGING_CREATION_INTENT_COMPLETION_FAILURE.with(|value| value.set(fail));
}

#[cfg(test)]
fn staging_creation_intent_completion_must_fail() -> bool {
    STAGING_CREATION_INTENT_COMPLETION_FAILURE.with(std::cell::Cell::get)
}

#[cfg(test)]
fn set_staging_created_hook<F>(hook: F)
where
    F: FnOnce() + 'static,
{
    STAGING_CREATED_HOOK.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(hook));
    });
}

#[cfg(test)]
fn run_staging_created_hook() {
    STAGING_CREATED_HOOK.with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook();
        }
    });
}

/// Path and filesystem identities captured while the staging directory's parent was stable.
#[derive(Debug)]
struct PrivateStagingDir {
    path: PathBuf,
    name: String,
    object_id: String,
    source_parent_object_id: String,
}

fn real_directory_object_id(path: &Path) -> std::io::Result<String> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || is_windows_reparse_point(&metadata)
    {
        return Err(std::io::Error::other("path is not a real directory"));
    }
    filesystem_object_id(path)
}

/// Keeps rollback attached to the parent object until post-create validation succeeds.
struct StagingCreationGuard {
    parent: std::fs::File,
    object_id: Option<String>,
    #[cfg(any(unix, windows))]
    staging: Option<std::fs::File>,
    #[cfg(unix)]
    name: std::ffi::CString,
}

impl StagingCreationGuard {
    fn rollback(&self) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::fd::{AsRawFd, FromRawFd};

            let fd = unsafe {
                libc::openat(
                    self.parent.as_raw_fd(),
                    self.name.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            };
            if fd < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::NotFound {
                    return Ok(());
                }
                return Err(error);
            }
            let current = unsafe { std::fs::File::from_raw_fd(fd) };
            let current_id = object_id_from_metadata(&current.metadata()?).ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "filesystem object identity is unavailable on this platform",
                )
            })?;
            if self
                .object_id
                .as_deref()
                .is_some_and(|id| id != current_id.as_str())
            {
                return Err(std::io::Error::other(
                    "private staging directory identity changed during rollback",
                ));
            }

            // The parent descriptor still names the reviewed directory after a pathname rename.
            if unsafe {
                libc::unlinkat(
                    self.parent.as_raw_fd(),
                    self.name.as_ptr(),
                    libc::AT_REMOVEDIR,
                )
            } != 0
            {
                return Err(std::io::Error::last_os_error());
            }
            return Ok(());
        }
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use windows_sys::Win32::Foundation::HANDLE;
            use windows_sys::Win32::Storage::FileSystem::{
                FileDispositionInfo, SetFileInformationByHandle, FILE_DISPOSITION_INFO,
            };

            let staging = self.staging.as_ref().ok_or_else(|| {
                std::io::Error::other("private staging directory handle is unavailable")
            })?;
            let info = winapi_util::file::information(staging)?;
            let current_id = format!(
                "windows:{}:{}",
                info.volume_serial_number(),
                info.file_index()
            );
            if self
                .object_id
                .as_deref()
                .is_some_and(|id| id != current_id.as_str())
            {
                return Err(std::io::Error::other(
                    "private staging directory identity changed during rollback",
                ));
            }

            // Mark the retained filesystem object for deletion through its own handle. The child
            // may have been renamed and its old pathname replaced after identity capture; no
            // pathname lookup is allowed to choose what rollback removes.
            let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
            if unsafe {
                SetFileInformationByHandle(
                    staging.as_raw_handle() as HANDLE,
                    FileDispositionInfo,
                    std::ptr::from_ref(&disposition).cast(),
                    std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
                )
            } == 0
            {
                return Err(std::io::Error::last_os_error());
            }
            return Ok(());
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "private staging rollback handles are unavailable on this platform",
            ))
        }
    }
}

#[derive(Debug)]
enum StagingCreationError {
    Failed(std::io::Error),
    CleanupPending {
        staging_dir: PrivateStagingDir,
        error: String,
    },
}

impl From<std::io::Error> for StagingCreationError {
    fn from(error: std::io::Error) -> Self {
        Self::Failed(error)
    }
}

fn open_staging_parent(parent: &Path) -> std::io::Result<std::fs::File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        return std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(parent);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_SHARE_READ: u32 = 0x0000_0001;
        const FILE_SHARE_WRITE: u32 = 0x0000_0002;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        // Omitting FILE_SHARE_DELETE prevents parent rename/replacement while rollback is armed.
        return std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
            .open(parent);
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = parent;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "private staging parent handles are unavailable on this platform",
        ))
    }
}

fn staging_parent_object_id(
    _parent_path: &Path,
    parent: &std::fs::File,
) -> std::io::Result<String> {
    #[cfg(unix)]
    {
        return object_id_from_metadata(&parent.metadata()?).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "filesystem object identity is unavailable on this platform",
            )
        });
    }
    #[cfg(not(unix))]
    {
        let _ = parent;
        real_directory_object_id(_parent_path)
    }
}

#[cfg(unix)]
fn staging_name_c_string(name: &str) -> std::io::Result<std::ffi::CString> {
    std::ffi::CString::new(name).map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))
}

fn create_staging_child(
    parent: &std::fs::File,
    _candidate: &Path,
    name: &str,
) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;

        let name = staging_name_c_string(name)?;
        if unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o700) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        return Ok(());
    }
    #[cfg(not(unix))]
    {
        let _ = (parent, name);
        std::fs::create_dir(_candidate)
    }
}

fn secure_created_staging_child(
    guard: &mut StagingCreationGuard,
    _candidate: &Path,
    name: &str,
) -> std::io::Result<String> {
    #[cfg(unix)]
    {
        use std::os::fd::{AsRawFd, FromRawFd};

        let name = staging_name_c_string(name)?;
        let fd = unsafe {
            libc::openat(
                guard.parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let staging = unsafe { std::fs::File::from_raw_fd(fd) };
        let object_id = object_id_from_metadata(&staging.metadata()?).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "filesystem object identity is unavailable on this platform",
            )
        })?;
        guard.object_id = Some(object_id.clone());
        guard.staging = Some(staging);
        if unsafe {
            libc::fchmod(
                guard
                    .staging
                    .as_ref()
                    .expect("staging handle was just retained")
                    .as_raw_fd(),
                0o700,
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error());
        }
        return Ok(object_id);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        const DELETE: u32 = 0x0001_0000;
        const FILE_READ_ATTRIBUTES: u32 = 0x0000_0080;
        const FILE_SHARE_READ: u32 = 0x0000_0001;
        const FILE_SHARE_WRITE: u32 = 0x0000_0002;
        const FILE_SHARE_DELETE: u32 = 0x0000_0004;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

        let _ = name;
        let staging = std::fs::OpenOptions::new()
            .read(true)
            .access_mode(FILE_READ_ATTRIBUTES | DELETE)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
            .open(_candidate)?;
        let metadata = staging.metadata()?;
        if !metadata.is_dir() || is_windows_reparse_point(&metadata) {
            return Err(std::io::Error::other(
                "private staging child is not a real directory",
            ));
        }
        guard.staging = Some(staging);
        let info = winapi_util::file::information(
            guard
                .staging
                .as_ref()
                .expect("staging handle was just retained"),
        )?;
        let object_id = format!(
            "windows:{}:{}",
            info.volume_serial_number(),
            info.file_index()
        );
        guard.object_id = Some(object_id.clone());
        Ok(object_id)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (guard, _candidate, name);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "private staging child handles are unavailable on this platform",
        ))
    }
}

fn staging_creation_error(
    guard: StagingCreationGuard,
    path: PathBuf,
    name: String,
    source_parent_object_id: String,
    authority: Option<&StagingRecoveryAuthority<'_>>,
    error: std::io::Error,
) -> StagingCreationError {
    match guard.rollback() {
        Ok(()) => StagingCreationError::Failed(complete_staging_creation_intent(
            authority,
            &name,
            &source_parent_object_id,
            error,
        )),
        Err(cleanup_error) => match guard.object_id {
            Some(object_id) => StagingCreationError::CleanupPending {
                staging_dir: PrivateStagingDir {
                    path,
                    name,
                    object_id,
                    source_parent_object_id,
                },
                error: format!(
                    "{error}; private staging directory cleanup failed: {cleanup_error}"
                ),
            },
            None => StagingCreationError::Failed(std::io::Error::other(format!(
                "{error}; private staging directory cleanup failed before its identity was available: {cleanup_error}"
            ))),
        },
    }
}

struct StagingRecoveryAuthority<'a> {
    expected_object_id: &'a str,
    expected_catalog_root_id: Option<&'a str>,
    op: &'a str,
    bytes: u64,
    journal_path: &'a Path,
    now_ms: u64,
    source_path: &'a Path,
}

impl StagingRecoveryAuthority<'_> {
    fn publish_creation_intent(
        &self,
        staging_name: &str,
        source_parent_object_id: &str,
        complete: bool,
    ) -> Result<(), SafetyError> {
        #[cfg(test)]
        if complete && staging_creation_intent_completion_must_fail() {
            return Err(SafetyError::Journal(
                "simulated staging creation intent completion failure".into(),
            ));
        }
        let intent = StagingCreationIntent {
            source_parent_object_id: source_parent_object_id.into(),
            staging_name: staging_name.into(),
            target_object_id: self.expected_object_id.into(),
            catalog_root_object_id: self.expected_catalog_root_id.map(str::to_owned),
        };
        let entry = JournalEntry {
            ts_ms: self.now_ms,
            op: self.op.into(),
            path: self.source_path.to_string_lossy().into_owned(),
            bytes: self.bytes,
            outcome: staging_creation_intent_outcome(&intent, complete),
        };
        journal_append_staging_authority(self.journal_path, &entry)
    }

    fn entry(
        &self,
        staging_dir: &PrivateStagingDir,
        complete: bool,
    ) -> Result<JournalEntry, SafetyError> {
        let recovery = staging_cleanup_recovery(
            staging_dir,
            self.expected_object_id,
            self.expected_catalog_root_id,
        )?;
        Ok(JournalEntry {
            ts_ms: self.now_ms,
            op: self.op.into(),
            path: self.source_path.to_string_lossy().into_owned(),
            bytes: self.bytes,
            outcome: if complete {
                staging_cleanup_complete_outcome(&recovery)
            } else {
                staging_cleanup_pending_outcome(&recovery)
            },
        })
    }

    fn publish(&self, staging_dir: &PrivateStagingDir, complete: bool) -> Result<(), SafetyError> {
        journal_append_staging_authority(self.journal_path, &self.entry(staging_dir, complete)?)
    }
}

fn complete_staging_creation_intent(
    authority: Option<&StagingRecoveryAuthority<'_>>,
    staging_name: &str,
    source_parent_object_id: &str,
    error: std::io::Error,
) -> std::io::Error {
    let Some(authority) = authority else {
        return error;
    };
    match authority.publish_creation_intent(staging_name, source_parent_object_id, true) {
        Ok(()) => error,
        Err(completion_error) => std::io::Error::other(format!(
            "{error}; staging creation intent completion failed: {completion_error}"
        )),
    }
}

fn staging_creation_error_with_authority(
    guard: StagingCreationGuard,
    staging_dir: PrivateStagingDir,
    authority: &StagingRecoveryAuthority<'_>,
    close_creation_intent: bool,
    error: std::io::Error,
) -> StagingCreationError {
    match guard.rollback() {
        Ok(()) => {
            if let Err(completion_error) = authority.publish(&staging_dir, true) {
                return StagingCreationError::Failed(std::io::Error::other(format!(
                    "{error}; staging recovery authority completion failed: {completion_error}"
                )));
            }
            if close_creation_intent {
                StagingCreationError::Failed(complete_staging_creation_intent(
                    Some(authority),
                    &staging_dir.name,
                    &staging_dir.source_parent_object_id,
                    error,
                ))
            } else {
                // The attempted terminal publication itself failed, so retain its pending intent
                // while the object-bound completion proves that rollback removed the child.
                StagingCreationError::Failed(error)
            }
        }
        Err(cleanup_error) => StagingCreationError::CleanupPending {
            staging_dir,
            error: format!("{error}; private staging directory cleanup failed: {cleanup_error}"),
        },
    }
}

fn create_private_staging_dir(
    path: &Path,
    now_ms: u64,
) -> Result<PrivateStagingDir, StagingCreationError> {
    create_private_staging_dir_inner(path, now_ms, None)
}

fn create_private_staging_dir_inner(
    path: &Path,
    now_ms: u64,
    authority: Option<&StagingRecoveryAuthority<'_>>,
) -> Result<PrivateStagingDir, StagingCreationError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
    let parent_handle = open_staging_parent(&parent)?;
    let source_parent_object_id = staging_parent_object_id(&parent, &parent_handle)?;
    let pid = std::process::id();
    for _ in 0..32 {
        let serial = STAGING_COUNTER.fetch_add(1, Ordering::Relaxed);
        let name = format!(".disksage-trash-{}-{}-{}", pid, now_ms, serial);
        let candidate = parent.join(&name);
        if let Some(authority) = authority {
            authority
                .publish_creation_intent(&name, &source_parent_object_id, false)
                .map_err(|error| {
                    std::io::Error::other(format!(
                        "staging creation intent publication failed: {error}"
                    ))
                })?;
            #[cfg(test)]
            run_staging_creation_intent_published_hook();
        }
        match create_staging_child(&parent_handle, &candidate, &name) {
            Ok(()) => {
                let mut guard = StagingCreationGuard {
                    parent: parent_handle,
                    object_id: None,
                    #[cfg(any(unix, windows))]
                    staging: None,
                    #[cfg(unix)]
                    name: staging_name_c_string(&name)
                        .expect("generated staging names contain no NUL bytes"),
                };
                let object_id = match secure_created_staging_child(&mut guard, &candidate, &name) {
                    Ok(object_id) => object_id,
                    Err(error) => {
                        return Err(staging_creation_error(
                            guard,
                            candidate,
                            name,
                            source_parent_object_id,
                            authority,
                            error,
                        ))
                    }
                };
                let staging_dir = PrivateStagingDir {
                    path: candidate,
                    name,
                    object_id,
                    source_parent_object_id,
                };
                #[cfg(test)]
                run_staging_created_hook();
                if let Some(authority) = authority {
                    if let Err(publication_error) = authority.publish(&staging_dir, false) {
                        let error = std::io::Error::other(format!(
                            "staging recovery authority publication failed: {publication_error}"
                        ));
                        return Err(staging_creation_error(
                            guard,
                            staging_dir.path,
                            staging_dir.name,
                            staging_dir.source_parent_object_id,
                            Some(authority),
                            error,
                        ));
                    }
                }
                let confirmed_parent_object_id = match real_directory_object_id(&parent) {
                    Ok(object_id) => object_id,
                    Err(error) => {
                        return Err(match authority {
                            Some(authority) => staging_creation_error_with_authority(
                                guard,
                                staging_dir,
                                authority,
                                true,
                                error,
                            ),
                            None => staging_creation_error(
                                guard,
                                staging_dir.path,
                                staging_dir.name,
                                staging_dir.source_parent_object_id,
                                authority,
                                error,
                            ),
                        })
                    }
                };
                if confirmed_parent_object_id != staging_dir.source_parent_object_id {
                    let error = std::io::Error::other(
                        "private staging directory parent identity changed during creation",
                    );
                    return Err(match authority {
                        Some(authority) => staging_creation_error_with_authority(
                            guard,
                            staging_dir,
                            authority,
                            true,
                            error,
                        ),
                        None => staging_creation_error(
                            guard,
                            staging_dir.path,
                            staging_dir.name,
                            staging_dir.source_parent_object_id,
                            authority,
                            error,
                        ),
                    });
                }
                if let Some(authority) = authority {
                    if let Err(completion_error) = authority.publish_creation_intent(
                        &staging_dir.name,
                        &staging_dir.source_parent_object_id,
                        true,
                    ) {
                        let error = std::io::Error::other(format!(
                            "staging creation intent completion failed: {completion_error}"
                        ));
                        return Err(staging_creation_error_with_authority(
                            guard,
                            staging_dir,
                            authority,
                            false,
                            error,
                        ));
                    }
                }
                return Ok(staging_dir);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if let Some(authority) = authority {
                    if let Err(completion_error) = authority.publish_creation_intent(
                        &name,
                        &source_parent_object_id,
                        true,
                    ) {
                        return Err(std::io::Error::other(format!(
                            "{error}; staging creation intent completion failed: {completion_error}"
                        ))
                        .into());
                    }
                }
                continue;
            }
            Err(error) => {
                return Err(complete_staging_creation_intent(
                    authority,
                    &name,
                    &source_parent_object_id,
                    error,
                )
                .into())
            }
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate a private trash staging directory",
    )
    .into())
}

fn restore_staged_if_source_absent(
    path: &Path,
    staged: &Path,
    recovery: &StagingCleanupRecovery,
) -> Result<(), String> {
    let source_absent = matches!(
        std::fs::symlink_metadata(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    );
    if !source_absent {
        return Err(format!(
            "staged object retained at {}; source path reappeared",
            staged.display()
        ));
    }
    std::fs::rename(staged, path)
        .map_err(|error| format!("staged restore failed for {}: {error}", staged.display()))?;
    cleanup_verified_empty_staging_dir(path, recovery)?;
    Ok(())
}

fn remove_staged_permanently_with<F>(staged: &Path, remove: F) -> Result<(), SafetyError>
where
    F: FnOnce(&Path) -> std::io::Result<()>,
{
    if let Err(error) = remove(staged) {
        return Err(SafetyError::Trash(format!(
            "permanent deletion failed; staged object retained at {}: {error}",
            staged.display()
        )));
    }
    Ok(())
}

pub fn trash_delete_if_identity(
    path: &Path,
    expected_object_id: &str,
    bytes: u64,
    journal_path: &Path,
    now_ms: u64,
) -> Result<(), SafetyError> {
    trash_delete_if_identity_with_catalog_root(
        path,
        None,
        expected_object_id,
        bytes,
        journal_path,
        now_ms,
    )
}

pub(crate) fn trash_delete_if_identity_in_catalog_root(
    path: &Path,
    catalog_root: &Path,
    expected_object_id: &str,
    bytes: u64,
    journal_path: &Path,
    now_ms: u64,
) -> Result<(), SafetyError> {
    trash_delete_if_identity_with_catalog_root(
        path,
        Some(catalog_root),
        expected_object_id,
        bytes,
        journal_path,
        now_ms,
    )
}

#[cfg(windows)]
fn is_windows_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    // Win32 FILE_ATTRIBUTE_REPARSE_POINT. This rejects junctions, mount points, symbolic links,
    // and other reparse-backed directory roots instead of treating only symbolic links as unsafe.
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_windows_reparse_point(_metadata: &std::fs::Metadata) -> bool {
    false
}

const STAGING_CLEANUP_PENDING_PREFIX: &str = "mutated_cleanup_pending:";
const STAGING_CLEANUP_COMPLETE_PREFIX: &str = "mutated_cleanup_complete:";
const STAGING_CREATION_INTENT_PREFIX: &str = "staging_creation_intent:";
const STAGING_CREATION_INTENT_COMPLETE_PREFIX: &str = "staging_creation_intent_complete:";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct StagingCreationIntent {
    source_parent_object_id: String,
    staging_name: String,
    target_object_id: String,
    catalog_root_object_id: Option<String>,
}

type StagingCreationIntentIdentity = (String, String, String, Option<String>);

fn staging_creation_intent_identity(
    intent: &StagingCreationIntent,
) -> StagingCreationIntentIdentity {
    (
        intent.source_parent_object_id.clone(),
        intent.staging_name.clone(),
        intent.target_object_id.clone(),
        intent.catalog_root_object_id.clone(),
    )
}

fn staging_creation_intent_outcome(intent: &StagingCreationIntent, complete: bool) -> String {
    let encoded = serde_json::to_string(intent)
        .expect("staging creation intent contains only serializable strings");
    let prefix = if complete {
        STAGING_CREATION_INTENT_COMPLETE_PREFIX
    } else {
        STAGING_CREATION_INTENT_PREFIX
    };
    format!("{prefix}{encoded}")
}

fn parse_staging_creation_intent(outcome: &str) -> Option<(StagingCreationIntent, bool)> {
    if let Some(encoded) = outcome.strip_prefix(STAGING_CREATION_INTENT_COMPLETE_PREFIX) {
        return serde_json::from_str(encoded)
            .ok()
            .map(|intent| (intent, true));
    }
    serde_json::from_str(outcome.strip_prefix(STAGING_CREATION_INTENT_PREFIX)?)
        .ok()
        .map(|intent| (intent, false))
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct StagingCleanupRecovery {
    staging_name: String,
    staging_object_id: String,
    source_parent_object_id: Option<String>,
    target_object_id: String,
    catalog_root_object_id: Option<String>,
    error: String,
}

type StagingCleanupRecoveryIdentity = (String, String, Option<String>, String, Option<String>);

fn staging_cleanup_recovery_identity(
    recovery: &StagingCleanupRecovery,
) -> StagingCleanupRecoveryIdentity {
    (
        recovery.staging_name.clone(),
        recovery.staging_object_id.clone(),
        recovery.source_parent_object_id.clone(),
        recovery.target_object_id.clone(),
        recovery.catalog_root_object_id.clone(),
    )
}

fn staging_cleanup_pending_outcome(recovery: &StagingCleanupRecovery) -> String {
    let encoded = serde_json::to_string(recovery)
        .expect("staging cleanup recovery contains only serializable strings");
    format!("{STAGING_CLEANUP_PENDING_PREFIX}{encoded}")
}

fn staging_cleanup_complete_outcome(recovery: &StagingCleanupRecovery) -> String {
    let encoded = serde_json::to_string(recovery)
        .expect("staging cleanup recovery contains only serializable strings");
    format!("{STAGING_CLEANUP_COMPLETE_PREFIX}{encoded}")
}

fn parse_staging_cleanup_pending(outcome: &str) -> Option<StagingCleanupRecovery> {
    serde_json::from_str(outcome.strip_prefix(STAGING_CLEANUP_PENDING_PREFIX)?).ok()
}

fn parse_staging_cleanup_complete(outcome: &str) -> Option<StagingCleanupRecovery> {
    serde_json::from_str(outcome.strip_prefix(STAGING_CLEANUP_COMPLETE_PREFIX)?).ok()
}

fn mutated_recovery_publication_error(
    journal_error: SafetyError,
    result: &Result<(), SafetyError>,
) -> SafetyError {
    let state = match result {
        Ok(()) => "staging cleanup completed".to_string(),
        Err(error) => error.to_string(),
    };
    SafetyError::Trash(format!(
        "mutation completed; durable recovery evidence publication failed: {journal_error}; {state}"
    ))
}

fn is_disksage_staging_name(name: &str) -> bool {
    let Some(suffix) = name.strip_prefix(".disksage-trash-") else {
        return false;
    };
    let mut parts = suffix.split('-');
    (0..3).all(|_| {
        parts
            .next()
            .is_some_and(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    }) && parts.next().is_none()
}

#[cfg(test)]
thread_local! {
    static STAGING_CLEANUP_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce(&Path) -> std::io::Result<()>>>> =
        std::cell::RefCell::new(None);
}

#[cfg(test)]
fn set_staging_cleanup_hook<F>(hook: F)
where
    F: FnOnce(&Path) -> std::io::Result<()> + 'static,
{
    STAGING_CLEANUP_HOOK.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(hook));
    });
}

fn remove_staging_dir(path: &Path) -> std::io::Result<()> {
    #[cfg(test)]
    if let Some(hook) = STAGING_CLEANUP_HOOK.with(|slot| slot.borrow_mut().take()) {
        return hook(path);
    }
    std::fs::remove_dir(path)
}

fn staging_cleanup_recovery(
    staging_dir: &PrivateStagingDir,
    expected_object_id: &str,
    expected_catalog_root_id: Option<&str>,
) -> Result<StagingCleanupRecovery, SafetyError> {
    if !is_disksage_staging_name(&staging_dir.name) {
        return Err(SafetyError::Trash(
            "private staging directory name is invalid".into(),
        ));
    }
    Ok(StagingCleanupRecovery {
        staging_name: staging_dir.name.clone(),
        staging_object_id: staging_dir.object_id.clone(),
        source_parent_object_id: Some(staging_dir.source_parent_object_id.clone()),
        target_object_id: expected_object_id.into(),
        catalog_root_object_id: expected_catalog_root_id.map(str::to_owned),
        error: String::new(),
    })
}

fn create_private_staging_dir_for_operation(
    path: &Path,
    expected_object_id: &str,
    expected_catalog_root_id: Option<&str>,
    op: &str,
    bytes: u64,
    journal_path: &Path,
    now_ms: u64,
) -> Result<PrivateStagingDir, SafetyError> {
    let authority = StagingRecoveryAuthority {
        expected_object_id,
        expected_catalog_root_id,
        op,
        bytes,
        journal_path,
        now_ms,
        source_path: path,
    };
    match create_private_staging_dir_inner(path, now_ms, Some(&authority)) {
        Ok(staging_dir) => Ok(staging_dir),
        Err(StagingCreationError::Failed(error)) => Err(SafetyError::Trash(error.to_string())),
        Err(StagingCreationError::CleanupPending { error, .. }) => Err(SafetyError::Trash(
            format!("staging creation cleanup remains pending: {error}"),
        )),
    }
}

fn cleanup_verified_empty_staging_dir(
    source_path: &Path,
    recovery: &StagingCleanupRecovery,
) -> Result<(), String> {
    if !is_disksage_staging_name(&recovery.staging_name) {
        return Err("private staging directory name is invalid".into());
    }
    let expected_parent_id = recovery
        .source_parent_object_id
        .as_deref()
        .ok_or_else(|| "private staging recovery lacks source parent identity".to_string())?;
    let parent = source_path
        .parent()
        .ok_or_else(|| "private staging directory parent is unavailable".to_string())?;
    let parent_metadata = std::fs::symlink_metadata(parent)
        .map_err(|error| format!("private staging directory parent is unavailable: {error}"))?;
    if !parent_metadata.is_dir()
        || parent_metadata.file_type().is_symlink()
        || is_windows_reparse_point(&parent_metadata)
    {
        return Err("private staging directory parent is not a real directory".into());
    }
    let actual_parent_id = filesystem_object_id(parent).map_err(|error| {
        format!("private staging directory parent identity is unavailable: {error}")
    })?;
    if actual_parent_id != expected_parent_id {
        return Err("private staging directory parent identity changed".into());
    }
    let staging_dir = parent.join(&recovery.staging_name);
    let metadata = match std::fs::symlink_metadata(&staging_dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "private staging directory metadata is unavailable: {error}"
            ))
        }
    };
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || is_windows_reparse_point(&metadata)
    {
        return Err("private staging cleanup target is not a real directory".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        if metadata.uid() != unsafe { libc::geteuid() }
            || metadata.permissions().mode() & 0o777 != 0o700
        {
            return Err("private staging cleanup target is not DiskSage-owned".into());
        }
    }
    let actual_id = filesystem_object_id(&staging_dir)
        .map_err(|error| format!("private staging directory identity is unavailable: {error}"))?;
    if actual_id != recovery.staging_object_id {
        return Err("private staging directory identity changed".into());
    }
    let mut entries = std::fs::read_dir(&staging_dir)
        .map_err(|error| format!("private staging directory is unreadable: {error}"))?;
    if entries
        .next()
        .transpose()
        .map_err(|error| format!("private staging directory is unreadable: {error}"))?
        .is_some()
    {
        return Err("private staging directory is not empty".into());
    }
    // This stays non-recursive and fails if anything appears after the emptiness check. The final
    // pathname-to-object mutation gap remains until this boundary can remove by an identity-bound
    // directory handle on every supported platform.
    remove_staging_dir(&staging_dir)
        .map_err(|error| format!("private staging directory cleanup failed: {error}"))
}

fn cleanup_staging_dir_for_operation(
    source_path: &Path,
    recovery: &StagingCleanupRecovery,
    op: &str,
    bytes: u64,
    journal_path: &Path,
    now_ms: u64,
) -> Result<(), String> {
    cleanup_verified_empty_staging_dir(source_path, recovery)?;
    let entry = JournalEntry {
        ts_ms: now_ms,
        op: op.into(),
        path: source_path.to_string_lossy().into_owned(),
        bytes,
        outcome: staging_cleanup_complete_outcome(recovery),
    };
    journal_append_staging_authority(journal_path, &entry).map_err(|error| {
        format!("private staging directory was removed, but recovery authority completion failed: {error}")
    })
}

fn restore_staged_for_operation(
    source_path: &Path,
    staged: &Path,
    recovery: &StagingCleanupRecovery,
    op: &str,
    bytes: u64,
    journal_path: &Path,
    now_ms: u64,
) -> Result<(), String> {
    let source_absent = matches!(
        std::fs::symlink_metadata(source_path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    );
    if !source_absent {
        return Err(format!(
            "staged object retained at {}; source path reappeared",
            staged.display()
        ));
    }
    std::fs::rename(staged, source_path)
        .map_err(|error| format!("staged restore failed for {}: {error}", staged.display()))?;
    cleanup_staging_dir_for_operation(source_path, recovery, op, bytes, journal_path, now_ms)
}

fn retry_pending_staging_cleanup(
    path: &Path,
    expected_object_id: &str,
    expected_catalog_root_id: Option<&str>,
    op: &str,
    bytes: u64,
    journal_path: &Path,
    now_ms: u64,
) -> Option<Result<(), SafetyError>> {
    let journal_path_value = path.to_string_lossy();
    // Fallback entries precede primary entries when timestamps tie, so a later terminal receipt
    // in the primary journal supersedes its emergency pending receipt deterministically.
    let mut entries = staging_recovery_journal_recent(journal_path);
    entries.reverse();
    let mut primary_entries = journal_recent(journal_path, usize::MAX);
    primary_entries.reverse();
    entries.extend(primary_entries);
    entries.sort_by_key(|entry| {
        (
            entry.ts_ms,
            parse_staging_cleanup_complete(&entry.outcome).is_some(),
        )
    });
    let matching_entries: Vec<_> = entries
        .into_iter()
        .filter(|entry| entry.op == op && entry.path == journal_path_value && entry.bytes == bytes)
        .collect();
    let mut recoveries = std::collections::HashMap::new();
    let mut creation_intents = std::collections::HashMap::new();
    for (sequence, entry) in matching_entries.into_iter().enumerate() {
        if let Some((intent, complete)) = parse_staging_creation_intent(&entry.outcome) {
            creation_intents.insert(
                staging_creation_intent_identity(&intent),
                (intent, complete),
            );
        } else if let Some(recovery) = parse_staging_cleanup_pending(&entry.outcome) {
            recoveries.insert(
                staging_cleanup_recovery_identity(&recovery),
                (recovery, false, sequence),
            );
        } else if let Some(recovery) = parse_staging_cleanup_complete(&entry.outcome) {
            if let Some(state) = recoveries.get_mut(&staging_cleanup_recovery_identity(&recovery)) {
                *state = (recovery, true, sequence);
            }
        }
    }

    creation_intents.retain(|_, (intent, complete)| {
        !*complete
            && !recoveries.values().any(|(recovery, _, _)| {
                recovery.staging_name == intent.staging_name
                    && recovery.source_parent_object_id.as_deref()
                        == Some(intent.source_parent_object_id.as_str())
                    && recovery.target_object_id == intent.target_object_id
                    && recovery.catalog_root_object_id == intent.catalog_root_object_id
            })
    });
    if creation_intents.values().any(|(intent, _)| {
        intent.target_object_id == expected_object_id
            && intent.catalog_root_object_id.as_deref() == expected_catalog_root_id
    }) {
        return Some(Err(SafetyError::Trash(
            "staging creation intent remains pending; exact child identity was not durably published"
                .into(),
        )));
    }
    if !creation_intents.is_empty() {
        return Some(Err(SafetyError::Protected(path.to_path_buf())));
    }

    let matches_expected = |recovery: &StagingCleanupRecovery| {
        recovery.target_object_id == expected_object_id
            && recovery.catalog_root_object_id.as_deref() == expected_catalog_root_id
    };
    if recoveries.values().any(|(recovery, _, _)| {
        matches_expected(recovery) && recovery.source_parent_object_id.is_none()
    }) {
        return Some(Err(SafetyError::Trash(
            "mutation completed; legacy staging recovery lacks source parent identity; cleanup remains pending"
                .into(),
        )));
    }
    let pending = recoveries
        .values()
        .filter(|(recovery, complete, _)| !complete && matches_expected(recovery))
        .max_by_key(|(_, _, sequence)| sequence)
        .map(|(recovery, _, _)| recovery.clone());
    let Some(mut recovery) = pending else {
        let completed = recoveries
            .values()
            .filter(|(recovery, complete, _)| *complete && matches_expected(recovery))
            .max_by_key(|(_, _, sequence)| sequence);
        return if completed.is_some() {
            Some(Ok(()))
        } else if recoveries.is_empty() {
            None
        } else {
            Some(Err(SafetyError::Protected(path.to_path_buf())))
        };
    };

    if recoveries
        .values()
        .any(|(candidate, complete, _)| {
            !complete
                && matches_expected(candidate)
                && staging_cleanup_recovery_identity(candidate)
                    != staging_cleanup_recovery_identity(&recovery)
        })
    {
        return Some(Err(SafetyError::Protected(path.to_path_buf())));
    }
    let cleanup = cleanup_verified_empty_staging_dir(path, &recovery);
    let mut entry = JournalEntry {
        ts_ms: now_ms,
        op: op.into(),
        path: journal_path_value.into_owned(),
        bytes,
        outcome: staging_cleanup_complete_outcome(&recovery),
    };
    let result = match cleanup {
        Ok(()) => Ok(()),
        Err(error) => {
            recovery.error = error.clone();
            entry.outcome = staging_cleanup_pending_outcome(&recovery);
            Err(SafetyError::Trash(format!(
                "mutation completed; staging cleanup remains pending: {error}"
            )))
        }
    };
    Some(
        match journal_append_staging_recovery(journal_path, &entry) {
            Ok(()) => result,
            Err(error) => Err(mutated_recovery_publication_error(error, &result)),
        },
    )
}

fn revalidate_catalog_root_before_staging(
    path: &Path,
    root: &Path,
    expected_catalog_root_id: &str,
    expected_object_id: &str,
) -> Result<(), SafetyError> {
    let metadata = std::fs::symlink_metadata(root)
        .map_err(|_| SafetyError::Protected(path.to_path_buf()))?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || is_windows_reparse_point(&metadata)
    {
        return Err(SafetyError::Protected(path.to_path_buf()));
    }
    let current_root_id = filesystem_object_id(root)
        .map_err(|_| SafetyError::Protected(path.to_path_buf()))?;
    if current_root_id != expected_catalog_root_id {
        return Err(SafetyError::Protected(path.to_path_buf()));
    }
    let canonical_root = strip_verbatim(
        &std::fs::canonicalize(root).map_err(|_| SafetyError::Protected(path.to_path_buf()))?,
    );
    let canonical_path = strip_verbatim(
        &std::fs::canonicalize(path).map_err(|_| SafetyError::Protected(path.to_path_buf()))?,
    );
    if canonical_path.parent() != Some(canonical_root.as_path())
        || is_explicitly_protected(&canonical_path)
    {
        return Err(SafetyError::Protected(path.to_path_buf()));
    }
    let current_target_id = filesystem_object_id(path)
        .map_err(|error| SafetyError::Trash(format!("object identity unavailable: {error}")))?;
    if current_target_id != expected_object_id {
        return Err(SafetyError::Trash(
            "개발 아티팩트의 파일시스템 객체가 바뀌었습니다. 다시 스캔하세요".into(),
        ));
    }
    Ok(())
}

fn trash_delete_if_identity_with_catalog_root(
    path: &Path,
    catalog_root: Option<&Path>,
    expected_object_id: &str,
    bytes: u64,
    journal_path: &Path,
    now_ms: u64,
) -> Result<(), SafetyError> {
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(SafetyError::Protected(path.to_path_buf()));
    }
    let guard_path =
        strip_verbatim(&std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()));
    let mut expected_catalog_root_id = None;
    let catalog_authorized = if let Some(root) = catalog_root {
        let initial_catalog_root_id = filesystem_object_id(root)
            .map_err(|_| SafetyError::Protected(path.to_path_buf()))?;
        #[cfg(test)]
        run_catalog_root_authorization_hook();
        let metadata = std::fs::symlink_metadata(root)
            .map_err(|_| SafetyError::Protected(path.to_path_buf()))?;
        let authorized = metadata.is_dir()
            && !metadata.file_type().is_symlink()
            && !is_windows_reparse_point(&metadata)
            && std::fs::canonicalize(root)
                .map(|root| strip_verbatim(&root))
                .is_ok_and(|root| guard_path.parent() == Some(root.as_path()))
            && !is_explicitly_protected(&guard_path);
        if authorized {
            let confirmed_catalog_root_id = filesystem_object_id(root)
                .map_err(|_| SafetyError::Protected(path.to_path_buf()))?;
            if confirmed_catalog_root_id != initial_catalog_root_id {
                return Err(SafetyError::Protected(path.to_path_buf()));
            }
            expected_catalog_root_id = Some(initial_catalog_root_id);
        }
        authorized
    } else {
        false
    };
    if catalog_root.is_some() && !catalog_authorized {
        return Err(SafetyError::Protected(path.to_path_buf()));
    }
    let shared_temp = is_shared_temp_path(&guard_path);
    let shared_temp_authorized = shared_temp && is_user_owned_shared_temp_tree(&guard_path);
    if shared_temp && !shared_temp_authorized {
        return Err(SafetyError::Protected(path.to_path_buf()));
    }
    if !shared_temp_authorized && !catalog_authorized && is_protected(&guard_path) {
        return Err(SafetyError::Protected(path.to_path_buf()));
    }
    if matches!(
        std::fs::symlink_metadata(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    ) {
        if let Some(retry) = retry_pending_staging_cleanup(
            path,
            expected_object_id,
            expected_catalog_root_id.as_deref(),
            "trash_delete",
            bytes,
            journal_path,
            now_ms,
        ) {
            return retry;
        }
    }
    let actual = filesystem_object_id(path)
        .map_err(|error| SafetyError::Trash(format!("object identity unavailable: {error}")))?;
    if actual != expected_object_id {
        return Err(SafetyError::Trash(
            "개발 아티팩트의 파일시스템 객체가 바뀌었습니다. 다시 스캔하세요".into(),
        ));
    }
    ensure_identity_bound_final_mutation_supported()?;
    let file_name = path.file_name().ok_or_else(|| {
        SafetyError::Trash("개발 아티팩트의 파일명이 없습니다. 다시 스캔하세요".into())
    })?;
    let staging_dir = create_private_staging_dir_for_operation(
        path,
        expected_object_id,
        expected_catalog_root_id.as_deref(),
        "trash_delete",
        bytes,
        journal_path,
        now_ms,
    )?;
    let staged = staging_dir.path.join(file_name);
    let recovery = staging_cleanup_recovery(
        &staging_dir,
        expected_object_id,
        expected_catalog_root_id.as_deref(),
    )?;
    let cleanup_staging = || {
        cleanup_staging_dir_for_operation(
            path,
            &recovery,
            "trash_delete",
            bytes,
            journal_path,
            now_ms,
        )
    };
    let restore_staged = || {
        restore_staged_for_operation(
            path,
            &staged,
            &recovery,
            "trash_delete",
            bytes,
            journal_path,
            now_ms,
        )
    };
    let mut entry = JournalEntry {
        ts_ms: now_ms,
        op: "trash_delete".into(),
        path: path.to_string_lossy().into_owned(),
        bytes,
        outcome: "pending".into(),
    };
    if let Err(error) = journal_append(journal_path, &entry) {
        return match cleanup_staging() {
            Ok(()) => Err(error),
            Err(cleanup_error) => Err(SafetyError::Journal(format!(
                "{error}; private staging directory cleanup failed: {cleanup_error}"
            ))),
        };
    }

    let mut cleanup_pending = None;
    let result = (|| -> Result<(), SafetyError> {
        if let (Some(root), Some(expected_catalog_root_id)) =
            (catalog_root, expected_catalog_root_id.as_deref())
        {
            let revalidation = (|| -> Result<(), SafetyError> {
                revalidate_catalog_root_before_staging(
                    path,
                    root,
                    expected_catalog_root_id,
                    expected_object_id,
                )?;
                Ok(())
            })();
            if let Err(error) = revalidation {
                return match cleanup_staging() {
                    Ok(()) => Err(error),
                    Err(cleanup_error) => Err(SafetyError::Trash(format!(
                        "{error}; private staging directory cleanup failed: {cleanup_error}"
                    ))),
                };
            }
        }
        if let Err(error) = std::fs::rename(path, &staged) {
            let mutation_error = SafetyError::Trash(format!("atomic staging move failed: {error}"));
            return match cleanup_staging() {
                Ok(()) => Err(mutation_error),
                Err(cleanup_error) => Err(SafetyError::Trash(format!(
                    "{mutation_error}; private staging directory cleanup failed: {cleanup_error}"
                ))),
            };
        }
        let moved_id = filesystem_object_id(&staged).map_err(|error| {
            let restore = restore_staged();
            match restore {
                Ok(()) => {
                    SafetyError::Trash(format!("staged object identity unavailable: {error}"))
                }
                Err(restore_error) => SafetyError::Trash(format!(
                    "staged object identity unavailable: {error}; {restore_error}"
                )),
            }
        })?;
        if moved_id != expected_object_id {
            return match restore_staged() {
                Ok(()) => Err(SafetyError::Trash(
                    "atomic staging move changed the filesystem object; nothing was trashed".into(),
                )),
                Err(restore_error) => Err(SafetyError::Trash(format!(
                    "atomic staging move changed the filesystem object; {restore_error}"
                ))),
            };
        }
        if let Err(error) = platform_trash_delete(&staged) {
            return match restore_staged() {
                Ok(()) => Err(SafetyError::Trash(error.to_string())),
                Err(restore_error) => {
                    Err(SafetyError::Trash(format!("{}; {restore_error}", error)))
                }
            };
        }
        match cleanup_staging() {
            Ok(()) => Ok(()),
            Err(error) => {
                let mut pending = recovery.clone();
                pending.error = error.clone();
                cleanup_pending = Some(pending);
                Err(SafetyError::Trash(format!(
                    "mutation completed; staging cleanup remains pending: {error}"
                )))
            }
        }
    })();
    entry.outcome = match (&result, cleanup_pending.as_ref()) {
        (_, Some(pending)) => staging_cleanup_pending_outcome(pending),
        (Ok(()), None) => "ok".into(),
        (Err(error), None) => format!("error:{error}"),
    };
    match journal_append(journal_path, &entry) {
        Ok(()) => result,
        Err(error) if result.is_ok() || cleanup_pending.is_some() => {
            Err(mutated_recovery_publication_error(error, &result))
        }
        Err(error) => Err(error),
    }
}

/// Permanently remove one unchanged, current-user-owned generated directory.
///
/// Callers must perform their domain-specific regenerability and active-use checks first. This
/// boundary rechecks path safety and filesystem identity, journals both intent and outcome, and
/// never follows a symbolic-link root.
pub fn permanent_delete_dir_if_identity(
    path: &Path,
    expected_object_id: &str,
    bytes: u64,
    journal_path: &Path,
    now_ms: u64,
) -> Result<(), SafetyError> {
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(SafetyError::Protected(path.to_path_buf()));
    }
    let guard_path =
        strip_verbatim(&std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()));
    let shared_temp_authorized =
        is_shared_temp_path(&guard_path) && is_user_owned_shared_temp_tree(&guard_path);
    if !shared_temp_authorized && is_protected(&guard_path) {
        return Err(SafetyError::Protected(path.to_path_buf()));
    }
    if matches!(
        std::fs::symlink_metadata(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    ) {
        if let Some(retry) = retry_pending_staging_cleanup(
            path,
            expected_object_id,
            None,
            "permanent_generated_directory_delete",
            bytes,
            journal_path,
            now_ms,
        ) {
            return retry;
        }
    }
    let metadata =
        std::fs::symlink_metadata(path).map_err(|error| SafetyError::Trash(error.to_string()))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(SafetyError::Trash(
            "permanent deletion requires a real generated directory".into(),
        ));
    }
    let actual = filesystem_object_id(path)
        .map_err(|error| SafetyError::Trash(format!("object identity unavailable: {error}")))?;
    if actual != expected_object_id {
        return Err(SafetyError::Trash(
            "generated directory identity changed; rescan before deletion".into(),
        ));
    }
    ensure_identity_bound_final_mutation_supported()?;
    let file_name = path.file_name().ok_or_else(|| {
        SafetyError::Trash("generated directory has no file name; rescan before deletion".into())
    })?;
    let staging_dir = create_private_staging_dir_for_operation(
        path,
        expected_object_id,
        None,
        "permanent_generated_directory_delete",
        bytes,
        journal_path,
        now_ms,
    )?;
    let staged = staging_dir.path.join(file_name);
    let recovery = staging_cleanup_recovery(&staging_dir, expected_object_id, None)?;
    let cleanup_staging = || {
        cleanup_staging_dir_for_operation(
            path,
            &recovery,
            "permanent_generated_directory_delete",
            bytes,
            journal_path,
            now_ms,
        )
    };
    let restore_staged = || {
        restore_staged_for_operation(
            path,
            &staged,
            &recovery,
            "permanent_generated_directory_delete",
            bytes,
            journal_path,
            now_ms,
        )
    };
    let mut entry = JournalEntry {
        ts_ms: now_ms,
        op: "permanent_generated_directory_delete".into(),
        path: path.to_string_lossy().into_owned(),
        bytes,
        outcome: "pending".into(),
    };
    if let Err(error) = journal_append(journal_path, &entry) {
        return match cleanup_staging() {
            Ok(()) => Err(error),
            Err(cleanup_error) => Err(SafetyError::Journal(format!(
                "{error}; private staging directory cleanup failed: {cleanup_error}"
            ))),
        };
    }
    let mut cleanup_pending = None;
    let result = (|| -> Result<(), SafetyError> {
        if let Err(error) = std::fs::rename(path, &staged) {
            let mutation_error = SafetyError::Trash(format!("atomic staging move failed: {error}"));
            return match cleanup_staging() {
                Ok(()) => Err(mutation_error),
                Err(cleanup_error) => Err(SafetyError::Trash(format!(
                    "{mutation_error}; private staging directory cleanup failed: {cleanup_error}"
                ))),
            };
        }
        let moved_id = filesystem_object_id(&staged).map_err(|error| {
            let restore = restore_staged();
            match restore {
                Ok(()) => SafetyError::Trash(format!(
                    "staged generated directory identity unavailable: {error}"
                )),
                Err(restore_error) => SafetyError::Trash(format!(
                    "staged generated directory identity unavailable: {error}; {restore_error}"
                )),
            }
        })?;
        if moved_id != expected_object_id {
            return match restore_staged() {
                Ok(()) => Err(SafetyError::Trash(
                    "atomic staging move changed the generated directory; nothing was deleted"
                        .into(),
                )),
                Err(restore_error) => Err(SafetyError::Trash(format!(
                    "atomic staging move changed the generated directory; {restore_error}"
                ))),
            };
        }
        remove_staged_permanently_with(&staged, |path| std::fs::remove_dir_all(path))?;
        match cleanup_staging() {
            Ok(()) => Ok(()),
            Err(error) => {
                let mut pending = recovery.clone();
                pending.error = error.clone();
                cleanup_pending = Some(pending);
                Err(SafetyError::Trash(format!(
                    "mutation completed; staging cleanup remains pending: {error}"
                )))
            }
        }
    })();
    entry.outcome = match (&result, cleanup_pending.as_ref()) {
        (_, Some(pending)) => staging_cleanup_pending_outcome(pending),
        (Ok(()), None) => "ok".into(),
        (Err(error), None) => format!("error:{error}"),
    };
    match journal_append(journal_path, &entry) {
        Ok(()) => result,
        Err(error) if result.is_ok() || cleanup_pending.is_some() => {
            Err(mutated_recovery_publication_error(error, &result))
        }
        Err(error) => Err(error),
    }
}

pub fn same_volume(src: &Path, dst: &Path) -> bool {
    let dst_probe = dst.parent().unwrap_or(dst);
    #[cfg(windows)]
    {
        fn drive(p: &Path) -> Option<String> {
            p.components().next().and_then(|c| match c {
                std::path::Component::Prefix(pr) => {
                    Some(pr.as_os_str().to_string_lossy().to_lowercase())
                }
                _ => None,
            })
        }
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

fn copy_then_hash(
    src: &Path,
    dst: &Path,
) -> std::io::Result<(u64, u64, Result<String, String>, Result<String, String>)> {
    {
        let mut src_file = std::fs::File::open(src)?;
        let mut dst_file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dst)?;
        std::io::copy(&mut src_file, &mut dst_file)?;
    }
    let src_len = std::fs::metadata(src)?.len();
    let dst_len = std::fs::metadata(dst)?.len();
    let src_hash = crate::dupes::hash_full(src);
    let dst_hash = crate::dupes::hash_full(dst);
    Ok((src_len, dst_len, src_hash, dst_hash))
}

fn hashes_match(
    src_hash: &Result<String, String>,
    dst_hash: &Result<String, String>,
    src_len: u64,
    dst_len: u64,
) -> bool {
    matches!((src_hash, dst_hash), (Ok(s), Ok(d)) if src_len == dst_len && s == d)
}

fn finalize_verified_copy(dst: &Path, verified: bool) -> std::io::Result<()> {
    if verified {
        Ok(())
    } else {
        let _ = std::fs::remove_file(dst);
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "복사 검증 실패",
        ))
    }
}

fn preserve_source_metadata(src: &Path, dst: &Path) -> std::io::Result<()> {
    let src_md = std::fs::metadata(src)?;
    let mut times = std::fs::FileTimes::new().set_modified(src_md.modified()?);
    if let Ok(accessed) = src_md.accessed() {
        times = times.set_accessed(accessed);
    }
    std::fs::OpenOptions::new()
        .write(true)
        .open(dst)?
        .set_times(times)?;
    std::fs::set_permissions(dst, src_md.permissions())?;
    Ok(())
}

fn copy_verified_io(src: &Path, dst: &Path) -> std::io::Result<()> {
    let (src_len, dst_len, src_hash, dst_hash) = match copy_then_hash(src, dst) {
        Ok(v) => v,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::AlreadyExists {
                let _ = std::fs::remove_file(dst);
            }
            return Err(e);
        }
    };
    finalize_verified_copy(dst, hashes_match(&src_hash, &dst_hash, src_len, dst_len))?;
    if let Err(e) = preserve_source_metadata(src, dst) {
        let _ = std::fs::remove_file(dst);
        return Err(e);
    }
    Ok(())
}

fn hardlink_move_io(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::hard_link(src, dst)?;
    std::fs::remove_file(src)?;
    Ok(())
}

fn do_move(
    src: &Path,
    dst: &Path,
    same_vol: bool,
    journal_path: &Path,
    now_ms: u64,
) -> Result<(), SafetyError> {
    let mut entry = JournalEntry {
        ts_ms: now_ms,
        op: "move".into(),
        path: format!("{} -> {}", src.display(), dst.display()),
        bytes: std::fs::metadata(src).map(|m| m.len()).unwrap_or(0),
        outcome: "pending".into(),
    };
    journal_append(journal_path, &entry)?;

    let result = if same_vol {
        hardlink_move_io(src, dst).map_err(|e| SafetyError::Trash(e.to_string()))
    } else {
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

pub fn move_file(
    src: &Path,
    dst: &Path,
    journal_path: &Path,
    now_ms: u64,
) -> Result<(), SafetyError> {
    for p in [src, dst] {
        if p.components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(SafetyError::Protected(p.to_path_buf()));
        }
        let guard = normalize_for_guard(p);
        if is_protected(&guard) {
            return Err(SafetyError::Protected(p.to_path_buf()));
        }
    }
    if dst.exists() {
        return Err(SafetyError::Trash(format!(
            "목적지가 이미 존재: {}",
            dst.display()
        )));
    }
    let dst_parent = dst.parent().unwrap_or(dst);
    std::fs::create_dir_all(dst_parent).map_err(|e| SafetyError::Trash(e.to_string()))?;
    do_move(src, dst, same_volume(src, dst), journal_path, now_ms)
}

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

    #[cfg(target_os = "macos")]
    #[test]
    fn protects_macos_system_paths() {
        for p in [
            "/System",
            "/System/Library/CoreServices",
            "/Library",
            "/Applications",
            "/private/etc",
            "/Volumes/Macintosh HD",
            "/cores",
            "/Network",
        ] {
            assert!(is_protected(Path::new(p)), "{p} must be protected on macOS");
        }
    }

    #[test]
    fn safety_error_display_messages() {
        assert!(SafetyError::Protected(PathBuf::from("/x"))
            .to_string()
            .contains("보호"));
        assert!(SafetyError::Trash("boom".into())
            .to_string()
            .contains("휴지통"));
        assert!(SafetyError::Journal("boom".into())
            .to_string()
            .contains("저널"));
    }

    #[test]
    fn is_home_root_false_when_env_absent() {
        assert!(!is_home_root(Path::new("/whatever"), None));
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

    #[test]
    fn explicit_marker_protects_its_directory_and_descendants_only() {
        let tmp = tempfile::tempdir().unwrap();
        let protected = tmp.path().join("crm");
        let sibling = tmp.path().join("cache");
        std::fs::create_dir_all(protected.join("exports")).unwrap();
        std::fs::create_dir_all(&sibling).unwrap();
        std::fs::write(protected.join(PROTECTED_PATH_MARKER), []).unwrap();

        assert!(is_protected(&protected));
        assert!(is_protected(&protected.join("exports/customer.db")));
        assert!(!is_protected(&sibling));
    }

    #[test]
    fn ontology_retention_binding_is_an_inherited_delete_veto() {
        let tmp = tempfile::tempdir().unwrap();
        let business = tmp.path().join("business-data");
        std::fs::create_dir_all(&business).unwrap();
        std::fs::write(
            business.join(ONTOLOGY_CLASS_MARKER),
            "https://disksage.app/ontology#CustomerRelationshipManagementData\n",
        )
        .unwrap();

        assert!(is_explicitly_protected(&business.join("customer.db")));
        assert!(is_protected(&business.join("customer.db")));
    }

    #[test]
    fn ontology_sidecar_protects_only_its_bound_file() {
        let tmp = tempfile::tempdir().unwrap();
        let export = tmp.path().join("crm-export.sql");
        let unrelated = tmp.path().join("cache.bin");
        std::fs::write(&export, b"crm").unwrap();
        std::fs::write(&unrelated, b"cache").unwrap();
        std::fs::write(
            sidecar(&export, ONTOLOGY_CLASS_MARKER).unwrap(),
            "https://disksage.app/ontology#CustomerRelationshipManagementData\n",
        )
        .unwrap();

        assert!(is_protected(&export));
        assert!(!is_protected(&unrelated));
    }

    #[cfg(unix)]
    #[test]
    fn current_user_owned_shared_temp_child_stays_globally_protected() {
        let Ok(tmp) = tempfile::tempdir_in(shared_temp_root_path()) else {
            return;
        };
        let child = tmp.path().join("owned.bin");
        std::fs::write(&child, b"owned").unwrap();
        assert!(is_shared_temp_path(&child));
        assert!(is_user_owned_shared_temp_tree(&child));
        assert!(is_protected(&child));
        assert!(is_protected(shared_temp_root_path()));
    }

    #[cfg(unix)]
    #[test]
    fn owned_symlink_child_does_not_block_exact_shared_temp_tree_authority() {
        use std::os::unix::fs::symlink;

        let Ok(tmp) = tempfile::tempdir_in(shared_temp_root_path()) else {
            return;
        };
        let target = tmp.path().join("target.bin");
        let link = tmp.path().join("runtime-link");
        std::fs::write(&target, b"owned").unwrap();
        symlink(&target, &link).unwrap();

        assert!(is_user_owned_shared_temp_tree(tmp.path()));
    }

    #[cfg(windows)]
    #[test]
    fn windows_guard_follows_system_root_env() {
        let sysroot = std::env::var("SystemRoot").unwrap();
        assert!(is_protected(std::path::Path::new(&sysroot)));
        assert!(is_protected(
            &std::path::Path::new(&sysroot).join("System32")
        ));
    }

    #[cfg(windows)]
    #[test]
    fn windows_guard_is_separator_agnostic_and_boundary_exact() {
        assert!(is_protected(Path::new("C:/Windows/System32")));
        assert!(is_protected(Path::new("c:/program files/SomeApp")));
        assert!(is_protected(Path::new("C:\\Program Files (x86)\\App")));
        assert!(!is_protected(Path::new("C:\\WindowsBackup")));
        assert!(!is_protected(Path::new("C:\\Windows.old")));
    }

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
        assert_eq!(recent[0].path, "/x/2");
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

    #[test]
    fn journal_io_err_wraps_as_journal_error() {
        let e = std::io::Error::new(std::io::ErrorKind::Other, "boom");
        assert!(matches!(journal_io_err(e), SafetyError::Journal(_)));
    }

    #[test]
    fn journal_serde_err_wraps_as_journal_error() {
        let e = serde_json::from_str::<i32>("not json").unwrap_err();
        assert!(matches!(journal_serde_err(e), SafetyError::Journal(_)));
    }

    #[test]
    fn trash_delete_rejects_protected_path_without_journaling() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let root = if cfg!(windows) { "C:\\Windows" } else { "/usr" };
        let err = trash_delete(Path::new(root), 0, &jp, 1);
        assert!(matches!(err, Err(SafetyError::Protected(_))));
        assert!(journal_recent(&jp, 10).is_empty());
    }

    #[test]
    fn filesystem_object_id_is_available_for_regular_fixture() {
        let tmp = tempfile::tempdir().unwrap();
        let victim = tmp.path().join("identity-fixture");
        std::fs::create_dir(&victim).unwrap();
        let first = filesystem_object_id(&victim).unwrap();
        let second = filesystem_object_id(&victim).unwrap();
        assert!(!first.is_empty());
        assert_eq!(first, second);
    }

    #[test]
    fn trash_delete_if_identity_rejects_a_replaced_object() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let victim = tmp.path().join("identity-target");
        let original = tmp.path().join("identity-original");
        let replacement = tmp.path().join("identity-replacement");
        std::fs::create_dir(&victim).unwrap();
        let expected = filesystem_object_id(&victim).unwrap();
        std::fs::rename(&victim, &original).unwrap();
        std::fs::create_dir(&replacement).unwrap();
        std::fs::rename(&replacement, &victim).unwrap();
        let err = trash_delete_if_identity(&victim, &expected, 0, &jp, 1);
        assert!(err.is_err());
        assert!(victim.exists());
        assert!(original.exists());
        assert!(journal_recent(&jp, 10).is_empty());
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn final_trash_source_substitution_does_not_mutate_replacement() {
        let tmp = tempfile::tempdir().unwrap();
        let victim = tmp.path().join("reviewed-cache");
        let reviewed = tmp.path().join("reviewed-cache-original");
        let replacement = tmp.path().join("reviewed-cache-replacement");
        std::fs::create_dir(&victim).unwrap();
        std::fs::write(victim.join("reviewed.bin"), b"reviewed").unwrap();
        std::fs::create_dir(&replacement).unwrap();
        std::fs::write(replacement.join("replacement.bin"), b"replacement").unwrap();
        let expected = filesystem_object_id(&victim).unwrap();
        let replacement_id = filesystem_object_id(&replacement).unwrap();
        let journal = tmp.path().join("journal.jsonl");
        let hook_victim = victim.clone();
        let hook_reviewed = reviewed.clone();
        let hook_replacement = replacement.clone();
        set_staging_created_hook(move || {
            std::fs::rename(&hook_victim, &hook_reviewed).unwrap();
            std::fs::rename(&hook_replacement, &hook_victim).unwrap();
        });

        let error = trash_delete_if_identity(&victim, &expected, 11, &journal, 1)
            .expect_err("a final source substitution must fail closed");

        assert!(error
            .to_string()
            .contains("atomic staging move changed the filesystem object"));
        assert_eq!(filesystem_object_id(&victim).unwrap(), replacement_id);
        assert_eq!(
            std::fs::read(victim.join("replacement.bin")).unwrap(),
            b"replacement"
        );
        assert_eq!(filesystem_object_id(&reviewed).unwrap(), expected);
        assert_eq!(
            std::fs::read(reviewed.join("reviewed.bin")).unwrap(),
            b"reviewed"
        );
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn final_permanent_source_substitution_does_not_mutate_replacement() {
        let tmp = tempfile::tempdir().unwrap();
        let victim = tmp.path().join("reviewed-cache");
        let reviewed = tmp.path().join("reviewed-cache-original");
        let replacement = tmp.path().join("reviewed-cache-replacement");
        std::fs::create_dir(&victim).unwrap();
        std::fs::write(victim.join("reviewed.bin"), b"reviewed").unwrap();
        std::fs::create_dir(&replacement).unwrap();
        std::fs::write(replacement.join("replacement.bin"), b"replacement").unwrap();
        let expected = filesystem_object_id(&victim).unwrap();
        let replacement_id = filesystem_object_id(&replacement).unwrap();
        let journal = tmp.path().join("journal.jsonl");
        let hook_victim = victim.clone();
        let hook_reviewed = reviewed.clone();
        let hook_replacement = replacement.clone();
        set_staging_created_hook(move || {
            std::fs::rename(&hook_victim, &hook_reviewed).unwrap();
            std::fs::rename(&hook_replacement, &hook_victim).unwrap();
        });

        let error = permanent_delete_dir_if_identity(&victim, &expected, 11, &journal, 1)
            .expect_err("a final source substitution must fail closed");

        assert!(error
            .to_string()
            .contains("atomic staging move changed the generated directory"));
        assert_eq!(filesystem_object_id(&victim).unwrap(), replacement_id);
        assert_eq!(
            std::fs::read(victim.join("replacement.bin")).unwrap(),
            b"replacement"
        );
        assert_eq!(filesystem_object_id(&reviewed).unwrap(), expected);
        assert_eq!(
            std::fs::read(reviewed.join("reviewed.bin")).unwrap(),
            b"reviewed"
        );
    }

    #[test]
    fn catalog_root_authority_never_overrides_an_explicit_protection_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("catalog");
        let victim = root.join("regenerable-cache");
        std::fs::create_dir_all(&victim).unwrap();
        std::fs::write(root.join(PROTECTED_PATH_MARKER), []).unwrap();
        let expected = filesystem_object_id(&victim).unwrap();
        let journal = tmp.path().join("journal.jsonl");

        let error = trash_delete_if_identity_in_catalog_root(
            &victim, &root, &expected, 0, &journal, 1,
        );

        assert!(matches!(error, Err(SafetyError::Protected(_))));
        assert!(victim.exists());
        assert!(journal_recent(&journal, 10).is_empty());
    }

    #[test]
    fn catalog_root_authority_rejects_a_non_parent_root_without_journaling() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("catalog");
        let wrong_root = tmp.path().join("other-catalog");
        let victim = root.join("regenerable-cache");
        std::fs::create_dir_all(&victim).unwrap();
        std::fs::create_dir(&wrong_root).unwrap();
        let expected = filesystem_object_id(&victim).unwrap();
        let journal = tmp.path().join("journal.jsonl");

        let error = trash_delete_if_identity_in_catalog_root(
            &victim,
            &wrong_root,
            &expected,
            0,
            &journal,
            1,
        );

        assert!(matches!(error, Err(SafetyError::Protected(_))));
        assert!(victim.exists());
        assert!(journal_recent(&journal, 10).is_empty());
    }

    #[test]
    fn catalog_root_revalidation_rejects_root_object_replacement_with_same_target_object() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("catalog");
        let reviewed_root = tmp.path().join("catalog-reviewed");
        let victim = root.join("regenerable-cache");
        std::fs::create_dir_all(&victim).unwrap();
        let expected_root_id = filesystem_object_id(&root).unwrap();
        let expected_target_id = filesystem_object_id(&victim).unwrap();

        std::fs::rename(&root, &reviewed_root).unwrap();
        std::fs::create_dir(&root).unwrap();
        std::fs::rename(reviewed_root.join("regenerable-cache"), &victim).unwrap();
        assert_eq!(filesystem_object_id(&victim).unwrap(), expected_target_id);

        let error = revalidate_catalog_root_before_staging(
            &victim,
            &root,
            &expected_root_id,
            &expected_target_id,
        );

        assert!(matches!(error, Err(SafetyError::Protected(_))));
        assert!(victim.exists());
    }

    #[test]
    fn catalog_root_authorization_rejects_root_replacement_inside_authorization_window() {
        let tmp = tempfile::tempdir().unwrap();
        let hook_root = tmp.path().join("catalog");
        let hook_reviewed_root = tmp.path().join("catalog-reviewed");
        let hook_victim = hook_root.join("regenerable-cache");
        let hook_parked_target = tmp.path().join("parked-target");
        std::fs::create_dir_all(&hook_victim).unwrap();
        let victim = hook_victim.clone();
        let root = hook_root.clone();
        let expected_target_id = filesystem_object_id(&victim).unwrap();
        let journal = tmp.path().join("journal.jsonl");

        set_catalog_root_authorization_hook(move || {
            std::fs::rename(&hook_victim, &hook_parked_target).unwrap();
            std::fs::rename(&hook_root, &hook_reviewed_root).unwrap();
            std::fs::create_dir(&hook_root).unwrap();
            std::fs::rename(&hook_parked_target, &hook_victim).unwrap();
        });

        let error = trash_delete_if_identity_in_catalog_root(
            &victim,
            &root,
            &expected_target_id,
            0,
            &journal,
            1,
        );

        assert!(matches!(error, Err(SafetyError::Protected(_))));
        assert_eq!(filesystem_object_id(&victim).unwrap(), expected_target_id);
        assert!(victim.exists());
        assert!(journal_recent(&journal, 10).is_empty());
        assert!(std::fs::read_dir(tmp.path()).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".disksage-trash-")));
    }

    #[cfg(unix)]
    #[test]
    #[cfg(not(target_os = "linux"))]
    fn staging_creation_parent_replacement_fails_without_untracked_residue() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path().join("owner-parent");
        let moved_parent = tmp.path().join("owner-parent-reviewed");
        let generated = parent.join("node_modules");
        std::fs::create_dir_all(&generated).unwrap();
        std::fs::write(generated.join("generated.bin"), b"generated").unwrap();
        let object_id = filesystem_object_id(&generated).unwrap();
        let journal = tmp.path().join("journal.jsonl");
        let hook_parent = parent.clone();
        let hook_moved_parent = moved_parent.clone();
        set_staging_created_hook(move || {
            std::fs::rename(&hook_parent, &hook_moved_parent).unwrap();
            std::fs::create_dir(&hook_parent).unwrap();
        });

        let error = permanent_delete_dir_if_identity(&generated, &object_id, 9, &journal, 1)
            .expect_err("a replaced staging parent must fail closed");

        assert!(error
            .to_string()
            .contains("parent identity changed during creation"));
        let moved_generated = moved_parent.join("node_modules");
        assert_eq!(filesystem_object_id(&moved_generated).unwrap(), object_id);
        assert!(moved_generated.join("generated.bin").is_file());
        assert!(journal_recent(&journal, 10).is_empty());
        let authority_journal = staging_authority_journal_path(&journal).unwrap();
        let entries = journal_recent(&authority_journal, 10);
        assert!(entries.iter().any(|entry| {
            parse_staging_creation_intent(&entry.outcome).is_some_and(|(_, complete)| complete)
        }));
        for inspected_parent in [&parent, &moved_parent] {
            assert!(std::fs::read_dir(inspected_parent)
                .unwrap()
                .all(|entry| !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".disksage-trash-")));
        }
    }

    #[cfg(unix)]
    #[test]
    #[cfg(not(target_os = "linux"))]
    fn staging_creation_child_failure_closes_durable_intent() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path().join("owner-parent");
        let detached_parent = tmp.path().join("owner-parent-detached");
        let parked_target = tmp.path().join("parked-node-modules");
        let generated = parent.join("node_modules");
        std::fs::create_dir_all(&generated).unwrap();
        std::fs::write(generated.join("generated.bin"), b"generated").unwrap();
        let object_id = filesystem_object_id(&generated).unwrap();
        let expected_parent_id = filesystem_object_id(&parent).unwrap();
        let journal = tmp.path().join("journal.jsonl");
        let hook_parent = parent.clone();
        let hook_detached_parent = detached_parent.clone();
        let hook_parked_target = parked_target.clone();
        set_staging_creation_intent_published_hook(move || {
            std::fs::rename(&hook_parent, &hook_detached_parent).unwrap();
            std::fs::rename(
                hook_detached_parent.join("node_modules"),
                &hook_parked_target,
            )
            .unwrap();
            std::fs::remove_dir(&hook_detached_parent).unwrap();
        });

        let error = permanent_delete_dir_if_identity(&generated, &object_id, 9, &journal, 1)
            .expect_err("mkdirat through an unlinked parent must fail");

        assert!(error.to_string().contains("No such file or directory"));
        assert!(!parent.exists());
        assert!(!detached_parent.exists());
        assert!(parked_target.join("generated.bin").is_file());
        let authority_journal = staging_authority_journal_path(&journal).unwrap();
        let entries = journal_recent(&authority_journal, 10);
        assert_eq!(entries.len(), 2);
        let (completed_intent, complete) = parse_staging_creation_intent(&entries[0].outcome)
            .expect("the failed child creation must have a terminal intent");
        let (pending_intent, pending_complete) = parse_staging_creation_intent(&entries[1].outcome)
            .expect("the child attempt must be preceded by a durable intent");
        assert!(complete);
        assert!(!pending_complete);
        assert_eq!(
            staging_creation_intent_identity(&completed_intent),
            staging_creation_intent_identity(&pending_intent)
        );
        assert_eq!(pending_intent.source_parent_object_id, expected_parent_id);
        assert_eq!(pending_intent.target_object_id, object_id);
        assert_eq!(pending_intent.catalog_root_object_id, None);

        std::fs::create_dir(&parent).unwrap();
        std::fs::rename(&parked_target, &generated).unwrap();
        assert_eq!(filesystem_object_id(&generated).unwrap(), object_id);
        permanent_delete_dir_if_identity(&generated, &object_id, 9, &journal, 2).unwrap();
        assert!(!generated.exists());
        assert!(std::fs::read_dir(&parent).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".disksage-trash-")));
        let mut successful_entries: Vec<_> = journal_recent(&authority_journal, 20)
            .into_iter()
            .filter(|entry| entry.ts_ms == 2)
            .collect();
        successful_entries.reverse();
        let object_authority_index = successful_entries
            .iter()
            .position(|entry| parse_staging_cleanup_pending(&entry.outcome).is_some())
            .expect("successful handoff requires durable object-bound authority");
        let intent_completion_index = successful_entries
            .iter()
            .position(|entry| {
                parse_staging_creation_intent(&entry.outcome).is_some_and(|(_, complete)| complete)
            })
            .expect("successful handoff must close its pre-create intent");
        assert!(object_authority_index < intent_completion_index);

        permanent_delete_dir_if_identity(&generated, &object_id, 9, &journal, 3)
            .expect("an absent-source retry must be idempotent");
    }

    #[cfg(unix)]
    #[test]
    #[cfg(not(target_os = "linux"))]
    fn staging_creation_intent_completion_failure_fails_closed() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path().join("owner-parent");
        let generated = parent.join("node_modules");
        std::fs::create_dir_all(&generated).unwrap();
        std::fs::write(generated.join("generated.bin"), b"generated").unwrap();
        let object_id = filesystem_object_id(&generated).unwrap();
        let journal = tmp.path().join("journal.jsonl");
        let authority_journal = staging_authority_journal_path(&journal).unwrap();
        set_staging_creation_intent_completion_failure(true);

        let error = permanent_delete_dir_if_identity(&generated, &object_id, 9, &journal, 1)
            .expect_err("intent completion publication failure must fail closed");

        set_staging_creation_intent_completion_failure(false);
        let message = error.to_string();
        assert_eq!(
            message
                .matches("staging creation intent completion failed")
                .count(),
            1
        );
        assert!(generated.join("generated.bin").is_file());
        assert!(std::fs::read_dir(&parent).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".disksage-trash-")));
        let entries = journal_recent(&authority_journal, 10);
        assert!(entries
            .iter()
            .any(|entry| parse_staging_cleanup_pending(&entry.outcome).is_some()));
        assert!(entries
            .iter()
            .any(|entry| parse_staging_cleanup_complete(&entry.outcome).is_some()));
        assert!(entries.iter().any(|entry| {
            parse_staging_creation_intent(&entry.outcome).is_some_and(|(_, complete)| !complete)
        }));
        assert!(!entries.iter().any(|entry| {
            parse_staging_creation_intent(&entry.outcome).is_some_and(|(_, complete)| complete)
        }));
    }

    #[cfg(windows)]
    #[test]
    fn staging_creation_windows_substitution_does_not_delete_replacement() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path().join("owner-parent");
        let generated = parent.join("node_modules");
        std::fs::create_dir_all(&generated).unwrap();
        std::fs::write(generated.join("generated.bin"), b"generated").unwrap();
        let object_id = filesystem_object_id(&generated).unwrap();
        let journal = tmp.path().join("journal.jsonl");
        let authority_journal = staging_authority_journal_path(&journal).unwrap();
        let displaced = parent.join("displaced-staging");
        let substitution = std::sync::Arc::new(std::sync::Mutex::new(None));
        let hook_parent = parent.clone();
        let hook_displaced = displaced.clone();
        let hook_substitution = std::sync::Arc::clone(&substitution);
        set_staging_creation_intent_completion_failure(true);
        set_staging_created_hook(move || {
            let staging_name = std::fs::read_dir(&hook_parent)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .find(|name| name.to_string_lossy().starts_with(".disksage-trash-"))
                .expect("the staging directory must exist before the substitution hook");
            let staging_path = hook_parent.join(staging_name);
            let original_id = filesystem_object_id(&staging_path).unwrap();
            std::fs::rename(&staging_path, &hook_displaced).unwrap();
            std::fs::create_dir(&staging_path).unwrap();
            let replacement_id = filesystem_object_id(&staging_path).unwrap();
            assert_ne!(original_id, replacement_id);
            *hook_substitution.lock().unwrap() = Some((staging_path, original_id, replacement_id));
        });

        let error = permanent_delete_dir_if_identity(&generated, &object_id, 9, &journal, 1)
            .expect_err("intent completion failure must trigger staging rollback");

        set_staging_creation_intent_completion_failure(false);
        assert!(error
            .to_string()
            .contains("staging creation intent completion failed"));
        let (staging_path, original_id, replacement_id) =
            substitution.lock().unwrap().take().unwrap();
        assert_eq!(filesystem_object_id(&staging_path).unwrap(), replacement_id);
        assert!(
            staging_path.is_dir(),
            "rollback must retain the replacement"
        );
        assert!(
            !displaced.exists(),
            "rollback must delete the original through its retained handle"
        );
        assert!(generated.join("generated.bin").is_file());
        let entries = journal_recent(&authority_journal, 10);
        assert!(entries.iter().any(|entry| {
            parse_staging_cleanup_pending(&entry.outcome)
                .is_some_and(|recovery| recovery.staging_object_id == original_id)
        }));
        assert!(!entries.iter().any(|entry| {
            parse_staging_cleanup_pending(&entry.outcome)
                .is_some_and(|recovery| recovery.staging_object_id == replacement_id)
        }));
    }

    #[cfg(unix)]
    #[test]
    #[cfg(not(target_os = "linux"))]
    fn staging_creation_rollback_failure_is_durably_recoverable() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path().join("owner-parent");
        let moved_parent = tmp.path().join("owner-parent-reviewed");
        let generated = parent.join("node_modules");
        std::fs::create_dir_all(&generated).unwrap();
        std::fs::write(generated.join("generated.bin"), b"generated").unwrap();
        let object_id = filesystem_object_id(&generated).unwrap();
        let expected_parent_id = filesystem_object_id(&parent).unwrap();
        let journal = tmp.path().join("journal.jsonl");
        let hook_parent = parent.clone();
        let hook_moved_parent = moved_parent.clone();
        set_staging_created_hook(move || {
            let staging_name = std::fs::read_dir(&hook_parent)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .find(|name| name.to_string_lossy().starts_with(".disksage-trash-"))
                .expect("the staging directory must exist before the race hook");
            std::fs::rename(&hook_parent, &hook_moved_parent).unwrap();
            std::fs::create_dir(&hook_parent).unwrap();
            std::fs::write(
                hook_moved_parent
                    .join(staging_name)
                    .join("rollback-blocker"),
                b"retain",
            )
            .unwrap();
        });

        let error = permanent_delete_dir_if_identity(&generated, &object_id, 9, &journal, 1)
            .expect_err("a failed staging rollback must fail closed");

        assert!(error
            .to_string()
            .contains("staging creation cleanup remains pending"));
        let authority_journal = staging_authority_journal_path(&journal).unwrap();
        let entries = journal_recent(&authority_journal, 10);
        assert!(!entries.iter().any(|entry| {
            parse_staging_creation_intent(&entry.outcome).is_some_and(|(_, complete)| complete)
        }));
        let recovery = entries
            .iter()
            .find_map(|entry| parse_staging_cleanup_pending(&entry.outcome))
            .expect("rollback failure must publish durable recovery evidence");
        let retained_staging = moved_parent.join(&recovery.staging_name);
        assert_eq!(
            recovery.source_parent_object_id.as_deref(),
            Some(expected_parent_id.as_str())
        );
        assert_eq!(
            filesystem_object_id(&retained_staging).unwrap(),
            recovery.staging_object_id
        );
        assert!(retained_staging.join("rollback-blocker").is_file());
        assert_eq!(
            filesystem_object_id(&moved_parent.join("node_modules")).unwrap(),
            object_id
        );
        assert!(std::fs::read_dir(&parent).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".disksage-trash-")));
    }

    #[cfg(unix)]
    #[test]
    #[cfg(not(target_os = "linux"))]
    fn staging_creation_rollback_and_journal_failure_does_not_leave_untracked_residue() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path().join("owner-parent");
        let moved_parent = tmp.path().join("owner-parent-reviewed");
        let generated = parent.join("node_modules");
        std::fs::create_dir_all(&generated).unwrap();
        std::fs::write(generated.join("generated.bin"), b"generated").unwrap();
        let object_id = filesystem_object_id(&generated).unwrap();
        let expected_parent_id = filesystem_object_id(&parent).unwrap();
        let journal = tmp.path().join("journal.jsonl");
        std::fs::create_dir(&journal).unwrap();
        let authority_journal = staging_authority_journal_path(&journal).unwrap();
        let hook_parent = parent.clone();
        let hook_moved_parent = moved_parent.clone();
        let hook_authority_journal = authority_journal.clone();
        set_staging_created_hook(move || {
            use std::os::unix::fs::PermissionsExt;

            let staging_name = std::fs::read_dir(&hook_parent)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .find(|name| name.to_string_lossy().starts_with(".disksage-trash-"))
                .expect("the staging directory must exist before the race hook");
            assert!(
                hook_authority_journal.is_file(),
                "creation intent must be durable before the staging child exists"
            );
            std::fs::set_permissions(
                &hook_authority_journal,
                std::fs::Permissions::from_mode(0o400),
            )
            .unwrap();
            std::fs::rename(&hook_parent, &hook_moved_parent).unwrap();
            std::fs::create_dir(&hook_parent).unwrap();
            std::fs::write(
                hook_moved_parent
                    .join(staging_name)
                    .join("rollback-blocker"),
                b"retain",
            )
            .unwrap();
        });

        let error = permanent_delete_dir_if_identity(&generated, &object_id, 9, &journal, 1)
            .expect_err("rollback and primary journal failures must fail closed");

        assert!(error
            .to_string()
            .contains("staging creation cleanup remains pending"));
        assert!(journal_recent(&journal, 10).is_empty());
        let entries = journal_recent(&authority_journal, 10);
        assert_eq!(entries.len(), 1);
        let (intent, complete) = parse_staging_creation_intent(&entries[0].outcome)
            .expect("the pre-create journal must retain exact recovery authority");
        assert!(!complete);
        let retained_staging = moved_parent.join(&intent.staging_name);
        assert_eq!(intent.source_parent_object_id, expected_parent_id);
        assert_eq!(intent.target_object_id, object_id);
        assert_eq!(intent.catalog_root_object_id, None);
        assert!(retained_staging.join("rollback-blocker").is_file());
        assert_eq!(
            filesystem_object_id(&moved_parent.join("node_modules")).unwrap(),
            object_id
        );
    }

    #[cfg(unix)]
    #[test]
    #[rustfmt::skip]
    #[cfg(not(target_os = "linux"))]
    fn staging_creation_rollback_and_all_recovery_publications_fail_does_not_leave_untracked_residue() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path().join("owner-parent");
        let moved_parent = tmp.path().join("owner-parent-reviewed");
        let generated = parent.join("node_modules");
        std::fs::create_dir_all(&generated).unwrap();
        std::fs::write(generated.join("generated.bin"), b"generated").unwrap();
        let object_id = filesystem_object_id(&generated).unwrap();
        let expected_parent_id = filesystem_object_id(&parent).unwrap();
        let journal = tmp.path().join("journal.jsonl");
        let recovery_journal = staging_recovery_journal_path(&journal).unwrap();
        let authority_journal = staging_authority_journal_path(&journal).unwrap();
        let hook_parent = parent.clone();
        let hook_moved_parent = moved_parent.clone();
        let hook_journal = journal.clone();
        let hook_recovery_journal = recovery_journal.clone();
        let hook_authority_journal = authority_journal.clone();
        set_staging_created_hook(move || {
            use std::os::unix::fs::PermissionsExt;

            let staging_name = std::fs::read_dir(&hook_parent)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .find(|name| name.to_string_lossy().starts_with(".disksage-trash-"))
                .expect("the staging directory must exist before the race hook");
            assert!(
                hook_authority_journal.is_file(),
                "exact recovery authority must be durable before the rollback race"
            );
            std::fs::set_permissions(
                &hook_authority_journal,
                std::fs::Permissions::from_mode(0o400),
            )
            .unwrap();
            std::fs::create_dir(&hook_journal).unwrap();
            std::fs::create_dir(&hook_recovery_journal).unwrap();
            std::fs::rename(&hook_parent, &hook_moved_parent).unwrap();
            std::fs::create_dir(&hook_parent).unwrap();
            std::fs::write(
                hook_moved_parent
                    .join(staging_name)
                    .join("rollback-blocker"),
                b"retain",
            )
            .unwrap();
        });

        let error = permanent_delete_dir_if_identity(&generated, &object_id, 9, &journal, 1)
            .expect_err("rollback and every later recovery publication must fail closed");

        assert!(error
            .to_string()
            .contains("staging creation cleanup remains pending"));
        let probe = JournalEntry {
            ts_ms: 2,
            op: "permanent_generated_directory_delete".into(),
            path: generated.to_string_lossy().into_owned(),
            bytes: 9,
            outcome: "probe".into(),
        };
        assert!(
            journal_append_staging_recovery(&journal, &probe).is_err(),
            "the fixture must exhaust both durable publication paths after rollback"
        );
        let entries = journal_recent(&authority_journal, 10);
        assert_eq!(entries.len(), 1);
        let (intent, complete) = parse_staging_creation_intent(&entries[0].outcome)
            .expect("pre-create authority must retain the exact planned staging child");
        assert!(!complete);
        let retained_staging = moved_parent.join(&intent.staging_name);
        assert_eq!(
            intent.source_parent_object_id,
            expected_parent_id
        );
        assert_eq!(intent.target_object_id, object_id);
        assert_eq!(intent.catalog_root_object_id, None);
        assert!(retained_staging.join("rollback-blocker").is_file());
        assert_eq!(
            filesystem_object_id(&moved_parent.join("node_modules")).unwrap(),
            object_id
        );
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn permanent_generated_directory_delete_rechecks_identity_and_journals() {
        let tmp = tempfile::tempdir().unwrap();
        let generated = tmp.path().join("node_modules");
        std::fs::create_dir(&generated).unwrap();
        std::fs::write(generated.join("generated.bin"), b"generated").unwrap();
        let object_id = filesystem_object_id(&generated).unwrap();
        let journal = tmp.path().join("journal.jsonl");

        permanent_delete_dir_if_identity(&generated, &object_id, 9, &journal, 1).unwrap();

        assert!(!generated.exists());
        let entries = journal_recent(&journal, 2);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].op, "permanent_generated_directory_delete");
        assert_eq!(entries[0].outcome, "ok");
        assert_eq!(entries[1].outcome, "pending");
        assert!(std::fs::read_dir(tmp.path()).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".disksage-trash-")));
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn post_mutation_cleanup_failure_is_persisted_and_retry_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let generated = tmp.path().join("node_modules");
        std::fs::create_dir(&generated).unwrap();
        std::fs::write(generated.join("generated.bin"), b"generated").unwrap();
        let object_id = filesystem_object_id(&generated).unwrap();
        let journal = tmp.path().join("journal.jsonl");
        set_staging_cleanup_hook(|_| {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "simulated cleanup failure",
            ))
        });

        let first =
            permanent_delete_dir_if_identity(&generated, &object_id, 9, &journal, 1).unwrap_err();

        assert!(first.to_string().contains("mutation completed"));
        assert!(!generated.exists());
        let pending = &journal_recent(&journal, 1)[0].outcome;
        let recovery = parse_staging_cleanup_pending(pending)
            .expect("post-mutation cleanup failure must be recoverable");
        let staging_dir = tmp.path().join(&recovery.staging_name);
        assert!(staging_dir.is_dir());
        assert!(std::fs::read_dir(&staging_dir).unwrap().next().is_none());

        permanent_delete_dir_if_identity(&generated, &object_id, 9, &journal, 2).unwrap();
        assert!(!staging_dir.exists());
        assert!(journal_recent(&journal, 1)[0]
            .outcome
            .starts_with(STAGING_CLEANUP_COMPLETE_PREFIX));

        permanent_delete_dir_if_identity(&generated, &object_id, 9, &journal, 3).unwrap();
        assert!(!staging_dir.exists());
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn pending_cleanup_rejects_moved_and_replaced_source_parent() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path().join("owner-parent");
        let moved_parent = tmp.path().join("owner-parent-reviewed");
        let generated = parent.join("node_modules");
        std::fs::create_dir_all(&generated).unwrap();
        std::fs::write(generated.join("generated.bin"), b"generated").unwrap();
        let object_id = filesystem_object_id(&generated).unwrap();
        let journal = tmp.path().join("journal.jsonl");
        set_staging_cleanup_hook(|_| {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "simulated cleanup failure",
            ))
        });

        permanent_delete_dir_if_identity(&generated, &object_id, 9, &journal, 1).unwrap_err();
        let recovery = parse_staging_cleanup_pending(&journal_recent(&journal, 1)[0].outcome)
            .expect("cleanup failure must publish recovery evidence");
        let retained_staging = parent.join(&recovery.staging_name);
        assert!(retained_staging.is_dir());

        std::fs::rename(&parent, &moved_parent).unwrap();
        std::fs::create_dir(&parent).unwrap();

        let retry = permanent_delete_dir_if_identity(&generated, &object_id, 9, &journal, 2)
            .expect_err("a replacement source parent must not complete cleanup");

        assert!(retry.to_string().contains("cleanup remains pending"));
        assert!(moved_parent.join(&recovery.staging_name).is_dir());
        assert!(!parent.join(&recovery.staging_name).exists());
        assert!(journal_recent(&journal, 1)[0]
            .outcome
            .starts_with(STAGING_CLEANUP_PENDING_PREFIX));
    }

    #[test]
    fn staging_receipt_retains_creation_parent_identity_after_move_and_replacement() {
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path().join("owner-parent");
        let moved_parent = tmp.path().join("owner-parent-reviewed");
        let source = parent.join("node_modules");
        std::fs::create_dir(&parent).unwrap();
        let expected_parent_id = filesystem_object_id(&parent).unwrap();
        let staging_dir = create_private_staging_dir(&source, 1).unwrap();

        std::fs::rename(&parent, &moved_parent).unwrap();
        std::fs::create_dir(&parent).unwrap();
        let replacement_parent_id = filesystem_object_id(&parent).unwrap();

        let recovery = staging_cleanup_recovery(&staging_dir, "target-id", None).unwrap();

        assert_eq!(
            recovery.source_parent_object_id.as_deref(),
            Some(expected_parent_id.as_str())
        );
        assert_ne!(
            recovery.source_parent_object_id.as_deref(),
            Some(replacement_parent_id.as_str())
        );
        assert_eq!(
            recovery.staging_object_id,
            filesystem_object_id(&moved_parent.join(&recovery.staging_name)).unwrap()
        );
        assert!(!parent.join(&recovery.staging_name).exists());
    }

    #[test]
    fn legacy_parent_unbound_recovery_fails_closed() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let staging_dir = create_private_staging_dir(&source, 1).unwrap();
        let recovery = staging_cleanup_recovery(&staging_dir, "target-id", None).unwrap();
        let legacy_outcome = format!(
            "{STAGING_CLEANUP_PENDING_PREFIX}{{\"staging_name\":\"{}\",\"staging_object_id\":\"{}\",\"target_object_id\":\"target-id\",\"catalog_root_object_id\":null,\"error\":\"simulated cleanup failure\"}}",
            recovery.staging_name, recovery.staging_object_id
        );
        let journal = tmp.path().join("journal.jsonl");
        journal_append(
            &journal,
            &JournalEntry {
                ts_ms: 1,
                op: "permanent_generated_directory_delete".into(),
                path: source.to_string_lossy().into_owned(),
                bytes: 0,
                outcome: legacy_outcome,
            },
        )
        .unwrap();

        let retry = retry_pending_staging_cleanup(
            &source,
            "target-id",
            None,
            "permanent_generated_directory_delete",
            0,
            &journal,
            2,
        )
        .expect("legacy recovery evidence must be recognized")
        .expect_err("legacy recovery evidence must fail closed");

        assert!(retry.to_string().contains("legacy staging recovery"));
        assert!(staging_dir.path.exists());
    }

    #[test]
    fn staging_cleanup_rejects_nonempty_or_replaced_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let staging_dir = create_private_staging_dir(&source, 1).unwrap();
        let recovery = staging_cleanup_recovery(&staging_dir, "target-id", None).unwrap();
        std::fs::write(staging_dir.path.join("unexpected"), b"retain").unwrap();

        let nonempty = cleanup_verified_empty_staging_dir(&source, &recovery).unwrap_err();

        assert!(nonempty.contains("not empty"));
        assert!(staging_dir.path.join("unexpected").exists());
        std::fs::remove_file(staging_dir.path.join("unexpected")).unwrap();
        let displaced_staging = tmp.path().join("displaced-staging");
        std::fs::rename(&staging_dir.path, &displaced_staging).unwrap();
        std::fs::create_dir(&staging_dir.path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&staging_dir.path, std::fs::Permissions::from_mode(0o700))
                .unwrap();
        }
        let replacement_id = filesystem_object_id(&staging_dir.path).unwrap();
        assert_ne!(replacement_id, recovery.staging_object_id);

        let replaced = cleanup_verified_empty_staging_dir(&source, &recovery).unwrap_err();

        assert!(replaced.contains("identity changed"));
        assert_eq!(
            filesystem_object_id(&staging_dir.path).unwrap(),
            replacement_id
        );
        assert_eq!(
            filesystem_object_id(&displaced_staging).unwrap(),
            recovery.staging_object_id
        );
    }

    #[test]
    fn pending_catalog_cleanup_rejects_a_changed_root_identity() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let staging_dir = create_private_staging_dir(&source, 1).unwrap();
        let mut recovery =
            staging_cleanup_recovery(&staging_dir, "target-id", Some("reviewed-root-id")).unwrap();
        recovery.error = "simulated cleanup failure".into();
        let journal = tmp.path().join("journal.jsonl");
        journal_append(
            &journal,
            &JournalEntry {
                ts_ms: 1,
                op: "trash_delete".into(),
                path: source.to_string_lossy().into_owned(),
                bytes: 0,
                outcome: staging_cleanup_pending_outcome(&recovery),
            },
        )
        .unwrap();

        let retry = retry_pending_staging_cleanup(
            &source,
            "target-id",
            Some("replacement-root-id"),
            "trash_delete",
            0,
            &journal,
            2,
        )
        .expect("pending recovery must be recognized");

        assert!(matches!(retry, Err(SafetyError::Protected(_))));
        assert!(staging_dir.path.exists());
    }

    #[test]
    fn permanent_delete_does_not_restore_a_partially_removed_staged_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("node_modules");
        let staging_dir = tmp.path().join(".disksage-trash");
        let staged = staging_dir.join("node_modules");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("removed.bin"), b"removed").unwrap();
        std::fs::write(source.join("retained.bin"), b"retained").unwrap();
        std::fs::create_dir(&staging_dir).unwrap();
        std::fs::rename(&source, &staged).unwrap();

        let result = remove_staged_permanently_with(&staged, |path| {
            std::fs::remove_file(path.join("removed.bin")).unwrap();
            Err(std::io::Error::other("simulated recursive delete failure"))
        });

        assert!(result.is_err());
        assert!(
            !source.exists(),
            "a partial tree must not be restored as live"
        );
        assert!(!staged.join("removed.bin").exists());
        assert!(staged.join("retained.bin").exists());
    }

    #[test]
    fn staged_restore_reports_reappeared_source_and_retains_staged_object() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let staging_dir = create_private_staging_dir(&source, 1).unwrap();
        let staged = staging_dir.path.join("source");
        let recovery = staging_cleanup_recovery(&staging_dir, "target-id", None).unwrap();
        std::fs::write(&source, b"replacement").unwrap();
        std::fs::write(&staged, b"reviewed").unwrap();
        let error = restore_staged_if_source_absent(&source, &staged, &recovery).unwrap_err();
        assert!(error.contains(staged.to_string_lossy().as_ref()));
        assert!(source.exists());
        assert!(staged.exists());
    }

    #[test]
    fn staged_restore_reports_rename_failure_with_staged_path() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let staging_dir = create_private_staging_dir(&source, 1).unwrap();
        let staged = staging_dir.path.join("source");
        let recovery = staging_cleanup_recovery(&staging_dir, "target-id", None).unwrap();
        let error = restore_staged_if_source_absent(&source, &staged, &recovery).unwrap_err();
        assert!(error.contains(staged.to_string_lossy().as_ref()));
        assert!(staging_dir.path.exists());
    }

    #[test]
    fn trash_delete_missing_path_journals_error_outcome() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let missing = tmp.path().join("ghost.bin");
        let err = trash_delete(&missing, 0, &jp, 1);
        assert!(matches!(err, Err(SafetyError::Trash(_))));
        let recent = journal_recent(&jp, 10);
        assert_eq!(recent.len(), 2);
        assert!(recent[0].outcome.starts_with("error:"));
        assert_eq!(recent[1].outcome, "pending");
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn trash_delete_roundtrip_lands_in_trash() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let victim = tmp.path().join("disksage-roundtrip-fixture.bin");
        std::fs::write(&victim, vec![0u8; 64]).unwrap();
        trash_delete(&victim, 64, &jp, 42).unwrap();
        assert!(!victim.exists());
        let recent = journal_recent(&jp, 10);
        assert_eq!(recent[0].outcome, "ok");
        assert_eq!(recent[0].ts_ms, 42);
        let items: Vec<_> = trash::os_limited::list()
            .unwrap()
            .into_iter()
            .filter(|i| {
                i.name
                    .to_string_lossy()
                    .contains("disksage-roundtrip-fixture")
            })
            .collect();
        assert!(!items.is_empty());
        trash::os_limited::purge_all(items).unwrap();
    }

    #[test]
    fn trash_delete_rejects_parent_dir_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let sneaky = tmp.path().join("..");
        let err = trash_delete(&sneaky, 0, &jp, 1);
        assert!(matches!(err, Err(SafetyError::Protected(_))));
        assert!(journal_recent(&jp, 10).is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn trash_delete_rejects_verbatim_protected_path() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let err = trash_delete(Path::new(r"\\?\C:\Windows\System32"), 0, &jp, 1);
        assert!(matches!(err, Err(SafetyError::Protected(_))));
        assert!(journal_recent(&jp, 10).is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn strip_verbatim_reconstructs_disk_and_unc_forms() {
        assert_eq!(
            strip_verbatim(Path::new(r"\\?\C:\Windows\System32")),
            Path::new(r"C:\Windows\System32")
        );
        assert_eq!(
            strip_verbatim(Path::new(r"\\?\UNC\srv\share\dir")),
            Path::new(r"\\srv\share\dir")
        );
        assert_eq!(
            strip_verbatim(Path::new(r"C:\plain")),
            Path::new(r"C:\plain")
        );
        assert_eq!(
            strip_verbatim(Path::new("relative/only")),
            Path::new("relative/only")
        );
        assert!(is_protected(&strip_verbatim(Path::new(
            r"\\?\UNC\srv\share"
        ))));
    }

    #[test]
    fn journal_append_heals_torn_tail() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("journal.jsonl");
        std::fs::write(&jp, "{\"torn\":").unwrap();
        journal_append(
            &jp,
            &JournalEntry {
                ts_ms: 1,
                op: "trash_delete".into(),
                path: "/x".into(),
                bytes: 0,
                outcome: "ok".into(),
            },
        )
        .unwrap();
        let recent = journal_recent(&jp, 10);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].path, "/x");
    }

    #[test]
    fn move_file_rejects_protected_src_or_dst() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let f = tmp.path().join("f.bin");
        std::fs::write(&f, b"x").unwrap();
        let protected = std::path::PathBuf::from(if cfg!(windows) {
            "C:\\Windows\\x"
        } else {
            "/usr/x"
        });
        assert!(matches!(
            move_file(&f, &protected, &jp, 1),
            Err(SafetyError::Protected(_))
        ));
        let pf = std::path::PathBuf::from(if cfg!(windows) {
            "C:\\Windows\\y"
        } else {
            "/usr/y"
        });
        assert!(matches!(
            move_file(&pf, &tmp.path().join("z"), &jp, 1),
            Err(SafetyError::Protected(_))
        ));
        assert!(journal_recent(&jp, 10).is_empty());
    }

    #[test]
    fn normalize_for_guard_existing_path_canonicalizes_directly() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("exists.bin");
        std::fs::write(&f, b"x").unwrap();
        let expected = strip_verbatim(&std::fs::canonicalize(&f).unwrap());
        assert_eq!(normalize_for_guard(&f), expected);
    }

    #[test]
    fn normalize_for_guard_walks_up_to_existing_ancestor_for_missing_path() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("nested").join("does-not-exist.bin");
        let expected_base = strip_verbatim(&std::fs::canonicalize(tmp.path()).unwrap());
        assert_eq!(
            normalize_for_guard(&missing),
            expected_base.join("nested").join("does-not-exist.bin")
        );
    }

    #[test]
    fn normalize_for_guard_no_existing_ancestor_falls_back_to_lexical() {
        let p = Path::new("disksage-nonexistent-relative-xyz-zzz");
        assert_eq!(normalize_for_guard(p), strip_verbatim(p));
    }

    #[cfg(unix)]
    #[test]
    fn move_file_rejects_dst_via_symlinked_protected_ancestor() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let src = tmp.path().join("src.bin");
        std::fs::write(&src, b"x").unwrap();
        let link = tmp.path().join("media_link");
        std::os::unix::fs::symlink("/usr", &link).unwrap();
        let dst = link.join("evil.bin");
        let err = move_file(&src, &dst, &jp, 1);
        assert!(matches!(err, Err(SafetyError::Protected(_))));
        assert!(src.exists());
        assert!(journal_recent(&jp, 10).is_empty());
    }

    #[test]
    fn do_move_same_volume_branch_renames() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let src = tmp.path().join("a.bin");
        let dst = tmp.path().join("b.bin");
        std::fs::write(&src, vec![7u8; 30]).unwrap();
        do_move(&src, &dst, true, &jp, 1).unwrap();
        assert!(!src.exists());
        assert_eq!(std::fs::read(&dst).unwrap().len(), 30);
    }

    #[test]
    fn do_move_same_volume_hard_link_fails_when_dest_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let src = tmp.path().join("a.bin");
        let dst = tmp.path().join("b.bin");
        std::fs::write(&src, b"original").unwrap();
        std::fs::write(&dst, b"pre-existing").unwrap();
        let err = do_move(&src, &dst, true, &jp, 1);
        assert!(matches!(err, Err(SafetyError::Trash(_))));
        assert!(src.exists());
        assert_eq!(std::fs::read(&dst).unwrap(), b"pre-existing");
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn do_move_cross_volume_branch_copies_verifies_and_trashes() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let src = tmp.path().join("disksage-xvol-fixture.bin");
        let dst = tmp.path().join("moved-disksage-xvol-fixture.bin");
        std::fs::write(&src, vec![9u8; 40]).unwrap();
        do_move(&src, &dst, false, &jp, 2).unwrap();
        assert!(!src.exists());
        assert_eq!(std::fs::read(&dst).unwrap().len(), 40);
        let items: Vec<_> = trash::os_limited::list()
            .unwrap()
            .into_iter()
            .filter(|i| i.name.to_string_lossy().contains("disksage-xvol-fixture"))
            .collect();
        trash::os_limited::purge_all(items).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn copy_verified_io_preserves_mtime_and_mode() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("disksage-xvol-meta-fixture.bin");
        let dst = tmp.path().join("moved-disksage-xvol-meta-fixture.bin");
        std::fs::write(&src, vec![7u8; 32]).unwrap();
        std::fs::set_permissions(&src, std::fs::Permissions::from_mode(0o600)).unwrap();
        let past =
            std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_577_836_800);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&src)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(past))
            .unwrap();
        let want_mtime = std::fs::metadata(&src).unwrap().modified().unwrap();
        copy_verified_io(&src, &dst).unwrap();
        assert!(src.exists());
        let dst_md = std::fs::metadata(&dst).unwrap();
        assert_eq!(dst_md.modified().unwrap(), want_mtime);
        assert_eq!(dst_md.permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn preserve_source_metadata_errors_on_missing_source() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("gone.bin");
        let dst = tmp.path().join("dst.bin");
        std::fs::write(&dst, b"x").unwrap();
        assert!(preserve_source_metadata(&missing, &dst).is_err());
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
        std::fs::write(&dst, b"bb").unwrap();
        assert!(move_file(&src, &dst, &jp, 1).is_err());
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

    #[test]
    fn same_volume_missing_path_is_not_same_volume() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("no-such-file.tmp");
        assert!(!same_volume(&missing, tmp.path()));
    }

    #[test]
    fn move_file_rejects_parent_dir_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let sneaky = tmp.path().join("..");
        let dst = tmp.path().join("z.bin");
        let err = move_file(&sneaky, &dst, &jp, 1);
        assert!(matches!(err, Err(SafetyError::Protected(_))));
        assert!(journal_recent(&jp, 10).is_empty());
    }

    #[test]
    fn move_file_reports_error_when_dest_parent_cannot_be_created() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let src = tmp.path().join("src.bin");
        std::fs::write(&src, b"hi").unwrap();
        let blocker = tmp.path().join("blocker");
        std::fs::write(&blocker, b"not a dir").unwrap();
        let dst = blocker.join("nested").join("dst.bin");
        let err = move_file(&src, &dst, &jp, 1);
        assert!(matches!(err, Err(SafetyError::Trash(_))));
        assert!(src.exists());
        assert!(journal_recent(&jp, 10).is_empty());
    }

    #[test]
    fn move_file_same_volume_rename_failure_journals_error_outcome() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let src = tmp.path().join("d");
        std::fs::create_dir(&src).unwrap();
        let dst = src.join("inner").join("d");
        let err = move_file(&src, &dst, &jp, 5);
        assert!(matches!(err, Err(SafetyError::Trash(_))));
        let recent = journal_recent(&jp, 10);
        assert_eq!(recent.len(), 2);
        assert!(recent[0].outcome.starts_with("error:"));
        assert_eq!(recent[1].outcome, "pending");
    }

    #[test]
    fn copy_then_hash_reads_matching_size_and_hash_for_identical_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.bin");
        let dst = tmp.path().join("dst.bin");
        std::fs::write(&src, b"same-bytes-here").unwrap();
        let (sl, dl, sh, dh) = copy_then_hash(&src, &dst).unwrap();
        assert_eq!(sl, dl);
        assert_eq!(sh, dh);
        assert!(hashes_match(&sh, &dh, sl, dl));
    }

    #[test]
    fn hashes_match_detects_size_or_hash_mismatch() {
        let a = || Ok::<String, String>("a".into());
        let b = || Ok::<String, String>("b".into());
        assert!(!hashes_match(&a(), &a(), 1, 2));
        assert!(!hashes_match(&a(), &b(), 1, 1));
        assert!(hashes_match(&a(), &a(), 1, 1));
    }

    #[test]
    fn hashes_match_fails_closed_when_either_hash_errored() {
        let ok = || Ok::<String, String>("same-hash".into());
        let err = || Err::<String, String>("read failed".into());
        assert!(!hashes_match(&err(), &ok(), 10, 10));
        assert!(!hashes_match(&ok(), &err(), 10, 10));
        assert!(!hashes_match(&err(), &err(), 10, 10));
    }

    #[test]
    fn finalize_verified_copy_removes_dst_and_errors_when_unverified() {
        let tmp = tempfile::tempdir().unwrap();
        let dst = tmp.path().join("partial.bin");
        std::fs::write(&dst, b"partial").unwrap();
        assert!(finalize_verified_copy(&dst, false).is_err());
        assert!(!dst.exists());
    }

    #[test]
    fn finalize_verified_copy_keeps_dst_when_verified() {
        let tmp = tempfile::tempdir().unwrap();
        let dst = tmp.path().join("good.bin");
        std::fs::write(&dst, b"good").unwrap();
        assert!(finalize_verified_copy(&dst, true).is_ok());
        assert!(dst.exists());
    }

    #[test]
    fn copy_verified_io_succeeds_and_content_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("s2.bin");
        let dst = tmp.path().join("d2.bin");
        std::fs::write(&src, vec![9u8; 128]).unwrap();
        copy_verified_io(&src, &dst).unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), std::fs::read(&src).unwrap());
    }

    #[test]
    fn copy_verified_io_cleans_up_and_errors_when_copy_source_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let missing_src = tmp.path().join("does-not-exist.bin");
        let dst = tmp.path().join("never-created.bin");
        let err = copy_verified_io(&missing_src, &dst);
        assert!(err.is_err());
        assert!(!dst.exists());
    }

    #[test]
    fn copy_verified_io_does_not_overwrite_existing_destination() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("s3.bin");
        let dst = tmp.path().join("d3.bin");
        std::fs::write(&src, b"new-content").unwrap();
        std::fs::write(&dst, b"pre-existing").unwrap();
        let err = copy_verified_io(&src, &dst);
        assert!(err.is_err());
        assert_eq!(std::fs::read(&dst).unwrap(), b"pre-existing");
        assert!(src.exists());
    }
}
