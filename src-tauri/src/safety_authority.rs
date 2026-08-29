use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub use crate::safety_core::{
    filesystem_object_id, is_protected, journal_append, journal_recent, move_file,
    object_id_from_metadata, same_volume, trash_delete, trash_delete_if_identity,
    trash_delete_if_identity_with_outcome, trash_delete_outcome_warning, JournalEntry, SafetyError,
    TrashDeleteOutcome,
};
pub(crate) use crate::safety_core::{
    is_shared_temp_path, is_user_owned_shared_temp_tree, PERMANENT_DIRECTORY_ACTIVE_USE_TIMEOUT_MS,
};

static CACHE_STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
std::thread_local! {
    static INJECT_FINAL_CACHE_AUTHORITY_FAILURE: std::cell::Cell<bool> = std::cell::Cell::new(false);
    static INJECT_TERMINAL_CACHE_JOURNAL_FAILURE: std::cell::Cell<bool> = std::cell::Cell::new(false);
}

#[cfg(test)]
fn inject_final_cache_authority_failure_once() {
    INJECT_FINAL_CACHE_AUTHORITY_FAILURE.with(|flag| flag.set(true));
}

#[cfg(test)]
fn inject_terminal_cache_journal_failure_once() {
    INJECT_TERMINAL_CACHE_JOURNAL_FAILURE.with(|flag| flag.set(true));
}

#[cfg(windows)]
fn strip_verbatim(path: &Path) -> PathBuf {
    use std::path::{Component, Prefix};
    let mut components = path.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return path.to_path_buf();
    };
    match prefix.kind() {
        Prefix::VerbatimDisk(drive) => {
            let mut out = PathBuf::from(format!("{}:\\", drive as char));
            out.extend(components.filter(|component| !matches!(component, Component::RootDir)));
            out
        }
        Prefix::VerbatimUNC(server, share) => {
            let mut out = PathBuf::from(r"\\");
            out.push(server);
            out.push(share);
            out.extend(components.filter(|component| !matches!(component, Component::RootDir)));
            out
        }
        _ => path.to_path_buf(),
    }
}

#[cfg(not(windows))]
fn strip_verbatim(path: &Path) -> PathBuf {
    path.to_path_buf()
}

fn authorize_cache_path(path: &Path) -> Result<(), SafetyError> {
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(SafetyError::Protected(path.to_path_buf()));
    }
    let guard_path = strip_verbatim(
        &std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()),
    );
    let shared_temp = is_shared_temp_path(&guard_path);
    let shared_temp_authorized = shared_temp && is_user_owned_shared_temp_tree(&guard_path);
    if shared_temp && !shared_temp_authorized {
        return Err(SafetyError::Protected(path.to_path_buf()));
    }
    if !shared_temp_authorized && is_protected(&guard_path) {
        return Err(SafetyError::Protected(path.to_path_buf()));
    }
    Ok(())
}

fn create_private_cache_staging_dir(path: &Path, now_ms: u64) -> std::io::Result<PathBuf> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let parent = std::fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
    let pid = std::process::id();
    for _ in 0..32 {
        let serial = CACHE_STAGING_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".disksage-trash-{}-{}-{}",
            pid, now_ms, serial
        ));
        match std::fs::create_dir(&candidate) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(
                        &candidate,
                        std::fs::Permissions::from_mode(0o700),
                    )?;
                }
                return Ok(candidate);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate a private cache staging directory",
    ))
}

fn restore_staged_cache(
    source: &Path,
    staged: &Path,
    staging_dir: &Path,
) -> Result<(), String> {
    let source_absent = matches!(
        std::fs::symlink_metadata(source),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    );
    if !source_absent {
        return Err(format!(
            "staged cache retained at {}; source path reappeared",
            staged.display()
        ));
    }
    std::fs::rename(staged, source)
        .map_err(|error| format!("staged cache restore failed for {}: {error}", staged.display()))?;
    std::fs::remove_dir(staging_dir).map_err(|error| {
        format!(
            "cache staging directory cleanup failed for {}: {error}",
            staging_dir.display()
        )
    })?;
    Ok(())
}

fn cleanup_empty_cache_staging_dir(staging_dir: &Path) -> Option<String> {
    std::fs::remove_dir(staging_dir)
        .err()
        .map(|error| format!("cache staging directory cleanup failed: {error}"))
}

