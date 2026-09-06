use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::Path;

pub const MAX_PRIVATE_EVIDENCE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrivateEvidenceReceipt {
    pub written: bool,
    pub sha256: String,
    pub bytes: usize,
    pub unix_mode: String,
    pub create_new: bool,
    pub contains_sensitive_local_paths: bool,
    pub is_approval: bool,
}

#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ObjectBoundPublicationError {
    ParentMissing,
    ParentUnavailable,
    ParentUnsafe,
    ParentWritableByOthers,
    ParentIdentityDrift,
    ForbiddenRootInvalid,
    ForbiddenRootUnavailable,
    ForbiddenRootIdentityDrift,
    InsideForbiddenRoot,
    NameInvalid,
    CreateFailed,
    ModeInvalid,
    WriteFailed,
    MetadataFailed,
    ParentSyncFailed,
    RecordIdentityDrift,
    RecordContentDrift,
    InvalidationFailed,
}

#[cfg(unix)]
fn revalidate_private_parent(
    directory: &std::fs::File,
    canonical_parent: &Path,
    expected_dev: u64,
    expected_ino: u64,
) -> Result<(), ObjectBoundPublicationError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let opened = directory
        .metadata()
        .map_err(|_| ObjectBoundPublicationError::ParentUnavailable)?;
    if !opened.is_dir() || opened.file_type().is_symlink() {
        return Err(ObjectBoundPublicationError::ParentUnsafe);
    }
    if opened.permissions().mode() & 0o022 != 0 {
        return Err(ObjectBoundPublicationError::ParentWritableByOthers);
    }

    let named = std::fs::symlink_metadata(canonical_parent)
        .map_err(|_| ObjectBoundPublicationError::ParentIdentityDrift)?;
    if named.file_type().is_symlink()
        || !named.is_dir()
        || named.dev() != expected_dev
        || named.ino() != expected_ino
    {
        return Err(ObjectBoundPublicationError::ParentIdentityDrift);
    }
    if named.permissions().mode() & 0o022 != 0 {
        return Err(ObjectBoundPublicationError::ParentWritableByOthers);
    }
    Ok(())
}

#[cfg(unix)]
fn revalidate_forbidden_root(
    directory: &std::fs::File,
    canonical_root: &Path,
    expected_dev: u64,
    expected_ino: u64,
) -> Result<(), ObjectBoundPublicationError> {
    use std::os::unix::fs::MetadataExt;

    let opened = directory
        .metadata()
        .map_err(|_| ObjectBoundPublicationError::ForbiddenRootIdentityDrift)?;
    if !opened.is_dir()
        || opened.file_type().is_symlink()
        || opened.dev() != expected_dev
        || opened.ino() != expected_ino
    {
        return Err(ObjectBoundPublicationError::ForbiddenRootIdentityDrift);
    }

    let named = std::fs::symlink_metadata(canonical_root)
        .map_err(|_| ObjectBoundPublicationError::ForbiddenRootIdentityDrift)?;
    if named.file_type().is_symlink()
        || !named.is_dir()
        || named.dev() != expected_dev
        || named.ino() != expected_ino
    {
        return Err(ObjectBoundPublicationError::ForbiddenRootIdentityDrift);
    }
    Ok(())
}

#[cfg(unix)]
fn publication_error_string(error: ObjectBoundPublicationError) -> String {
    match error {
        ObjectBoundPublicationError::ParentMissing => "private-evidence-parent-missing",
        ObjectBoundPublicationError::ParentUnavailable => "private-evidence-parent-unavailable",
        ObjectBoundPublicationError::ParentUnsafe => "private-evidence-parent-unsafe",
        ObjectBoundPublicationError::ParentWritableByOthers => {
            "private-evidence-parent-writable-by-others"
        }
        ObjectBoundPublicationError::ParentIdentityDrift => {
            "private-evidence-parent-identity-drift"
        }
        ObjectBoundPublicationError::ForbiddenRootInvalid => {
            "private-evidence-source-root-invalid"
        }
        ObjectBoundPublicationError::ForbiddenRootUnavailable => {
            "private-evidence-source-root-unavailable"
        }
        ObjectBoundPublicationError::ForbiddenRootIdentityDrift => {
            "private-evidence-source-root-identity-drift"
        }
        ObjectBoundPublicationError::InsideForbiddenRoot => "private-evidence-inside-source-root",
        ObjectBoundPublicationError::NameInvalid => "private-evidence-name-invalid",
        ObjectBoundPublicationError::CreateFailed => "private-evidence-create-failed",
        ObjectBoundPublicationError::ModeInvalid => "private-evidence-mode-invalid",
        ObjectBoundPublicationError::WriteFailed => "private-evidence-write-failed",
        ObjectBoundPublicationError::MetadataFailed => "private-evidence-metadata-failed",
        ObjectBoundPublicationError::ParentSyncFailed => "private-evidence-parent-sync-failed",
        ObjectBoundPublicationError::RecordIdentityDrift => {
            "private-evidence-record-identity-drift"
        }
        ObjectBoundPublicationError::RecordContentDrift => {
            "private-evidence-record-content-drift"
        }
        ObjectBoundPublicationError::InvalidationFailed => {
            "private-evidence-invalidation-failed"
        }
    }
    .to_string()
}

