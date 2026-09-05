//! Object-bound atomic replacement for private local records.
//!
//! The Unix implementation keeps parent authority descriptor-relative and revalidates the named
//! staging object against the exact opened file before `renameat`, then verifies the published name
//! against that same opened object. Pathname revalidation detects namespace drift; it is not treated
//! as equivalent to a source-handle-conditioned rename on platforms that do not provide one.

use std::path::Path;

/// Stable failure classes for object-bound private-record replacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ObjectBoundReplaceError {
    ParentMissing,
    ParentUnavailable,
    ParentUnsafe,
    ParentWritableByOthers,
    ParentIdentityDrift,
    NameInvalid,
    TargetUnsafe,
    TargetUnavailable,
    TemporaryCreateFailed,
    TemporaryIdentityDrift,
    ModeInvalid,
    WriteFailed,
    CleanupFailed,
    RenameFailed,
    DirectorySyncFailed,
    PostPublishTemporaryIdentityDrift,
    PostPublishParentIdentityDrift,
    UnsupportedPlatform,
}

impl ObjectBoundReplaceError {
    /// Stable machine-readable failure identifier for domain adapters.
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::ParentMissing => "object-bound-replace-parent-missing",
            Self::ParentUnavailable => "object-bound-replace-parent-unavailable",
            Self::ParentUnsafe => "object-bound-replace-parent-unsafe",
            Self::ParentWritableByOthers => "object-bound-replace-parent-writable-by-others",
            Self::ParentIdentityDrift => "object-bound-replace-parent-identity-drift",
            Self::NameInvalid => "object-bound-replace-name-invalid",
            Self::TargetUnsafe => "object-bound-replace-target-unsafe",
            Self::TargetUnavailable => "object-bound-replace-target-unavailable",
            Self::TemporaryCreateFailed => "object-bound-replace-temporary-create-failed",
            Self::TemporaryIdentityDrift => "object-bound-replace-temporary-identity-drift",
            Self::ModeInvalid => "object-bound-replace-mode-invalid",
            Self::WriteFailed => "object-bound-replace-write-failed",
            Self::CleanupFailed => "object-bound-replace-cleanup-failed",
            Self::RenameFailed => "object-bound-replace-rename-failed",
            Self::DirectorySyncFailed => "object-bound-replace-directory-sync-failed",
            Self::PostPublishTemporaryIdentityDrift => {
                "object-bound-replace-post-publish-temporary-identity-drift"
            }
            Self::PostPublishParentIdentityDrift => {
                "object-bound-replace-post-publish-parent-identity-drift"
            }
            Self::UnsupportedPlatform => "object-bound-replace-unsupported-platform",
        }
    }
}

/// Atomically replace one private record while keeping directory mutation authority bound to one
/// opened directory object. On Unix the destination directory must already exist and must not be
/// group/other writable. The requested mode may contain owner bits only; group/other permissions
/// fail closed. The staging file is create-new/no-follow, synced through its opened descriptor,
/// revalidated against its directory entry immediately before `renameat`, and checked again at the
/// final name after publication. Error cleanup invalidates only the exact opened staging file and
/// never unlinks a possibly replaced staging pathname.
///
/// POSIX `renameat` still identifies its source by directory-relative name. The pre/post identity
/// checks therefore detect known substitution windows but do not claim a source-handle-conditioned
/// rename primitive. Callers requiring that stronger property must remain fail closed until their
/// platform provides an accepted primitive. Windows currently fails closed entirely.
pub(crate) fn replace_object_bound_bytes(
    path: &Path,
    encoded: &[u8],
    unix_mode: u32,
) -> Result<(), ObjectBoundReplaceError> {
    #[cfg(unix)]
    {
        replace_object_bound_bytes_with_hooks(path, encoded, unix_mode, || {}, || {}, || {})
    }

    #[cfg(not(unix))]
    {
        let _ = (path, encoded, unix_mode);
        Err(ObjectBoundReplaceError::UnsupportedPlatform)
    }
}