#[cfg(target_os = "macos")]
fn platform_cache_trash_delete(path: &Path) -> Result<(), trash::Error> {
    use trash::macos::{DeleteMethod, TrashContextExtMacos};
    let mut context = trash::TrashContext::new();
    context.set_delete_method(DeleteMethod::NsFileManager);
    context.delete(path)
}

#[cfg(not(target_os = "macos"))]
fn platform_cache_trash_delete(path: &Path) -> Result<(), trash::Error> {
    trash::delete(path)
}

fn authority_manifest_error(message: impl Into<String>) -> SafetyError {
    SafetyError::Trash(message.into())
}

fn final_cache_authority_target(path: &Path) -> Result<crate::rules::CacheTarget, String> {
    #[cfg(test)]
    if INJECT_FINAL_CACHE_AUTHORITY_FAILURE.with(|flag| flag.replace(false)) {
        return Err("injected-final-cache-authority-failure".into());
    }
    crate::rules::cache_authority_target(path)
}

fn append_cache_journal(journal_path: &Path, entry: &JournalEntry) -> Result<(), SafetyError> {
    #[cfg(test)]
    if entry.outcome != "pending"
        && INJECT_TERMINAL_CACHE_JOURNAL_FAILURE.with(|flag| flag.replace(false))
    {
        return Err(SafetyError::Journal(
            "injected-terminal-cache-journal-failure".into(),
        ));
    }
    journal_append(journal_path, entry)
}

fn cache_authority_snapshot(
    path: &Path,
    expected_object_id: &str,
    bytes: u64,
    expected_modified_ms: u64,
    expected_manifest_fingerprint: &str,
    require_reviewed_root: bool,
) -> Result<crate::rules::CacheTarget, SafetyError> {
    let (expected_root, expected_stable) =
        crate::rules::cache_manifest_components(expected_manifest_fingerprint).ok_or_else(|| {
            authority_manifest_error(
                "cache manifest authority version is unavailable; rescan before cleanup",
            )
        })?;
    let target = crate::rules::cache_authority_target(path)
        .map_err(|error| authority_manifest_error(format!("cache authority unavailable: {error}")))?;
    let (actual_root, actual_stable) = crate::rules::cache_manifest_components(
        &target.manifest_fingerprint,
    )
    .ok_or_else(|| authority_manifest_error("cache authority snapshot is malformed"))?;
    if target.object_id != expected_object_id
        || target.bytes != bytes
        || target.modified_ms != expected_modified_ms
        || actual_stable != expected_stable
        || (require_reviewed_root && actual_root != expected_root)
    {
        return Err(authority_manifest_error(
            "cache target authority changed; rescan before cleanup",
        ));
    }
    Ok(target)
}

