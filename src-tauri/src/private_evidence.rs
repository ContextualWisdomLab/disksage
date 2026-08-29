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
/// group or other principals. The file is created once with mode 0600, synced, and never
/// overwritten. A failed write is removed before returning.
#[cfg(unix)]
pub fn write_private_json_create_new(
    source_root: &Path,
    path: &Path,
    value: &impl Serialize,
) -> Result<PrivateEvidenceReceipt, String> {
    write_private_json_create_new_with_limit(source_root, path, value, MAX_PRIVATE_EVIDENCE_BYTES)
}

/// Persist exact local evidence with a domain-specific encoded-size ceiling.
#[cfg(unix)]
pub fn write_private_json_create_new_with_limit(
    source_root: &Path,
    path: &Path,
    value: &impl Serialize,
    max_encoded_bytes: usize,
) -> Result<PrivateEvidenceReceipt, String> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

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
    if encoded.len() > max_encoded_bytes {
        return Err("private-evidence-too-large".into());
    }

    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&final_path)
        .map_err(|_| "private-evidence-create-failed".to_string())?;
    let result = (|| -> Result<(), String> {
        file.write_all(&encoded)
            .and_then(|_| file.sync_all())
            .map_err(|_| "private-evidence-write-failed".to_string())?;
        let metadata = file
            .metadata()
            .map_err(|_| "private-evidence-metadata-failed".to_string())?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.permissions().mode() & 0o777 != 0o600
        {
            return Err("private-evidence-mode-invalid".into());
        }
        std::fs::File::open(&canonical_parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| "private-evidence-parent-sync-failed".to_string())
    })();
    if let Err(error) = result {
        drop(file);
        let _ = std::fs::remove_file(&final_path);
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

#[cfg(not(unix))]
pub fn write_private_json_create_new_with_limit(
    _source_root: &Path,
    _path: &Path,
    _value: &impl Serialize,
    _max_encoded_bytes: usize,
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
}