#[cfg(unix)]
fn revalidate_parent(
    directory: &std::fs::File,
    parent: &Path,
    expected_dev: u64,
    expected_ino: u64,
) -> Result<(), ObjectBoundReplaceError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let opened = directory
        .metadata()
        .map_err(|_| ObjectBoundReplaceError::ParentUnavailable)?;
    if !opened.is_dir() || opened.file_type().is_symlink() {
        return Err(ObjectBoundReplaceError::ParentUnsafe);
    }
    if opened.permissions().mode() & 0o022 != 0 {
        return Err(ObjectBoundReplaceError::ParentWritableByOthers);
    }

    let named = std::fs::symlink_metadata(parent)
        .map_err(|_| ObjectBoundReplaceError::ParentIdentityDrift)?;
    if !named.is_dir()
        || named.file_type().is_symlink()
        || named.dev() != expected_dev
        || named.ino() != expected_ino
    {
        return Err(ObjectBoundReplaceError::ParentIdentityDrift);
    }
    if named.permissions().mode() & 0o022 != 0 {
        return Err(ObjectBoundReplaceError::ParentWritableByOthers);
    }
    Ok(())
}

#[cfg(unix)]
fn validate_target_at(
    directory: &std::fs::File,
    final_name: &std::ffi::CString,
) -> Result<(), ObjectBoundReplaceError> {
    use std::mem::MaybeUninit;
    use std::os::fd::AsRawFd;

    let mut stat = MaybeUninit::<libc::stat>::uninit();
    let result = unsafe {
        libc::fstatat(
            directory.as_raw_fd(),
            final_name.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if result == 0 {
        let stat = unsafe { stat.assume_init() };
        if stat.st_mode & libc::S_IFMT != libc::S_IFREG {
            return Err(ObjectBoundReplaceError::TargetUnsafe);
        }
        return Ok(());
    }

    if std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT) {
        Ok(())
    } else {
        Err(ObjectBoundReplaceError::TargetUnavailable)
    }
}

#[cfg(unix)]
fn revalidate_opened_file_at(
    directory: &std::fs::File,
    name: &std::ffi::CString,
    opened_file: &std::fs::File,
    unix_mode: u32,
    drift: ObjectBoundReplaceError,
) -> Result<(), ObjectBoundReplaceError> {
    use std::mem::MaybeUninit;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let opened = opened_file.metadata().map_err(|_| drift)?;
    if !opened.is_file()
        || opened.file_type().is_symlink()
        || opened.permissions().mode() & 0o777 != unix_mode
    {
        return Err(drift);
    }

    let mut stat = MaybeUninit::<libc::stat>::uninit();
    let result = unsafe {
        libc::fstatat(
            directory.as_raw_fd(),
            name.as_ptr(),
            stat.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if result != 0 {
        return Err(drift);
    }
    let visible = unsafe { stat.assume_init() };
    if visible.st_mode & libc::S_IFMT != libc::S_IFREG
        || visible.st_dev as u64 != opened.dev()
        || visible.st_ino as u64 != opened.ino()
        || visible.st_mode as u32 & 0o777 != unix_mode
    {
        return Err(drift);
    }
    Ok(())
}

#[cfg(unix)]
fn invalidate_opened_temporary(
    temporary: &std::fs::File,
    directory: &std::fs::File,
) -> Result<(), ObjectBoundReplaceError> {
    temporary
        .set_len(0)
        .and_then(|_| temporary.sync_all())
        .and_then(|_| directory.sync_all())
        .map_err(|_| ObjectBoundReplaceError::CleanupFailed)
}

#[cfg(unix)]
fn open_temporary_at(
    directory: &std::fs::File,
    unix_mode: u32,
) -> Result<(std::ffi::CString, std::fs::File), ObjectBoundReplaceError> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMPORARY_ID: AtomicU64 = AtomicU64::new(1);

    for _ in 0..64 {
        let id = NEXT_TEMPORARY_ID.fetch_add(1, Ordering::Relaxed);
        let temporary_name = std::ffi::CString::new(format!(
            ".disksage-private-replace-{}-{id}.tmp",
            std::process::id()
        ))
        .map_err(|_| ObjectBoundReplaceError::NameInvalid)?;
        let fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                temporary_name.as_ptr(),
                libc::O_WRONLY
                    | libc::O_CREAT
                    | libc::O_EXCL
                    | libc::O_CLOEXEC
                    | libc::O_NOFOLLOW,
                unix_mode as libc::c_uint,
            )
        };
        if fd >= 0 {
            return Ok((temporary_name, unsafe { std::fs::File::from_raw_fd(fd) }));
        }
        if std::io::Error::last_os_error().raw_os_error() != Some(libc::EEXIST) {
            return Err(ObjectBoundReplaceError::TemporaryCreateFailed);
        }
    }

    Err(ObjectBoundReplaceError::TemporaryCreateFailed)
}