fn cache_manifest_is_v2(manifest: &str) -> bool {
    crate::rules::cache_manifest_components(manifest).is_some()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PermanentDeleteOutcome {
    pub deleted: bool,
    pub terminal_journal_error: Option<String>,
    pub staging_cleanup_error: Option<String>,
}

pub(crate) fn permanent_delete_outcome_warning(outcome: &PermanentDeleteOutcome) -> Option<String> {
    let mut warnings = Vec::new();
    if let Some(error) = &outcome.terminal_journal_error {
        warnings.push(format!("terminal journal append failed after deletion: {error}"));
    }
    if let Some(error) = &outcome.staging_cleanup_error {
        warnings.push(error.clone());
    }
    (!warnings.is_empty()).then(|| warnings.join("; "))
}

fn restore_final_snapshot_failure(
    path: &Path,
    staged: &Path,
    staging_dir: &Path,
    error: String,
) -> Result<crate::rules::CacheTarget, SafetyError> {
    let authority_error = authority_manifest_error(format!("cache authority unavailable: {error}"));
    match restore_staged_cache(path, staged, staging_dir) {
        Ok(()) => Err(authority_error),
        Err(restore_error) => Err(authority_manifest_error(format!(
            "{authority_error}; {restore_error}"
        ))),
    }
}

/// Move an unchanged cache target to Trash after binding the original pathname to full reviewed
/// root metadata. After the atomic rename, the expected rename-induced root ctime is recaptured and
/// the relocation-stable tree is checked twice before Trash receives the staged object.
pub(crate) fn trash_delete_cache_target_with_outcome(
    path: &Path,
    expected_object_id: &str,
    bytes: u64,
    expected_modified_ms: u64,
    expected_manifest_fingerprint: &str,
    journal_path: &Path,
    now_ms: u64,
) -> Result<TrashDeleteOutcome, SafetyError> {
    if !cache_manifest_is_v2(expected_manifest_fingerprint) {
        if expected_manifest_fingerprint.starts_with("v2:") {
            return Err(authority_manifest_error(
                "cache manifest authority is malformed; rescan before cleanup",
            ));
        }
        return crate::safety_core::trash_delete_cache_target_with_outcome(
            path,
            expected_object_id,
            bytes,
            expected_modified_ms,
            expected_manifest_fingerprint,
            journal_path,
            now_ms,
        );
    }

    authorize_cache_path(path)?;
    let actual = filesystem_object_id(path)
        .map_err(|error| authority_manifest_error(format!("object identity unavailable: {error}")))?;
    if actual != expected_object_id {
        return Err(authority_manifest_error(
            "cache target filesystem object changed; rescan before cleanup",
        ));
    }
    let file_name = path.file_name().ok_or_else(|| {
        authority_manifest_error("cache target has no file name; rescan before cleanup")
    })?;
    let staging_dir = create_private_cache_staging_dir(path, now_ms)
        .map_err(|error| authority_manifest_error(error.to_string()))?;
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
    let mutation = (|| -> Result<(), SafetyError> {
        cache_authority_snapshot(
            path,
            expected_object_id,
            bytes,
            expected_modified_ms,
            expected_manifest_fingerprint,
            true,
        )?;
        if let Err(error) = std::fs::rename(path, &staged) {
            let _ = std::fs::remove_dir(&staging_dir);
            return Err(authority_manifest_error(format!(
                "atomic cache staging move failed: {error}"
            )));
        }
        let moved_id = filesystem_object_id(&staged).map_err(|error| {
            let restore = restore_staged_cache(path, &staged, &staging_dir);
            match restore {
                Ok(()) => authority_manifest_error(format!(
                    "staged cache object identity unavailable: {error}"
                )),
                Err(restore_error) => authority_manifest_error(format!(
                    "staged cache object identity unavailable: {error}; {restore_error}"
                )),
            }
        })?;
        if moved_id != expected_object_id {
            return match restore_staged_cache(path, &staged, &staging_dir) {
                Ok(()) => Err(authority_manifest_error(
                    "atomic cache staging move changed the filesystem object; nothing was trashed",
                )),
                Err(restore_error) => Err(authority_manifest_error(format!(
                    "atomic cache staging move changed the filesystem object; {restore_error}"
                ))),
            };
        }
        let staged_baseline = match cache_authority_snapshot(
            &staged,
            expected_object_id,
            bytes,
            expected_modified_ms,
            expected_manifest_fingerprint,
            false,
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return match restore_staged_cache(path, &staged, &staging_dir) {
                    Ok(()) => Err(error),
                    Err(restore_error) => Err(authority_manifest_error(format!(
                        "{error}; {restore_error}"
                    ))),
                }
            }
        };
        let staged_live = match final_cache_authority_target(&staged) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return restore_final_snapshot_failure(path, &staged, &staging_dir, error)
                    .map(|_| ());
            }
        };
        if staged_live != staged_baseline {
            return match restore_staged_cache(path, &staged, &staging_dir) {
                Ok(()) => Err(authority_manifest_error(
                    "staged cache metadata changed; nothing was moved to Trash",
                )),
                Err(restore_error) => Err(authority_manifest_error(format!(
                    "staged cache metadata changed; {restore_error}"
                ))),
            };
        }
        if let Err(error) = platform_cache_trash_delete(&staged) {
            return match restore_staged_cache(path, &staged, &staging_dir) {
                Ok(()) => Err(authority_manifest_error(error.to_string())),
                Err(restore_error) => Err(authority_manifest_error(format!(
                    "{error}; {restore_error}"
                ))),
            };
        }
        staging_cleanup_error = cleanup_empty_cache_staging_dir(&staging_dir);
        Ok(())
    })();

    entry.outcome = match &mutation {
        Ok(()) => "ok".into(),
        Err(error) => format!("error:{error}"),
    };
    let terminal_journal = journal_append(journal_path, &entry);
    match mutation {
        Ok(()) => Ok(TrashDeleteOutcome {
            moved_to_trash: true,
            terminal_journal_error: terminal_journal.err().map(|error| error.to_string()),
            staging_cleanup_error,
        }),
        Err(error) => Err(error),
    }
}

