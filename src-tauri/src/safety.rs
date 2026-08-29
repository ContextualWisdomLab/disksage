use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Recursive active-use probes for irreversible generated-tree deletion must accommodate real
/// dependency trees while remaining bounded and fail-closed.
pub(crate) const PERMANENT_DIRECTORY_ACTIVE_USE_TIMEOUT_MS: u64 = 30_000;

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
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .append(true)
        .open(journal_path)
        .map_err(journal_io_err)?;
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
        .map_err(journal_io_err)
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

fn create_private_staging_dir(path: &Path, now_ms: u64) -> std::io::Result<PathBuf> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let parent = std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
    let pid = std::process::id();
    for _ in 0..32 {
        let serial = STAGING_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".disksage-trash-{}-{}-{}", pid, now_ms, serial));
        match std::fs::create_dir(&candidate) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&candidate, std::fs::Permissions::from_mode(0o700))?;
                }
                return Ok(candidate);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate a private trash staging directory",
    ))
}

fn restore_staged_if_source_absent(
    path: &Path,
    staged: &Path,
    staging_dir: &Path,
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
    std::fs::remove_dir(staging_dir).map_err(|error| {
        format!(
            "staging directory cleanup failed for {}: {error}",
            staging_dir.display()
        )
    })?;
    Ok(())
}