/// Create and durably publish one immutable byte record relative to the exact private parent
/// directory object admitted by the caller-supplied absolute pathname.
///
/// The parent must already exist and must not be writable by group or other principals. Relative
/// destination or forbidden-root authority is rejected before hooks or filesystem lookup so
/// publication never depends on ambient process CWD. Directory-looking Unix destinations ending in
/// a raw `/` are rejected before hooks or lookup instead of being normalized into file authority.
/// The pathname is opened with `O_NOFOLLOW` before canonicalization and the opened directory is bound
/// to the device/inode observed during initial admission. Record creation is descriptor-relative and
/// create-new. `forbidden_root`, when present, is identity-admitted before any test seam, then
/// canonicalized and opened; the opened directory must retain that initial device/inode and is
/// revalidated before creation and finalization so source-root alias replacement cannot silently
/// retarget publication policy. Finalization also reopens the visible record descriptor-relative and
/// verifies exact identity, mode, length, and bytes before success.
#[cfg(unix)]
pub(crate) fn write_object_bound_bytes_create_new(
    path: &Path,
    encoded: &[u8],
    unix_mode: u32,
    forbidden_root: Option<&Path>,
) -> Result<(), ObjectBoundPublicationError> {
    write_object_bound_bytes_create_new_with_hooks(
        path,
        encoded,
        unix_mode,
        forbidden_root,
        || {},
        || {},
        || {},
    )
}