/// Permanently remove one unchanged generated cache directory. The reviewed original root must
/// match its full metadata (including Unix ctime/uid/gid). The staged root establishes a new
/// post-rename baseline, which is rechecked after the recursive active-use probe before deletion.
pub fn permanent_delete_dir_if_identity(
    path: &Path,
    expected_object_id: &str,
    bytes: u64,
    expected_modified_ms: u64,
    expected_manifest_fingerprint: &str,
    journal_path: &Path,
    now_ms: u64,
) -> Result<PermanentDeleteOutcome, SafetyError> {
    if !cache_manifest_is_v2(expected_manifest_fingerprint) {
        if expected_manifest_fingerprint.starts_with("v2:") {
            return Err(authority_manifest_error(
                "cache manifest authority is malformed; rescan before deletion",
            ));
        }
        return crate::safety_core::permanent_delete_dir_if_identity(
            path,
            expected_object_id,
            bytes,
            expected_modified_ms,
            expected_manifest_fingerprint,
            journal_path,
            now_ms,
        )
        .map(|()| PermanentDeleteOutcome {
            deleted: true,
            terminal_journal_error: None,
            staging_cleanup_error: None,
        });
    }

    authorize_cache_path(path)?;
    let metadata =
        std::fs::symlink_metadata(path).map_err(|error| authority_manifest_error(error.to_string()))?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(authority_manifest_error(
            "permanent deletion requires a real generated directory",
        ));
    }
    let actual = filesystem_object_id(path)
        .map_err(|error| authority_manifest_error(format!("object identity unavailable: {error}")))?;
    if actual != expected_object_id {
        return Err(authority_manifest_error(
            "generated directory identity changed; rescan before deletion",
        ));
    }
    let file_name = path.file_name().ok_or_else(|| {
        authority_manifest_error("generated directory has no file name; rescan before deletion")
    })?;
    let staging_dir = create_private_cache_staging_dir(path, now_ms)
        .map_err(|error| authority_manifest_error(error.to_string()))?;
    let staged = staging_dir.join(file_name);
    let mut entry = JournalEntry {
        ts_ms: now_ms,
        op: "permanent_generated_directory_delete".into(),
        path: path.to_string_lossy().into_owned(),
        bytes,
        outcome: "pending".into(),
    };
    if let Err(error) = append_cache_journal(journal_path, &entry) {
        let _ = std::fs::remove_dir(&staging_dir);
        return Err(error);
    }

    let mutation = (|| -> Result<Option<String>, SafetyError> {
        cache_authority_snapshot(
            path,
            expected_object_id,
            bytes,
            expected_modified_ms,
            expected_manifest_fingerprint,
            true,
        )?;
        if let Err(error) = std::fs::rename(path, &staged) {
            let _ = std::fs::remove_dir(&staging_dir);
            return Err(authority_manifest_error(format!(
                "atomic cache staging move failed: {error}"
            )));
        }
        let moved_id = filesystem_object_id(&staged).map_err(|error| {
            let restore = restore_staged_cache(path, &staged, &staging_dir);
            match restore {
                Ok(()) => authority_manifest_error(format!(
                    "staged generated directory identity unavailable: {error}"
                )),
                Err(restore_error) => authority_manifest_error(format!(
                    "staged generated directory identity unavailable: {error}; {restore_error}"
                )),
            }
        })?;
        if moved_id != expected_object_id {
            return match restore_staged_cache(path, &staged, &staging_dir) {
                Ok(()) => Err(authority_manifest_error(
                    "atomic cache staging move changed the generated directory; nothing was deleted",
                )),
                Err(restore_error) => Err(authority_manifest_error(format!(
                    "atomic cache staging move changed the generated directory; {restore_error}"
                ))),
            };
        }
        let staged_baseline = match cache_authority_snapshot(
            &staged,
            expected_object_id,
            bytes,
            expected_modified_ms,
            expected_manifest_fingerprint,
            false,
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return match restore_staged_cache(path, &staged, &staging_dir) {
                    Ok(()) => Err(error),
                    Err(restore_error) => Err(authority_manifest_error(format!(
                        "{error}; {restore_error}"
                    ))),
                }
            }
        };

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
            return match restore_staged_cache(path, &staged, &staging_dir) {
                Ok(()) => Err(authority_manifest_error(format!(
                    "{reason}; nothing was deleted"
                ))),
                Err(restore_error) => Err(authority_manifest_error(format!(
                    "{reason}; {restore_error}"
                ))),
            };
        }

        let staged_live = match final_cache_authority_target(&staged) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                return restore_final_snapshot_failure(path, &staged, &staging_dir, error)
                    .map(|_| None);
            }
        };
        if staged_live != staged_baseline {
            return match restore_staged_cache(path, &staged, &staging_dir) {
                Ok(()) => Err(authority_manifest_error(
                    "staged generated directory metadata changed; nothing was deleted",
                )),
                Err(restore_error) => Err(authority_manifest_error(format!(
                    "staged generated directory metadata changed; {restore_error}"
                ))),
            };
        }

        if let Err(error) = std::fs::remove_dir_all(&staged) {
            return Err(authority_manifest_error(format!(
                "permanent deletion failed; staged object retained at {}: {error}",
                staged.display()
            )));
        }
        Ok(cleanup_empty_cache_staging_dir(&staging_dir))
    })();

    entry.outcome = match &mutation {
        Ok(_) => "ok".into(),
        Err(error) => format!("error:{error}"),
    };
    let terminal_journal = append_cache_journal(journal_path, &entry);
    match mutation {
        Ok(staging_cleanup_error) => Ok(PermanentDeleteOutcome {
            deleted: true,
            terminal_journal_error: terminal_journal.err().map(|error| error.to_string()),
            staging_cleanup_error,
        }),
        Err(error) => match terminal_journal {
            Ok(()) => Err(error),
            Err(journal_error) => Err(authority_manifest_error(format!(
                "{error}; terminal journal append failed: {journal_error}"
            ))),
        },
    }
}