#[cfg(unix)]
fn replace_object_bound_bytes_with_hooks<F, G, H>(
    path: &Path,
    encoded: &[u8],
    unix_mode: u32,
    before_temporary_create: F,
    after_temporary_sync: G,
    after_rename_before_directory_sync: H,
) -> Result<(), ObjectBoundReplaceError>
where
    F: FnOnce(),
    G: FnOnce(),
    H: FnOnce(),
{
    use std::ffi::CString;
    use std::io::Write;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    if unix_mode & !0o777 != 0 || unix_mode & 0o077 != 0 {
        return Err(ObjectBoundReplaceError::ModeInvalid);
    }

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or(ObjectBoundReplaceError::ParentMissing)?;
    let final_name = path
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or(ObjectBoundReplaceError::NameInvalid)?;
    let final_name = CString::new(final_name.as_bytes())
        .map_err(|_| ObjectBoundReplaceError::NameInvalid)?;

    let parent_metadata = std::fs::symlink_metadata(parent)
        .map_err(|_| ObjectBoundReplaceError::ParentUnavailable)?;
    if !parent_metadata.is_dir() || parent_metadata.file_type().is_symlink() {
        return Err(ObjectBoundReplaceError::ParentUnsafe);
    }
    if parent_metadata.permissions().mode() & 0o022 != 0 {
        return Err(ObjectBoundReplaceError::ParentWritableByOthers);
    }
    let expected_dev = parent_metadata.dev();
    let expected_ino = parent_metadata.ino();

    let parent_c = CString::new(parent.as_os_str().as_bytes())
        .map_err(|_| ObjectBoundReplaceError::ParentUnavailable)?;
    let directory_fd = unsafe {
        libc::open(
            parent_c.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        )
    };
    if directory_fd < 0 {
        return Err(ObjectBoundReplaceError::ParentIdentityDrift);
    }
    let directory = unsafe { std::fs::File::from_raw_fd(directory_fd) };
    let opened = directory
        .metadata()
        .map_err(|_| ObjectBoundReplaceError::ParentUnavailable)?;
    if opened.dev() != expected_dev || opened.ino() != expected_ino {
        return Err(ObjectBoundReplaceError::ParentIdentityDrift);
    }
    revalidate_parent(&directory, parent, expected_dev, expected_ino)?;
    validate_target_at(&directory, &final_name)?;

    before_temporary_create();
    revalidate_parent(&directory, parent, expected_dev, expected_ino)?;

    let (temporary_name, mut temporary) = open_temporary_at(&directory, unix_mode)?;
    let before_publication = (|| -> Result<(), ObjectBoundReplaceError> {
        temporary
            .set_permissions(std::fs::Permissions::from_mode(unix_mode))
            .map_err(|_| ObjectBoundReplaceError::ModeInvalid)?;
        temporary
            .write_all(encoded)
            .and_then(|_| temporary.sync_all())
            .map_err(|_| ObjectBoundReplaceError::WriteFailed)?;
        let metadata = temporary
            .metadata()
            .map_err(|_| ObjectBoundReplaceError::WriteFailed)?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.permissions().mode() & 0o777 != unix_mode
        {
            return Err(ObjectBoundReplaceError::ModeInvalid);
        }

        after_temporary_sync();
        revalidate_parent(&directory, parent, expected_dev, expected_ino)?;
        revalidate_opened_file_at(
            &directory,
            &temporary_name,
            &temporary,
            unix_mode,
            ObjectBoundReplaceError::TemporaryIdentityDrift,
        )?;
        validate_target_at(&directory, &final_name)?;
        Ok(())
    })();

    if let Err(error) = before_publication {
        invalidate_opened_temporary(&temporary, &directory)?;
        return Err(error);
    }

    revalidate_opened_file_at(
        &directory,
        &temporary_name,
        &temporary,
        unix_mode,
        ObjectBoundReplaceError::TemporaryIdentityDrift,
    )?;
    let rename_result = unsafe {
        libc::renameat(
            directory.as_raw_fd(),
            temporary_name.as_ptr(),
            directory.as_raw_fd(),
            final_name.as_ptr(),
        )
    };
    if rename_result != 0 {
        invalidate_opened_temporary(&temporary, &directory)?;
        return Err(ObjectBoundReplaceError::RenameFailed);
    }

    revalidate_opened_file_at(
        &directory,
        &final_name,
        &temporary,
        unix_mode,
        ObjectBoundReplaceError::PostPublishTemporaryIdentityDrift,
    )?;

    after_rename_before_directory_sync();
    directory
        .sync_all()
        .map_err(|_| ObjectBoundReplaceError::DirectorySyncFailed)?;

    if revalidate_parent(&directory, parent, expected_dev, expected_ino).is_err() {
        return Err(ObjectBoundReplaceError::PostPublishParentIdentityDrift);
    }

    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn private_parent(root: &tempfile::TempDir) -> std::path::PathBuf {
        let parent = root.path().join("private");
        std::fs::create_dir(&parent).expect("create private parent");
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700))
            .expect("set private parent mode");
        parent
    }

    #[test]
    fn atomically_replaces_regular_record_and_leaves_no_staging_name() {
        let root = tempfile::tempdir().expect("tempdir");
        let parent = private_parent(&root);
        let record = parent.join("connections.json");
        std::fs::write(&record, b"old").expect("seed record");

        replace_object_bound_bytes(&record, b"new", 0o600).expect("replace record");

        assert_eq!(std::fs::read(&record).expect("read record"), b"new");
        let names = std::fs::read_dir(&parent)
            .expect("read parent")
            .map(|entry| entry.expect("entry").file_name())
            .collect::<Vec<_>>();
        assert_eq!(names, vec![std::ffi::OsString::from("connections.json")]);
    }

    #[test]
    fn parent_replacement_before_temporary_create_fails_without_redirecting_bytes() {
        let root = tempfile::tempdir().expect("tempdir");
        let parent = private_parent(&root);
        let moved = root.path().join("private-moved");
        let record = parent.join("connections.json");
        std::fs::write(&record, b"old").expect("seed record");

        let error = replace_object_bound_bytes_with_hooks(
            &record,
            b"new",
            0o600,
            || {
                std::fs::rename(&parent, &moved).expect("move admitted parent");
                std::fs::create_dir(&parent).expect("install replacement parent");
                std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700))
                    .expect("set replacement mode");
            },
            || {},
            || {},
        )
        .expect_err("namespace drift must fail closed");

        assert_eq!(error, ObjectBoundReplaceError::ParentIdentityDrift);
        assert!(!parent.join("connections.json").exists());
        assert_eq!(std::fs::read(moved.join("connections.json")).expect("old record"), b"old");
    }

    #[test]
    fn parent_replacement_after_temporary_sync_invalidates_pinned_staging_without_redirecting_bytes() {
        let root = tempfile::tempdir().expect("tempdir");
        let parent = private_parent(&root);
        let moved = root.path().join("private-moved");
        let record = parent.join("connections.json");
        std::fs::write(&record, b"old").expect("seed record");

        let error = replace_object_bound_bytes_with_hooks(
            &record,
            b"new",
            0o600,
            || {},
            || {
                std::fs::rename(&parent, &moved).expect("move admitted parent");
                std::fs::create_dir(&parent).expect("install replacement parent");
                std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700))
                    .expect("set replacement mode");
            },
            || {},
        )
        .expect_err("namespace drift must fail closed");

        assert_eq!(error, ObjectBoundReplaceError::ParentIdentityDrift);
        assert!(!parent.join("connections.json").exists());
        assert_eq!(std::fs::read(moved.join("connections.json")).expect("old record"), b"old");
        let staging = std::fs::read_dir(&moved)
            .expect("read moved parent")
            .map(|entry| entry.expect("entry").path())
            .find(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().starts_with(".disksage-private-replace-"))
                    .unwrap_or(false)
            })
            .expect("invalidated staging record");
        assert_eq!(std::fs::metadata(staging).expect("staging metadata").len(), 0);
    }

    #[test]
    fn temporary_name_replacement_after_sync_never_publishes_replacement_bytes() {
        let root = tempfile::tempdir().expect("tempdir");
        let parent = private_parent(&root);
        let record = parent.join("connections.json");
        std::fs::write(&record, b"old").expect("seed record");
        let hook_parent = parent.clone();
        let replacement_path = std::sync::Arc::new(std::sync::Mutex::new(None));
        let hook_replacement_path = std::sync::Arc::clone(&replacement_path);

        let error = replace_object_bound_bytes_with_hooks(
            &record,
            b"authorized",
            0o600,
            || {},
            move || {
                let staging_name = std::fs::read_dir(&hook_parent)
                    .expect("read private parent")
                    .map(|entry| entry.expect("entry").file_name())
                    .find(|name| {
                        let name = name.to_string_lossy();
                        name.starts_with(".disksage-private-replace-") && name.ends_with(".tmp")
                    })
                    .expect("staging record");
                let staging_path = hook_parent.join(staging_name);
                std::fs::remove_file(&staging_path).expect("remove admitted staging name");
                std::fs::write(&staging_path, b"attacker").expect("install replacement staging");
                std::fs::set_permissions(
                    &staging_path,
                    std::fs::Permissions::from_mode(0o600),
                )
                .expect("set replacement staging mode");
                *hook_replacement_path.lock().expect("replacement path lock") =
                    Some(staging_path);
            },
            || {},
        )
        .expect_err("temporary pathname replacement must fail closed");

        assert_eq!(
            error.code(),
            "object-bound-replace-temporary-identity-drift"
        );
        assert_eq!(std::fs::read(&record).expect("old record"), b"old");
        let replacement_path = replacement_path
            .lock()
            .expect("replacement path lock")
            .clone()
            .expect("replacement path");
        assert_eq!(
            std::fs::read(replacement_path).expect("replacement staging"),
            b"attacker"
        );
    }

    #[test]
    fn post_rename_parent_replacement_reports_uncertain_path_without_touching_replacement() {
        let root = tempfile::tempdir().expect("tempdir");
        let parent = private_parent(&root);
        let moved = root.path().join("private-moved");
        let record = parent.join("connections.json");
        std::fs::write(&record, b"old").expect("seed record");

        let error = replace_object_bound_bytes_with_hooks(
            &record,
            b"new",
            0o600,
            || {},
            || {},
            || {
                std::fs::rename(&parent, &moved).expect("move admitted parent after rename");
                std::fs::create_dir(&parent).expect("install replacement parent");
                std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700))
                    .expect("set replacement mode");
            },
        )
        .expect_err("post-publication namespace drift must be observable");

        assert_eq!(error, ObjectBoundReplaceError::PostPublishParentIdentityDrift);
        assert!(!parent.join("connections.json").exists());
        assert_eq!(std::fs::read(moved.join("connections.json")).expect("pinned record"), b"new");
    }

    #[test]
    fn rejects_non_regular_existing_target_without_following_it() {
        let root = tempfile::tempdir().expect("tempdir");
        let parent = private_parent(&root);
        let record = parent.join("connections.json");
        std::fs::create_dir(&record).expect("create unsafe target");

        let error = replace_object_bound_bytes(&record, b"new", 0o600)
            .expect_err("directory target must fail closed");
        assert_eq!(error, ObjectBoundReplaceError::TargetUnsafe);
        assert!(record.is_dir());
    }

    #[test]
    fn stable_error_codes_cover_durability_and_platform_boundaries() {
        assert_eq!(
            ObjectBoundReplaceError::DirectorySyncFailed.code(),
            "object-bound-replace-directory-sync-failed"
        );
        assert_eq!(
            ObjectBoundReplaceError::TemporaryIdentityDrift.code(),
            "object-bound-replace-temporary-identity-drift"
        );
        assert_eq!(
            ObjectBoundReplaceError::PostPublishTemporaryIdentityDrift.code(),
            "object-bound-replace-post-publish-temporary-identity-drift"
        );
        assert_eq!(
            ObjectBoundReplaceError::UnsupportedPlatform.code(),
            "object-bound-replace-unsupported-platform"
        );
    }
}