fn remove_staged_permanently_with<F>(
    staged: &Path,
    staging_dir: &Path,
    remove: F,
) -> Result<(), SafetyError>
where
    F: FnOnce(&Path) -> std::io::Result<()>,
{
    if let Err(error) = remove(staged) {
        return Err(SafetyError::Trash(format!(
            "permanent deletion failed; staged object retained at {}: {error}",
            staged.display()
        )));
    }
    let _ = std::fs::remove_dir(staging_dir);
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashDeleteOutcome {
    pub moved_to_trash: bool,
    pub terminal_journal_error: Option<String>,
    pub staging_cleanup_error: Option<String>,
}

/// Return post-mutation warnings without rewriting a completed Trash move as a failed mutation.
pub fn trash_delete_outcome_warning(outcome: &TrashDeleteOutcome) -> Option<String> {
    let mut warnings = Vec::new();
    if let Some(error) = &outcome.terminal_journal_error {
        warnings.push(format!("trash move completed but terminal audit record failed: {error}"));
    }
    if let Some(error) = &outcome.staging_cleanup_error {
        warnings.push(error.clone());
    }
    (!warnings.is_empty()).then(|| warnings.join("; "))
}

fn trash_delete_outcome(
    mutation: Result<(), SafetyError>,
    terminal_journal: Result<(), SafetyError>,
    staging_cleanup_error: Option<String>,
) -> Result<TrashDeleteOutcome, SafetyError> {
    match mutation {
        Ok(()) => Ok(TrashDeleteOutcome {
            moved_to_trash: true,
            terminal_journal_error: terminal_journal.err().map(|error| error.to_string()),
            staging_cleanup_error,
        }),
        Err(error) => Err(error),
    }
}

fn cleanup_empty_staging_dir(staging_dir: &Path) -> Option<String> {
    std::fs::remove_dir(staging_dir)
        .err()
        .map(|error| format!("staging directory cleanup failed: {error}"))
}

pub fn trash_delete_if_identity_with_outcome(
    path: &Path,
    expected_object_id: &str,
    bytes: u64,
    journal_path: &Path,
    now_ms: u64,
) -> Result<TrashDeleteOutcome, SafetyError> {
    trash_delete_if_identity_with_verifier(
        path,
        expected_object_id,
        bytes,
        journal_path,
        now_ms,
        |_| true,
    )
}

pub(crate) fn trash_delete_if_identity_with_verifier<F>(
    path: &Path,
    expected_object_id: &str,
    bytes: u64,
    journal_path: &Path,
    now_ms: u64,
    evidence_matches: F,
) -> Result<TrashDeleteOutcome, SafetyError>
where
    F: Fn(&Path) -> bool,
{
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
    let actual = filesystem_object_id(path)
        .map_err(|error| SafetyError::Trash(format!("object identity unavailable: {error}")))?;
    if actual != expected_object_id {
        return Err(SafetyError::Trash(
            "개발 아티팩트의 파일시스템 객체가 바뀌었습니다. 다시 스캔하세요".into(),
        ));
    }
    let file_name = path.file_name().ok_or_else(|| {
        SafetyError::Trash("개발 아티팩트의 파일명이 없습니다. 다시 스캔하세요".into())
    })?;
    let staging_dir = create_private_staging_dir(path, now_ms)
        .map_err(|error| SafetyError::Trash(error.to_string()))?;
    let staged = staging_dir.join(file_name);
    let mut entry = JournalEntry {
        ts_ms: now_ms,
        op: "trash_delete".into(),
        path: path.to_string_lossy().into_owned(),
        bytes,
        outcome: "pending".into(),
    };
    if let Err(error) = journal_append(journal_path, &entry) {
        let _ = std::fs::remove_dir(&staging_dir);
        return Err(error);
    }

    let mut staging_cleanup_error = None;
    let result = (|| -> Result<(), SafetyError> {
        if !evidence_matches(path) {
            let _ = std::fs::remove_dir(&staging_dir);
            return Err(SafetyError::Trash(
                "reviewed contents changed; rescan before moving to Trash".into(),
            ));
        }
        if let Err(error) = std::fs::rename(path, &staged) {
            let _ = std::fs::remove_dir(&staging_dir);
            return Err(SafetyError::Trash(format!(
                "atomic staging move failed: {error}"
            )));
        }
        let moved_id = filesystem_object_id(&staged).map_err(|error| {
            let restore = restore_staged_if_source_absent(path, &staged, &staging_dir);
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
            return match restore_staged_if_source_absent(path, &staged, &staging_dir) {
                Ok(()) => Err(SafetyError::Trash(
                    "atomic staging move changed the filesystem object; nothing was trashed".into(),
                )),
                Err(restore_error) => Err(SafetyError::Trash(format!(
                    "atomic staging move changed the filesystem object; {restore_error}"
                ))),
            };
        }
        if !evidence_matches(&staged) {
            return match restore_staged_if_source_absent(path, &staged, &staging_dir) {
                Ok(()) => Err(SafetyError::Trash(
                    "staged contents changed; nothing was moved to Trash".into(),
                )),
                Err(restore_error) => Err(SafetyError::Trash(format!(
                    "staged contents changed; {restore_error}"
                ))),
            };
        }
        if let Err(error) = platform_trash_delete(&staged) {
            return match restore_staged_if_source_absent(path, &staged, &staging_dir) {
                Ok(()) => Err(SafetyError::Trash(error.to_string())),
                Err(restore_error) => {
                    Err(SafetyError::Trash(format!("{}; {restore_error}", error)))
                }
            };
        }
        staging_cleanup_error = cleanup_empty_staging_dir(&staging_dir);
        Ok(())
    })();
    entry.outcome = match &result {
        Ok(()) => "ok".into(),
        Err(error) => format!("error:{error}"),
    };
    let terminal_journal = journal_append(journal_path, &entry);
    trash_delete_outcome(result, terminal_journal, staging_cleanup_error)
}

/// Move an unchanged cache target to Trash after revalidating its complete bounded manifest on
/// both sides of the atomic staging rename.
pub(crate) fn trash_delete_cache_target_with_outcome(
    path: &Path,
    expected_object_id: &str,
    bytes: u64,
    expected_modified_ms: u64,
    expected_manifest_fingerprint: &str,
    journal_path: &Path,
    now_ms: u64,
) -> Result<TrashDeleteOutcome, SafetyError> {
    trash_delete_if_identity_with_verifier(
        path,
        expected_object_id,
        bytes,
        journal_path,
        now_ms,
        |candidate_path| {
            crate::rules::cache_target(candidate_path)
                .ok()
                .is_some_and(|target| {
                    target.object_id == expected_object_id
                        && target.bytes == bytes
                        && target.modified_ms == expected_modified_ms
                        && target.manifest_fingerprint == expected_manifest_fingerprint
                })
        },
    )
}

pub fn trash_delete_if_identity(
    path: &Path,
    expected_object_id: &str,
    bytes: u64,
    journal_path: &Path,
    now_ms: u64,
) -> Result<(), SafetyError> {
    let outcome = trash_delete_if_identity_with_outcome(
        path,
        expected_object_id,
        bytes,
        journal_path,
        now_ms,
    )?;
    completed_trash_move(outcome)
}

/// Preserve truthful mutation and audit status for legacy callers that expose only one result.
///
/// Callers that can publish post-mutation warnings use `trash_delete_if_identity_with_outcome`.
/// A terminal journal failure must reach legacy callers even though the completed OS Trash move
/// remains represented by the outcome-aware API. Empty staging-directory cleanup is advisory.
fn completed_trash_move(outcome: TrashDeleteOutcome) -> Result<(), SafetyError> {
    if !outcome.moved_to_trash {
        return Err(SafetyError::Trash(
            "trash move did not complete; rescan before cleanup".into(),
        ));
    }
    if let Some(error) = outcome.terminal_journal_error {
        return Err(SafetyError::Journal(format!(
            "trash move completed but its terminal audit record failed: {error}"
        )));
    }
    Ok(())
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
    expected_modified_ms: u64,
    expected_manifest_fingerprint: &str,
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
    let manifest_matches = |candidate_path: &Path| {
        crate::rules::cache_target(candidate_path)
            .ok()
            .is_some_and(|target| {
                target.object_id == expected_object_id
                    && target.bytes == bytes
                    && target.modified_ms == expected_modified_ms
                    && target.manifest_fingerprint == expected_manifest_fingerprint
            })
    };
    let file_name = path.file_name().ok_or_else(|| {
        SafetyError::Trash("generated directory has no file name; rescan before deletion".into())
    })?;
    let staging_dir = create_private_staging_dir(path, now_ms)
        .map_err(|error| SafetyError::Trash(error.to_string()))?;
    let staged = staging_dir.join(file_name);
    let mut entry = JournalEntry {
        ts_ms: now_ms,
        op: "permanent_generated_directory_delete".into(),
        path: path.to_string_lossy().into_owned(),
        bytes,
        outcome: "pending".into(),
    };
    if let Err(error) = journal_append(journal_path, &entry) {
        let _ = std::fs::remove_dir(&staging_dir);
        return Err(error);
    }
    let result = (|| -> Result<(), SafetyError> {
        if !manifest_matches(path) {
            let _ = std::fs::remove_dir(&staging_dir);
            return Err(SafetyError::Trash(
                "generated directory manifest changed; rescan before deletion".into(),
            ));
        }
        if let Err(error) = std::fs::rename(path, &staged) {
            let _ = std::fs::remove_dir(&staging_dir);
            return Err(SafetyError::Trash(format!(
                "atomic staging move failed: {error}"
            )));
        }
        let moved_id = filesystem_object_id(&staged).map_err(|error| {
            let restore = restore_staged_if_source_absent(path, &staged, &staging_dir);
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
            return match restore_staged_if_source_absent(path, &staged, &staging_dir) {
                Ok(()) => Err(SafetyError::Trash(
                    "atomic staging move changed the generated directory; nothing was deleted"
                        .into(),
                )),
                Err(restore_error) => Err(SafetyError::Trash(format!(
                    "atomic staging move changed the generated directory; {restore_error}"
                ))),
            };
        }
        // The caller's probe precedes the atomic rename and therefore cannot close the final
        // open-handle race by itself.  Once staged, the original pathname is unavailable to new
        // users; recursively probe the exact staged object before the irreversible removal.
        let active_use = crate::git_worktree::active_use_evidence_with_command_path(
            &staged,
            path,
            PERMANENT_DIRECTORY_ACTIVE_USE_TIMEOUT_MS,
            crate::reclaim::ACTIVE_USE_PROBE_MAX_PIDS,
            true,
        );
        if !active_use.assessed || !active_use.evidence_complete || active_use.active {
            let reason = if active_use.active {
                "staged generated directory is still in active use"
            } else {
                "staged generated directory active-use evidence is incomplete"
            };
            return match restore_staged_if_source_absent(path, &staged, &staging_dir) {
                Ok(()) => Err(SafetyError::Trash(format!("{reason}; nothing was deleted"))),
                Err(restore_error) => Err(SafetyError::Trash(format!("{reason}; {restore_error}"))),
            };
        }
        if !manifest_matches(&staged) {
            return match restore_staged_if_source_absent(path, &staged, &staging_dir) {
                Ok(()) => Err(SafetyError::Trash(
                    "staged generated directory manifest changed; nothing was deleted".into(),
                )),
                Err(restore_error) => Err(SafetyError::Trash(format!(
                    "staged generated directory manifest changed; {restore_error}"
                ))),
            };
        }
        remove_staged_permanently_with(&staged, &staging_dir, |path| std::fs::remove_dir_all(path))
    })();
    entry.outcome = match &result {
        Ok(()) => "ok".into(),
        Err(error) => format!("error:{error}"),
    };
    journal_append(journal_path, &entry)?;
    result
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

    #[test]
    fn successful_trash_preserves_moved_state_when_terminal_journal_fails() {
        let outcome =
            trash_delete_outcome(Ok(()), Err(SafetyError::Journal("disk-full".into())), None)
                .unwrap();
        assert!(outcome.moved_to_trash);
        assert_eq!(
            outcome.terminal_journal_error.as_deref(),
            Some("저널 기록 실패: disk-full")
        );
        let warning = trash_delete_outcome_warning(&outcome).unwrap();
        assert!(warning.contains("terminal audit record failed"));
        assert!(warning.contains("disk-full"));
    }

    #[test]
    fn legacy_result_propagates_terminal_audit_failure_after_completed_move() {
        let outcome = TrashDeleteOutcome {
            moved_to_trash: true,
            terminal_journal_error: Some("journal device full".into()),
            staging_cleanup_error: None,
        };

        let error = completed_trash_move(outcome).unwrap_err();
        assert!(matches!(error, SafetyError::Journal(message) if message.contains("journal device full")));
    }

    #[test]
    fn legacy_result_keeps_completed_move_success_when_only_empty_staging_cleanup_fails() {
        let outcome = TrashDeleteOutcome {
            moved_to_trash: true,
            terminal_journal_error: None,
            staging_cleanup_error: Some("staging cleanup failed".into()),
        };

        assert!(completed_trash_move(outcome).is_ok());
    }

    #[test]
    fn successful_trash_removes_its_empty_private_staging_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let staging = tmp.path().join(".disksage-trash-staging");
        std::fs::create_dir(&staging).unwrap();
        assert_eq!(cleanup_empty_staging_dir(&staging), None);
        assert!(!staging.exists());
    }
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
    fn permanent_generated_directory_delete_rechecks_identity_and_journals() {
        let tmp = tempfile::tempdir().unwrap();
        let generated = tmp.path().join("node_modules");
        std::fs::create_dir(&generated).unwrap();
        std::fs::write(generated.join("generated.bin"), b"generated").unwrap();
        let target = crate::rules::cache_target(&generated).unwrap();
        let journal = tmp.path().join("journal.jsonl");

        permanent_delete_dir_if_identity(
            &generated,
            &target.object_id,
            target.bytes,
            target.modified_ms,
            &target.manifest_fingerprint,
            &journal,
            1,
        )
        .unwrap();

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
    fn permanent_generated_directory_delete_rejects_a_stale_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let generated = tmp.path().join("generated-cache");
        std::fs::create_dir(&generated).unwrap();
        let nested = generated.join("generated.bin");
        std::fs::write(&nested, b"before!").unwrap();
        let target = crate::rules::cache_target(&generated).unwrap();
        std::fs::write(&nested, b"changed").unwrap();
        let journal = tmp.path().join("journal.jsonl");

        let result = permanent_delete_dir_if_identity(
            &generated,
            &target.object_id,
            target.bytes,
            target.modified_ms,
            &target.manifest_fingerprint,
            &journal,
            2,
        );

        assert!(result.is_err());
        assert!(generated.exists());
        let entries = journal_recent(&journal, 2);
        assert_eq!(entries.len(), 2);
        assert!(entries[0].outcome.contains("manifest changed"));
    }

    #[cfg(unix)]
    #[test]
    fn permanent_generated_directory_delete_restores_an_open_tree() {
        let tmp = tempfile::tempdir().unwrap();
        let generated = tmp.path().join("generated-cache");
        std::fs::create_dir(&generated).unwrap();
        let open_file = std::fs::File::create(generated.join("in-use.bin")).unwrap();
        let target = crate::rules::cache_target(&generated).unwrap();
        let journal = tmp.path().join("journal.jsonl");

        let result = permanent_delete_dir_if_identity(
            &generated,
            &target.object_id,
            target.bytes,
            target.modified_ms,
            &target.manifest_fingerprint,
            &journal,
            2,
        );

        drop(open_file);
        assert!(result.is_err());
        assert!(generated.exists());
        assert!(journal_recent(&journal, 2)[0]
            .outcome
            .contains("active use"));
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

        let result = remove_staged_permanently_with(&staged, &staging_dir, |path| {
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
        let staging_dir = tmp.path().join(".disksage-trash-staging");
        let staged = staging_dir.join("source");
        std::fs::write(&source, b"replacement").unwrap();
        std::fs::create_dir(&staging_dir).unwrap();
        std::fs::write(&staged, b"reviewed").unwrap();
        let error = restore_staged_if_source_absent(&source, &staged, &staging_dir).unwrap_err();
        assert!(error.contains(staged.to_string_lossy().as_ref()));
        assert!(source.exists());
        assert!(staged.exists());
    }

    #[test]
    fn staged_restore_reports_rename_failure_with_staged_path() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let staging_dir = tmp.path().join(".disksage-trash-staging");
        let staged = staging_dir.join("source");
        std::fs::create_dir(&staging_dir).unwrap();
        let error = restore_staged_if_source_absent(&source, &staged, &staging_dir).unwrap_err();
        assert!(error.contains(staged.to_string_lossy().as_ref()));
        assert!(staging_dir.exists());
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
        let jp = tmp.path().join("j.jsonl");
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

    #[test]
    fn trash_staging_restores_when_manifest_changes_at_mutation_boundary() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("reviewed-cache");
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("payload.bin"), b"reviewed").unwrap();
        let object_id = filesystem_object_id(&target).unwrap();
        let journal = tmp.path().join("journal.jsonl");
        let probes = AtomicUsize::new(0);

        let error = trash_delete_if_identity_with_verifier(
            &target,
            &object_id,
            8,
            &journal,
            77,
            |_| probes.fetch_add(1, Ordering::SeqCst) == 0,
        )
        .unwrap_err();

        assert!(error.to_string().contains("staged contents changed"));
        assert!(target.exists(), "the reviewed object must be restored");
        assert_eq!(std::fs::read(target.join("payload.bin")).unwrap(), b"reviewed");
        assert_eq!(probes.load(Ordering::SeqCst), 2);
    }
}
