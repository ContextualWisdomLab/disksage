//! Object-bound atomic replacement for private local records.
//!
//! The Unix implementation keeps temporary creation, validation, rename, cleanup, and directory
//! durability relative to one admitted directory descriptor. Pathname revalidation is used to
//! detect namespace drift; it is never used as the mutation authority after the directory is open.

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
    ModeInvalid,
    WriteFailed,
    CleanupFailed,
    RenameFailed,
    DirectorySyncFailed,
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
            Self::ModeInvalid => "object-bound-replace-mode-invalid",
            Self::WriteFailed => "object-bound-replace-write-failed",
            Self::CleanupFailed => "object-bound-replace-cleanup-failed",
            Self::RenameFailed => "object-bound-replace-rename-failed",
            Self::DirectorySyncFailed => "object-bound-replace-directory-sync-failed",
            Self::PostPublishParentIdentityDrift => {
                "object-bound-replace-post-publish-parent-identity-drift"
            }
            Self::UnsupportedPlatform => "object-bound-replace-unsupported-platform",
        }
    }
}

/// Atomically replace one private record while keeping mutation authority bound to one directory
/// object. On Unix the destination directory must already exist and must not be group/other
/// writable. The requested mode may contain owner bits only; group/other permissions fail closed.
/// The replacement file is created with `O_EXCL|O_NOFOLLOW`, normalized to `unix_mode`, synced,
/// renamed with `renameat`, and followed by a directory `fsync`.
///
/// Windows currently fails closed because an equivalent handle-relative temporary-create and
/// replace primitive has not yet been implemented. Callers must not fall back to pathname writes.
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
fn unlink_temporary_at(
    directory: &std::fs::File,
    temporary_name: &std::ffi::CString,
) -> Result<(), ObjectBoundReplaceError> {
    use std::os::fd::AsRawFd;

    let result = unsafe { libc::unlinkat(directory.as_raw_fd(), temporary_name.as_ptr(), 0) };
    if result == 0 {
        Ok(())
    } else {
        Err(ObjectBoundReplaceError::CleanupFailed)
    }
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
        validate_target_at(&directory, &final_name)?;
        Ok(())
    })();

    if let Err(error) = before_publication {
        unlink_temporary_at(&directory, &temporary_name)?;
        directory
            .sync_all()
            .map_err(|_| ObjectBoundReplaceError::DirectorySyncFailed)?;
        return Err(error);
    }

    let rename_result = unsafe {
        libc::renameat(
            directory.as_raw_fd(),
            temporary_name.as_ptr(),
            directory.as_raw_fd(),
            final_name.as_ptr(),
        )
    };
    if rename_result != 0 {
        unlink_temporary_at(&directory, &temporary_name)?;
        directory
            .sync_all()
            .map_err(|_| ObjectBoundReplaceError::DirectorySyncFailed)?;
        return Err(ObjectBoundReplaceError::RenameFailed);
    }

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
    fn parent_replacement_after_temporary_sync_cleans_pinned_staging_without_redirecting_bytes() {
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
        assert_eq!(std::fs::read_dir(&moved).expect("read moved parent").count(), 1);
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
            ObjectBoundReplaceError::UnsupportedPlatform.code(),
            "object-bound-replace-unsupported-platform"
        );
    }
}