#[cfg(all(test, unix))]
mod authority_failure_tests {
    use super::*;

    fn fixture(label: &str) -> (tempfile::TempDir, PathBuf, crate::rules::CacheTarget, PathBuf) {
        let temp = tempfile::tempdir().expect("tempdir");
        let cache = temp.path().join(label);
        std::fs::create_dir(&cache).expect("cache dir");
        std::fs::write(cache.join("payload.bin"), b"disk-sage-test-payload").expect("payload");
        let target = crate::rules::cache_authority_target(&cache).expect("authority target");
        let journal = temp.path().join("journal.jsonl");
        (temp, cache, target, journal)
    }

    #[test]
    fn reversible_final_snapshot_failure_restores_original_cache() {
        let (_temp, cache, target, journal) = fixture("reversible-cache");
        inject_final_cache_authority_failure_once();

        let error = trash_delete_cache_target_with_outcome(
            &cache,
            &target.object_id,
            target.bytes,
            target.modified_ms,
            &target.manifest_fingerprint,
            &journal,
            1_000,
        )
        .expect_err("injected final snapshot failure must abort Trash");

        assert!(error.to_string().contains("injected-final-cache-authority-failure"));
        assert!(cache.is_dir(), "the original cache path must be restored");
        assert_eq!(
            std::fs::read(cache.join("payload.bin")).expect("restored payload"),
            b"disk-sage-test-payload"
        );
    }

    #[test]
    fn permanent_final_snapshot_failure_restores_original_cache() {
        let (_temp, cache, target, journal) = fixture("permanent-cache");
        inject_final_cache_authority_failure_once();

        let error = permanent_delete_dir_if_identity(
            &cache,
            &target.object_id,
            target.bytes,
            target.modified_ms,
            &target.manifest_fingerprint,
            &journal,
            2_000,
        )
        .expect_err("injected final snapshot failure must abort permanent deletion");

        assert!(error.to_string().contains("injected-final-cache-authority-failure"));
        assert!(cache.is_dir(), "the original cache path must be restored");
        assert_eq!(
            std::fs::read(cache.join("payload.bin")).expect("restored payload"),
            b"disk-sage-test-payload"
        );
    }

    #[test]
    fn completed_permanent_delete_survives_terminal_journal_failure() {
        let (_temp, cache, target, journal) = fixture("terminal-journal-cache");
        inject_terminal_cache_journal_failure_once();

        let outcome = permanent_delete_dir_if_identity(
            &cache,
            &target.object_id,
            target.bytes,
            target.modified_ms,
            &target.manifest_fingerprint,
            &journal,
            3_000,
        )
        .expect("completed deletion must remain a completed outcome");

        assert!(outcome.deleted);
        assert!(!cache.exists(), "the generated cache was already deleted");
        assert!(outcome
            .terminal_journal_error
            .as_deref()
            .is_some_and(|error| error.contains("injected-terminal-cache-journal-failure")));
        assert!(permanent_delete_outcome_warning(&outcome)
            .is_some_and(|warning| warning.contains("terminal journal")));
    }
}