/// Internal deterministic seam for dependent authority writers to prove directory-replacement
/// handling while exercising the same descriptor-bound publication implementation used in production.
#[cfg(unix)]
pub(crate) fn write_object_bound_bytes_create_new_with_hooks<F, G, H>(
    path: &Path,
    encoded: &[u8],
    unix_mode: u32,
    forbidden_root: Option<&Path>,
    before_parent_open: F,
    before_create: G,
    before_finalize: H,
) -> Result<(), ObjectBoundPublicationError>
where
    F: FnOnce(),
    G: FnOnce(),
    H: FnOnce(),
{
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    if !matches!(unix_mode, 0o400 | 0o600) {
        return Err(ObjectBoundPublicationError::ModeInvalid);
    }
    if !path.is_absolute() || path.as_os_str().as_bytes().ends_with(b"/") {
        return Err(ObjectBoundPublicationError::NameInvalid);
    }
    if forbidden_root.is_some_and(|root| !root.is_absolute()) {
        return Err(ObjectBoundPublicationError::ForbiddenRootInvalid);
    }

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or(ObjectBoundPublicationError::ParentMissing)?;
    let parent_metadata = std::fs::symlink_metadata(parent)
        .map_err(|_| ObjectBoundPublicationError::ParentUnavailable)?;
    if !parent_metadata.is_dir() || parent_metadata.file_type().is_symlink() {
        return Err(ObjectBoundPublicationError::ParentUnsafe);
    }
    if parent_metadata.permissions().mode() & 0o022 != 0 {
        return Err(ObjectBoundPublicationError::ParentWritableByOthers);
    }
    let expected_parent_dev = parent_metadata.dev();
    let expected_parent_ino = parent_metadata.ino();

    let expected_forbidden_identity = if let Some(forbidden_root) = forbidden_root {
        let metadata = std::fs::metadata(forbidden_root)
            .map_err(|_| ObjectBoundPublicationError::ForbiddenRootUnavailable)?;
        if !metadata.is_dir() {
            return Err(ObjectBoundPublicationError::ForbiddenRootIdentityDrift);
        }
        Some((metadata.dev(), metadata.ino()))
    } else {
        None
    };

    before_parent_open();

    let parent_c = CString::new(parent.as_os_str().as_bytes())
        .map_err(|_| ObjectBoundPublicationError::ParentUnavailable)?;
    let directory_fd = unsafe {
        libc::open(
            parent_c.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        )
    };
    if directory_fd < 0 {
        return Err(ObjectBoundPublicationError::ParentIdentityDrift);
    }
    let directory = unsafe { std::fs::File::from_raw_fd(directory_fd) };
    let opened_parent_metadata = directory
        .metadata()
        .map_err(|_| ObjectBoundPublicationError::ParentUnavailable)?;
    if opened_parent_metadata.dev() != expected_parent_dev
        || opened_parent_metadata.ino() != expected_parent_ino
    {
        return Err(ObjectBoundPublicationError::ParentIdentityDrift);
    }

    let canonical_parent = std::fs::canonicalize(parent)
        .map_err(|_| ObjectBoundPublicationError::ParentIdentityDrift)?;
    revalidate_private_parent(
        &directory,
        &canonical_parent,
        expected_parent_dev,
        expected_parent_ino,
    )?;

    let forbidden_authority = if let Some(forbidden_root) = forbidden_root {
        let (expected_forbidden_dev, expected_forbidden_ino) = expected_forbidden_identity
            .expect("forbidden-root identity must be admitted when policy is present");
        let canonical_forbidden = std::fs::canonicalize(forbidden_root)
            .map_err(|_| ObjectBoundPublicationError::ForbiddenRootIdentityDrift)?;
        let forbidden_c = CString::new(canonical_forbidden.as_os_str().as_bytes())
            .map_err(|_| ObjectBoundPublicationError::ForbiddenRootUnavailable)?;
        let forbidden_fd = unsafe {
            libc::open(
                forbidden_c.as_ptr(),
                libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
            )
        };
        if forbidden_fd < 0 {
            return Err(ObjectBoundPublicationError::ForbiddenRootIdentityDrift);
        }
        let forbidden_directory = unsafe { std::fs::File::from_raw_fd(forbidden_fd) };
        let forbidden_metadata = forbidden_directory
            .metadata()
            .map_err(|_| ObjectBoundPublicationError::ForbiddenRootIdentityDrift)?;
        if !forbidden_metadata.is_dir()
            || forbidden_metadata.file_type().is_symlink()
            || forbidden_metadata.dev() != expected_forbidden_dev
            || forbidden_metadata.ino() != expected_forbidden_ino
        {
            return Err(ObjectBoundPublicationError::ForbiddenRootIdentityDrift);
        }
        revalidate_forbidden_root(
            &forbidden_directory,
            &canonical_forbidden,
            expected_forbidden_dev,
            expected_forbidden_ino,
        )?;
        if canonical_parent.starts_with(&canonical_forbidden) {
            return Err(ObjectBoundPublicationError::InsideForbiddenRoot);
        }
        Some((
            forbidden_directory,
            canonical_forbidden,
            expected_forbidden_dev,
            expected_forbidden_ino,
        ))
    } else {
        None
    };

    let file_name = path
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or(ObjectBoundPublicationError::NameInvalid)?;
    let file_name_c = CString::new(file_name.as_bytes())
        .map_err(|_| ObjectBoundPublicationError::NameInvalid)?;

    before_create();
    revalidate_private_parent(
        &directory,
        &canonical_parent,
        expected_parent_dev,
        expected_parent_ino,
    )?;
    if let Some((forbidden_directory, canonical_forbidden, forbidden_dev, forbidden_ino)) =
        forbidden_authority.as_ref()
    {
        revalidate_forbidden_root(
            forbidden_directory,
            canonical_forbidden,
            *forbidden_dev,
            *forbidden_ino,
        )?;
    }

    let file_fd = unsafe {
        libc::openat(
            directory.as_raw_fd(),
            file_name_c.as_ptr(),
            libc::O_WRONLY
                | libc::O_CREAT
                | libc::O_EXCL
                | libc::O_CLOEXEC
                | libc::O_NOFOLLOW,
            unix_mode as libc::c_uint,
        )
    };
    if file_fd < 0 {
        return Err(ObjectBoundPublicationError::CreateFailed);
    }
    let mut file = unsafe { std::fs::File::from_raw_fd(file_fd) };
    let publication = (|| -> Result<(), ObjectBoundPublicationError> {
        file.set_permissions(std::fs::Permissions::from_mode(unix_mode))
            .map_err(|_| ObjectBoundPublicationError::ModeInvalid)?;
        file.write_all(encoded)
            .and_then(|_| file.sync_all())
            .map_err(|_| ObjectBoundPublicationError::WriteFailed)?;
        let opened_file_metadata = file
            .metadata()
            .map_err(|_| ObjectBoundPublicationError::MetadataFailed)?;
        if !opened_file_metadata.is_file()
            || opened_file_metadata.file_type().is_symlink()
            || opened_file_metadata.permissions().mode() & 0o7777 != unix_mode
        {
            return Err(ObjectBoundPublicationError::ModeInvalid);
        }
        directory
            .sync_all()
            .map_err(|_| ObjectBoundPublicationError::ParentSyncFailed)?;

        before_finalize();

        revalidate_private_parent(
            &directory,
            &canonical_parent,
            expected_parent_dev,
            expected_parent_ino,
        )?;
        if let Some((forbidden_directory, canonical_forbidden, forbidden_dev, forbidden_ino)) =
            forbidden_authority.as_ref()
        {
            revalidate_forbidden_root(
                forbidden_directory,
                canonical_forbidden,
                *forbidden_dev,
                *forbidden_ino,
            )?;
        }

        let visible_fd = unsafe {
            libc::openat(
                directory.as_raw_fd(),
                file_name_c.as_ptr(),
                libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
            )
        };
        if visible_fd < 0 {
            return Err(ObjectBoundPublicationError::RecordIdentityDrift);
        }
        let mut visible = unsafe { std::fs::File::from_raw_fd(visible_fd) };
        let final_file_metadata = visible
            .metadata()
            .map_err(|_| ObjectBoundPublicationError::RecordIdentityDrift)?;
        if final_file_metadata.file_type().is_symlink()
            || !final_file_metadata.is_file()
            || final_file_metadata.dev() != opened_file_metadata.dev()
            || final_file_metadata.ino() != opened_file_metadata.ino()
        {
            return Err(ObjectBoundPublicationError::RecordIdentityDrift);
        }
        if final_file_metadata.permissions().mode() & 0o7777 != unix_mode {
            return Err(ObjectBoundPublicationError::ModeInvalid);
        }
        if final_file_metadata.len() != encoded.len() as u64 {
            return Err(ObjectBoundPublicationError::RecordContentDrift);
        }
        let mut final_bytes = Vec::with_capacity(encoded.len());
        Read::by_ref(&mut visible)
            .take((encoded.len() as u64).saturating_add(1))
            .read_to_end(&mut final_bytes)
            .map_err(|_| ObjectBoundPublicationError::RecordContentDrift)?;
        if final_bytes != encoded {
            return Err(ObjectBoundPublicationError::RecordContentDrift);
        }
        Ok(())
    })();

    if let Err(error) = publication {
        let invalidation = file
            .set_len(0)
            .and_then(|_| file.set_permissions(std::fs::Permissions::from_mode(unix_mode)))
            .and_then(|_| file.sync_all())
            .and_then(|_| directory.sync_all());
        if invalidation.is_err() {
            return Err(ObjectBoundPublicationError::InvalidationFailed);
        }
        return Err(error);
    }
    Ok(())
}

