use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Write;
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

/// Persist exact local evidence outside the audited source tree.
///
/// The destination parent must already exist, must not be a symlink, and must not be writable by
/// group or other principals. On Unix, publication is bound to the exact opened parent-directory
/// object so a same-user pathname replacement cannot redirect the write after authorization. The
/// file is created once with mode 0600, synced, and never overwritten. A failed publication is
/// cleaned through the authorized directory descriptor rather than through a mutable pathname.
#[cfg(unix)]
pub fn write_private_json_create_new(
    source_root: &Path,
    path: &Path,
    value: &impl Serialize,
) -> Result<PrivateEvidenceReceipt, String> {
    write_private_json_create_new_unix_with_hook(source_root, path, value, || {})
}

#[cfg(unix)]
fn write_private_json_create_new_unix_with_hook<F>(
    source_root: &Path,
    path: &Path,
    value: &impl Serialize,
    before_create: F,
) -> Result<PrivateEvidenceReceipt, String>
where
    F: FnOnce(),
{
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| "private-evidence-parent-missing".to_string())?;
    let parent_metadata = std::fs::symlink_metadata(parent)
        .map_err(|_| "private-evidence-parent-unavailable".to_string())?;
    if !parent_metadata.is_dir() || parent_metadata.file_type().is_symlink() {
        return Err("private-evidence-parent-unsafe".into());
    }
    if parent_metadata.permissions().mode() & 0o022 != 0 {
        return Err("private-evidence-parent-writable-by-others".into());
    }
    let canonical_parent = std::fs::canonicalize(parent)
        .map_err(|_| "private-evidence-parent-unavailable".to_string())?;
    let canonical_source = std::fs::canonicalize(source_root)
        .map_err(|_| "private-evidence-source-root-unavailable".to_string())?;
    if canonical_parent.starts_with(&canonical_source) {
        return Err("private-evidence-inside-source-root".into());
    }
    let file_name = path
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| "private-evidence-name-invalid".to_string())?;
    let final_path = canonical_parent.join(file_name);
    let encoded = serde_json::to_vec_pretty(value)
        .map_err(|_| "private-evidence-json-invalid".to_string())?;
    if encoded.len() > MAX_PRIVATE_EVIDENCE_BYTES {
        return Err("private-evidence-too-large".into());
    }

    let parent_c = CString::new(canonical_parent.as_os_str().as_bytes())
        .map_err(|_| "private-evidence-parent-unavailable".to_string())?;
    let file_name_c = CString::new(file_name.as_bytes())
        .map_err(|_| "private-evidence-name-invalid".to_string())?;
    let directory_fd = unsafe {
        libc::open(
            parent_c.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        )
    };
    if directory_fd < 0 {
        return Err("private-evidence-parent-unavailable".into());
    }
    let directory = unsafe { std::fs::File::from_raw_fd(directory_fd) };
    let opened_parent_metadata = directory
        .metadata()
        .map_err(|_| "private-evidence-parent-unavailable".to_string())?;
    if !opened_parent_metadata.is_dir() || opened_parent_metadata.file_type().is_symlink() {
        return Err("private-evidence-parent-unsafe".into());
    }
    if opened_parent_metadata.permissions().mode() & 0o022 != 0 {
        return Err("private-evidence-parent-writable-by-others".into());
    }
    let current_parent_metadata = std::fs::symlink_metadata(&canonical_parent)
        .map_err(|_| "private-evidence-parent-identity-drift".to_string())?;
    if current_parent_metadata.file_type().is_symlink()
        || !current_parent_metadata.is_dir()
        || current_parent_metadata.dev() != opened_parent_metadata.dev()
        || current_parent_metadata.ino() != opened_parent_metadata.ino()
    {
        return Err("private-evidence-parent-identity-drift".into());
    }

    before_create();

    // Re-check the pathname immediately after the deterministic race seam. Publication itself is
    // descriptor-relative, so even a later rename cannot redirect record creation to a new parent.
    let parent_before_create = std::fs::symlink_metadata(&canonical_parent)
        .map_err(|_| "private-evidence-parent-identity-drift".to_string())?;
    if parent_before_create.file_type().is_symlink()
        || !parent_before_create.is_dir()
        || parent_before_create.dev() != opened_parent_metadata.dev()
        || parent_before_create.ino() != opened_parent_metadata.ino()
    {
        return Err("private-evidence-parent-identity-drift".into());
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
            0o600,
        )
    };
    if file_fd < 0 {
        return Err("private-evidence-create-failed".into());
    }
    let mut file = unsafe { std::fs::File::from_raw_fd(file_fd) };

    let publication = (|| -> Result<(), String> {
        file.write_all(&encoded)
            .and_then(|_| file.sync_all())
            .map_err(|_| "private-evidence-write-failed".to_string())?;
        let opened_file_metadata = file
            .metadata()
            .map_err(|_| "private-evidence-metadata-failed".to_string())?;
        if !opened_file_metadata.is_file()
            || opened_file_metadata.file_type().is_symlink()
            || opened_file_metadata.permissions().mode() & 0o777 != 0o600
        {
            return Err("private-evidence-mode-invalid".into());
        }
        directory
            .sync_all()
            .map_err(|_| "private-evidence-parent-sync-failed".to_string())?;

        let final_parent_metadata = std::fs::symlink_metadata(&canonical_parent)
            .map_err(|_| "private-evidence-parent-identity-drift".to_string())?;
        if final_parent_metadata.file_type().is_symlink()
            || !final_parent_metadata.is_dir()
            || final_parent_metadata.dev() != opened_parent_metadata.dev()
            || final_parent_metadata.ino() != opened_parent_metadata.ino()
        {
            return Err("private-evidence-parent-identity-drift".into());
        }

        let final_file_metadata = std::fs::symlink_metadata(&final_path)
            .map_err(|_| "private-evidence-record-identity-drift".to_string())?;
        if final_file_metadata.file_type().is_symlink()
            || !final_file_metadata.is_file()
            || final_file_metadata.dev() != opened_file_metadata.dev()
            || final_file_metadata.ino() != opened_file_metadata.ino()
        {
            return Err("private-evidence-record-identity-drift".into());
        }
        Ok(())
    })();

    if let Err(error) = publication {
        drop(file);
        unsafe {
            libc::unlinkat(directory.as_raw_fd(), file_name_c.as_ptr(), 0);
        }
        let _ = directory.sync_all();
        return Err(error);
    }

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

        let error = write_private_json_create_new_unix_with_hook(
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
        )
        .unwrap_err();

        assert_eq!(error, "private-evidence-parent-identity-drift");
        assert!(!replacement_parent.join("audit.json").exists());
        assert!(!moved_parent.join("audit.json").exists());
    }
}