/// Persist exact local evidence outside the audited source tree.
///
/// The destination parent must already exist, must not be a symlink, and must not be writable by
/// group or other principals. On Unix, publication is bound to the exact caller-supplied parent
/// directory object admitted before canonicalization, so a same-user pathname replacement cannot
/// redirect either canonicalization or the later write. The forbidden source root is likewise bound
/// to an opened directory object for the publication lifetime and must be absolute so policy does
/// not depend on ambient process CWD. The file is created once with mode 0600, synced, and never
/// overwritten. Finalization reopens the visible record descriptor-relative and verifies exact
/// identity, mode, length, and bytes. After a post-create failure, the still-open record is
/// truncated, restored to the requested private mode, and synced through its descriptor. The
/// pathname is deliberately not unlinked because a same-user process may already have replaced that
/// name; this can leave a zero-length mode-0600 create-new tombstone that requires explicit operator
/// cleanup.
#[cfg(unix)]
pub fn write_private_json_create_new(
    source_root: &Path,
    path: &Path,
    value: &impl Serialize,
) -> Result<PrivateEvidenceReceipt, String> {
    write_private_json_create_new_unix_with_hooks(source_root, path, value, || {}, || {}, || {})
}

#[cfg(unix)]
fn write_private_json_create_new_unix_with_hooks<F, G, H>(
    source_root: &Path,
    path: &Path,
    value: &impl Serialize,
    before_parent_open: F,
    before_create: G,
    before_finalize: H,
) -> Result<PrivateEvidenceReceipt, String>
where
    F: FnOnce(),
    G: FnOnce(),
    H: FnOnce(),
{
    let encoded = serde_json::to_vec_pretty(value)
        .map_err(|_| "private-evidence-json-invalid".to_string())?;
    if encoded.len() > MAX_PRIVATE_EVIDENCE_BYTES {
        return Err("private-evidence-too-large".into());
    }

    write_object_bound_bytes_create_new_with_hooks(
        path,
        &encoded,
        0o600,
        Some(source_root),
        before_parent_open,
        before_create,
        before_finalize,
    )
    .map_err(publication_error_string)?;

    let sha256 = Sha256::digest(&encoded)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    Ok(PrivateEvidenceReceipt {
        written: true,
        sha256,
        bytes: encoded.len(),
        unix_mode: "0600".into(),
        create_new: true,
        contains_sensitive_local_paths: true,
        is_approval: false,
    })
}

#[cfg(not(unix))]
pub fn write_private_json_create_new(
    _source_root: &Path,
    _path: &Path,
    _value: &impl Serialize,
) -> Result<PrivateEvidenceReceipt, String> {
    Err("private-evidence-secure-mode-unsupported".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn creates_once_with_mode_0600_outside_source() {
        use std::os::unix::fs::PermissionsExt;

        let source = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let path = private.path().join("audit.json");
        let value = serde_json::json!({"private": true});
        let receipt = write_private_json_create_new(source.path(), &path, &value).unwrap();
        assert_eq!(receipt.sha256.len(), 64);
        assert!(receipt.bytes > 0);
        assert_eq!(receipt.unix_mode, "0600");
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(write_private_json_create_new(source.path(), &path, &value).is_err());
        assert!(write_private_json_create_new(
            source.path(),
            &source.path().join("inside.json"),
            &value
        )
        .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_parent_writable_by_other_principals() {
        use std::os::unix::fs::PermissionsExt;

        let source = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        let mut permissions = std::fs::metadata(private.path()).unwrap().permissions();
        permissions.set_mode(0o777);
        std::fs::set_permissions(private.path(), permissions).unwrap();

        let path = private.path().join("audit.json");
        let error = write_private_json_create_new(
            source.path(),
            &path,
            &serde_json::json!({"private": true}),
        )
        .unwrap_err();

        assert_eq!(error, "private-evidence-parent-writable-by-others");
        assert!(!path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn fails_closed_if_parent_is_replaced_before_open() {
        use std::os::unix::fs::PermissionsExt;

        let source = tempfile::tempdir().unwrap();
        let fixture = tempfile::tempdir().unwrap();
        let parent = fixture.path().join("records");
        let moved_parent = fixture.path().join("authorized-records-moved");
        std::fs::create_dir(&parent).unwrap();
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = parent.join("audit.json");
        let parent_for_hook = parent.clone();
        let moved_for_hook = moved_parent.clone();

        let error = write_private_json_create_new_unix_with_hooks(
            source.path(),
            &path,
            &serde_json::json!({"private": true}),
            move || {
                std::fs::rename(&parent_for_hook, &moved_for_hook).unwrap();
                std::fs::create_dir(&parent_for_hook).unwrap();
                std::fs::set_permissions(
                    &parent_for_hook,
                    std::fs::Permissions::from_mode(0o700),
                )
                .unwrap();
            },
            || {},
            || {},
        )
        .unwrap_err();

        assert_eq!(error, "private-evidence-parent-identity-drift");
        assert!(!parent.join("audit.json").exists());
        assert!(!moved_parent.join("audit.json").exists());
    }

    #[cfg(unix)]
    #[test]
    fn fails_closed_if_parent_becomes_shared_writable_after_authorization() {
        use std::os::unix::fs::PermissionsExt;

        let source = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        std::fs::set_permissions(private.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = private.path().join("audit.json");
        let parent_for_hook = private.path().to_path_buf();

        let error = write_private_json_create_new_unix_with_hooks(
            source.path(),
            &path,
            &serde_json::json!({"private": true}),
            || {},
            move || {
                std::fs::set_permissions(
                    &parent_for_hook,
                    std::fs::Permissions::from_mode(0o770),
                )
                .unwrap();
            },
            || {},
        )
        .unwrap_err();

        assert_eq!(error, "private-evidence-parent-writable-by-others");
        assert!(!path.exists());
    }

    #[cfg(unix)]
    #[test]
    fn failed_post_create_validation_leaves_private_zero_length_tombstone() {
        use std::os::unix::fs::PermissionsExt;

        let source = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        std::fs::set_permissions(private.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = private.path().join("audit.json");
        let parent_for_hook = private.path().to_path_buf();

        let error = write_private_json_create_new_unix_with_hooks(
            source.path(),
            &path,
            &serde_json::json!({"private": true}),
            || {},
            || {},
            move || {
                std::fs::set_permissions(
                    &parent_for_hook,
                    std::fs::Permissions::from_mode(0o770),
                )
                .unwrap();
            },
        )
        .unwrap_err();

        assert_eq!(error, "private-evidence-parent-writable-by-others");
        let metadata = std::fs::metadata(&path).unwrap();
        assert_eq!(metadata.len(), 0);
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn fails_closed_if_private_parent_is_replaced_after_authorization() {
        use std::os::unix::fs::PermissionsExt;

        let source = tempfile::tempdir().unwrap();
        let fixture = tempfile::tempdir().unwrap();
        let parent = fixture.path().join("records");
        let moved_parent = fixture.path().join("authorized-records-moved");
        std::fs::create_dir(&parent).unwrap();
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = parent.join("audit.json");
        let replacement_parent = parent.clone();
        let parent_for_hook = parent.clone();
        let moved_for_hook = moved_parent.clone();

        let error = write_private_json_create_new_unix_with_hooks(
            source.path(),
            &path,
            &serde_json::json!({"private": true}),
            || {},
            move || {
                std::fs::rename(&parent_for_hook, &moved_for_hook).unwrap();
                std::fs::create_dir(&parent_for_hook).unwrap();
                std::fs::set_permissions(
                    &parent_for_hook,
                    std::fs::Permissions::from_mode(0o700),
                )
                .unwrap();
            },
            || {},
        )
        .unwrap_err();

        assert_eq!(error, "private-evidence-parent-identity-drift");
        assert!(!replacement_parent.join("audit.json").exists());
        assert!(!moved_parent.join("audit.json").exists());
    }

    #[cfg(unix)]
    #[test]
    fn post_create_parent_replacement_invalidates_only_authorized_record() {
        use std::os::unix::fs::PermissionsExt;

        let source = tempfile::tempdir().unwrap();
        let fixture = tempfile::tempdir().unwrap();
        let parent = fixture.path().join("records");
        let moved_parent = fixture.path().join("authorized-records-moved");
        std::fs::create_dir(&parent).unwrap();
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = parent.join("audit.json");
        let replacement_parent = parent.clone();
        let parent_for_hook = parent.clone();
        let moved_for_hook = moved_parent.clone();

        let error = write_private_json_create_new_unix_with_hooks(
            source.path(),
            &path,
            &serde_json::json!({"private": true}),
            || {},
            || {},
            move || {
                std::fs::rename(&parent_for_hook, &moved_for_hook).unwrap();
                std::fs::create_dir(&parent_for_hook).unwrap();
                std::fs::set_permissions(
                    &parent_for_hook,
                    std::fs::Permissions::from_mode(0o700),
                )
                .unwrap();
            },
        )
        .unwrap_err();

        assert_eq!(error, "private-evidence-parent-identity-drift");
        assert!(
            !replacement_parent.join("audit.json").exists(),
            "replacement directory must never receive the authorized record"
        );
        let tombstone = moved_parent.join("audit.json");
        let metadata = std::fs::metadata(&tombstone).unwrap();
        assert_eq!(metadata.len(), 0, "authorized record must be invalidated");
        assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn failed_final_identity_check_preserves_unrelated_replacement_record() {
        use std::os::unix::fs::PermissionsExt;

        let source = tempfile::tempdir().unwrap();
        let private = tempfile::tempdir().unwrap();
        std::fs::set_permissions(private.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = private.path().join("audit.json");
        let path_for_hook = path.clone();
        let replacement = b"attacker-replacement".to_vec();
        let replacement_for_hook = replacement.clone();

        let error = write_private_json_create_new_unix_with_hooks(
            source.path(),
            &path,
            &serde_json::json!({"private": true}),
            || {},
            || {},
            move || {
                std::fs::remove_file(&path_for_hook).unwrap();
                std::fs::write(&path_for_hook, &replacement_for_hook).unwrap();
            },
        )
        .unwrap_err();

        assert_eq!(error, "private-evidence-record-identity-drift");
        assert_eq!(std::fs::read(&path).unwrap(), replacement);
    }
}
