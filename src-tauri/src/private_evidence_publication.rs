pub use crate::private_evidence_core::{PrivateEvidenceReceipt, MAX_PRIVATE_EVIDENCE_BYTES};

#[cfg(unix)]
pub(crate) use crate::private_evidence_core::ObjectBoundPublicationError;

#[cfg(unix)]
use serde::Serialize;
#[cfg(unix)]
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::io::Write;

#[cfg(unix)]
struct BoundedJsonWriter {
    bytes: Vec<u8>,
    exceeded: bool,
}

#[cfg(unix)]
impl BoundedJsonWriter {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            exceeded: false,
        }
    }

    fn into_inner(self) -> Vec<u8> {
        self.bytes
    }
}

#[cfg(unix)]
impl Write for BoundedJsonWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let remaining = MAX_PRIVATE_EVIDENCE_BYTES.saturating_sub(self.bytes.len());
        if buf.len() > remaining {
            self.exceeded = true;
            return Err(std::io::Error::other(
                "private evidence exceeds encoded-size budget",
            ));
        }
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(unix)]
fn serialize_private_json_bounded(value: &impl Serialize) -> Result<Vec<u8>, String> {
    let mut writer = BoundedJsonWriter::new();
    match serde_json::to_writer_pretty(&mut writer, value) {
        Ok(()) => Ok(writer.into_inner()),
        Err(_) if writer.exceeded => Err("private-evidence-too-large".into()),
        Err(_) => Err("private-evidence-json-invalid".into()),
    }
}

#[cfg(unix)]
fn map_object_publication_error(error: ObjectBoundPublicationError) -> String {
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
        ObjectBoundPublicationError::InvalidationFailed => "private-evidence-invalidation-failed",
    }
    .to_string()
}

/// Persist exact local JSON evidence outside the audited source tree without materializing an
/// unbounded encoded payload first.
///
/// Serialization stops as soon as the encoded JSON budget is exhausted, before filesystem lookup or
/// mutation. Successful payloads retain the same 8 MiB maximum, pretty-JSON representation, SHA-256
/// receipt, exact Unix 0600 mode, create-new semantics, and source-root exclusion contract as the
/// object-bound publication core.
#[cfg(unix)]
pub fn write_private_json_create_new(
    source_root: &std::path::Path,
    path: &std::path::Path,
    value: &impl Serialize,
) -> Result<PrivateEvidenceReceipt, String> {
    let encoded = serialize_private_json_bounded(value)?;

    write_object_bound_bytes_create_new(path, &encoded, 0o600, Some(source_root))
        .map_err(map_object_publication_error)?;

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
    _source_root: &std::path::Path,
    _path: &std::path::Path,
    _value: &impl serde::Serialize,
) -> Result<PrivateEvidenceReceipt, String> {
    Err("private-evidence-secure-mode-unsupported".into())
}

/// Expose the deterministic publication hooks without bypassing the private-record mode invariant.
///
/// Dependent authority tests use these hooks to inject namespace and permission races into the same
/// production implementation. Admission must therefore happen before any hook, filesystem lookup, or
/// mutation; otherwise the test seam itself becomes a broader publication capability than production.
#[cfg(unix)]
pub(crate) fn write_object_bound_bytes_create_new_with_hooks<F, G, H>(
    path: &std::path::Path,
    encoded: &[u8],
    unix_mode: u32,
    forbidden_root: Option<&std::path::Path>,
    before_parent_open: F,
    before_create: G,
    before_finalize: H,
) -> Result<(), ObjectBoundPublicationError>
where
    F: FnOnce(),
    G: FnOnce(),
    H: FnOnce(),
{
    if !matches!(unix_mode, 0o400 | 0o600) {
        return Err(ObjectBoundPublicationError::ModeInvalid);
    }

    crate::private_evidence_core::write_object_bound_bytes_create_new_with_hooks(
        path,
        encoded,
        unix_mode,
        forbidden_root,
        before_parent_open,
        before_create,
        before_finalize,
    )
}

#[cfg(unix)]
fn map_directory_publication_error(error: String) -> ObjectBoundPublicationError {
    match error.as_str() {
        "private-directory-publication-anchor-missing"
        | "private-directory-publication-parent-missing"
        | "private-directory-publication-parent-provisioning-unavailable" => {
            ObjectBoundPublicationError::ParentMissing
        }
        "private-directory-publication-anchor-unavailable" => {
            ObjectBoundPublicationError::ParentUnavailable
        }
        "private-directory-publication-directory-unsafe" => ObjectBoundPublicationError::ParentUnsafe,
        "private-directory-publication-directory-writable-by-others" => {
            ObjectBoundPublicationError::ParentWritableByOthers
        }
        "private-directory-publication-anchor-identity-drift"
        | "private-directory-publication-directory-identity-drift" => {
            ObjectBoundPublicationError::ParentIdentityDrift
        }
        "private-directory-publication-path-invalid"
        | "private-directory-publication-file-name-invalid"
        | "private-directory-publication-directory-name-invalid" => {
            ObjectBoundPublicationError::NameInvalid
        }
        "private-directory-publication-file-create-failed"
        | "private-directory-publication-directory-create-failed"
        | "private-directory-publication-directory-open-failed" => {
            ObjectBoundPublicationError::CreateFailed
        }
        "private-directory-publication-file-mode-invalid"
        | "private-directory-publication-directory-mode-invalid"
        | "private-directory-publication-directory-mode-drift"
        | "private-directory-publication-directory-mode-failed"
        | "private-directory-publication-file-mode-failed"
        | "private-directory-publication-file-mode-drift" => ObjectBoundPublicationError::ModeInvalid,
        "private-directory-publication-file-write-failed" => ObjectBoundPublicationError::WriteFailed,
        "private-directory-publication-file-metadata-failed" => {
            ObjectBoundPublicationError::MetadataFailed
        }
        "private-directory-publication-directory-sync-failed" => {
            ObjectBoundPublicationError::ParentSyncFailed
        }
        "private-directory-publication-file-identity-drift" => {
            ObjectBoundPublicationError::RecordIdentityDrift
        }
        "private-directory-publication-file-content-drift" => {
            ObjectBoundPublicationError::RecordContentDrift
        }
        "private-directory-publication-invalidation-failed" => {
            ObjectBoundPublicationError::InvalidationFailed
        }
        _ => ObjectBoundPublicationError::ParentUnavailable,
    }
}

/// Publish a private create-new record through an existing owner-private final parent when no
/// forbidden-root policy is required.
///
/// Both publication paths admit only reusable private-record modes 0400 and 0600. Callers with a
/// forbidden root retain the direct object-bound parent contract. The no-policy path also requires
/// the final parent to pre-exist at exact mode 0700. It deliberately does not create missing
/// ancestors because POSIX `mkdirat()` does not return an opened handle for the newly created
/// directory; a pathname-only create-then-open interval is not accepted as same-object authority.
#[cfg(unix)]
pub(crate) fn write_object_bound_bytes_create_new(
    path: &std::path::Path,
    encoded: &[u8],
    unix_mode: u32,
    forbidden_root: Option<&std::path::Path>,
) -> Result<(), ObjectBoundPublicationError> {
    if !matches!(unix_mode, 0o400 | 0o600) {
        return Err(ObjectBoundPublicationError::ModeInvalid);
    }

    if forbidden_root.is_some() {
        return write_object_bound_bytes_create_new_with_hooks(
            path,
            encoded,
            unix_mode,
            forbidden_root,
            || {},
            || {},
            || {},
        );
    }

    crate::private_directory_publication::write_private_bytes_create_new_with_parents(
        path, encoded, unix_mode, 0o700,
    )
    .map_err(map_directory_publication_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn no_policy_publication_requires_existing_private_parent() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let target = temp.path().join("receipts/provider-cache/receipt.json");

        let error = write_object_bound_bytes_create_new(&target, b"receipt", 0o400, None)
            .expect_err("missing parent provisioning must fail closed");

        assert_eq!(error, ObjectBoundPublicationError::ParentMissing);
        assert!(!temp.path().join("receipts").exists());
        assert!(!target.exists());
    }

    #[cfg(unix)]
    #[test]
    fn no_policy_publication_uses_existing_exact_private_parent() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let parent = temp.path().join("receipts");
        fs::create_dir(&parent).unwrap();
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).unwrap();
        let target = parent.join("receipt.json");

        write_object_bound_bytes_create_new(&target, b"receipt", 0o400, None).unwrap();

        assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o400);
        assert_eq!(fs::read(target).unwrap(), b"receipt");
    }

    #[cfg(unix)]
    #[test]
    fn no_policy_content_drift_keeps_content_drift_error_class() {
        assert_eq!(
            map_directory_publication_error(
                "private-directory-publication-file-content-drift".to_string(),
            ),
            ObjectBoundPublicationError::RecordContentDrift
        );
    }

    #[cfg(unix)]
    #[test]
    fn bounded_json_serializer_preserves_small_payload_representation() {
        let encoded = serialize_private_json_bounded(&serde_json::json!({"private": true})).unwrap();
        assert_eq!(
            encoded,
            serde_json::to_vec_pretty(&serde_json::json!({"private": true})).unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn forbidden_root_policy_rejects_non_private_file_mode_before_create() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let forbidden = tempfile::tempdir().unwrap();
        let destination = tempfile::tempdir().unwrap();
        fs::set_permissions(destination.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let target = destination.path().join("receipt.json");

        let error = write_object_bound_bytes_create_new(
            &target,
            b"sensitive receipt",
            0o644,
            Some(forbidden.path()),
        )
        .expect_err("forbidden-root publication must reject a non-private mode before creation");

        assert_eq!(error, ObjectBoundPublicationError::ModeInvalid);
        assert!(!target.exists(), "invalid mode must not create a record");
    }

    #[cfg(unix)]
    #[test]
    fn hook_publication_rejects_non_private_mode_before_hooks_or_create() {
        use std::cell::Cell;
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let forbidden = tempfile::tempdir().unwrap();
        let destination = tempfile::tempdir().unwrap();
        fs::set_permissions(destination.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let target = destination.path().join("hook-receipt.json");
        let hook_calls = Cell::new(0_u8);

        let error = write_object_bound_bytes_create_new_with_hooks(
            &target,
            b"sensitive receipt",
            0o644,
            Some(forbidden.path()),
            || hook_calls.set(hook_calls.get() + 1),
            || hook_calls.set(hook_calls.get() + 1),
            || hook_calls.set(hook_calls.get() + 1),
        )
        .expect_err("the reusable hook seam must preserve private-mode admission");

        assert_eq!(error, ObjectBoundPublicationError::ModeInvalid);
        assert_eq!(hook_calls.get(), 0, "invalid mode must fail before test seams run");
        assert!(!target.exists(), "invalid mode must not create a record");
    }

    #[cfg(unix)]
    #[test]
    fn core_hook_boundary_rejects_non_private_mode_before_hooks_or_create() {
        use std::cell::Cell;
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let forbidden = tempfile::tempdir().unwrap();
        let destination = tempfile::tempdir().unwrap();
        fs::set_permissions(destination.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let target = destination.path().join("core-hook-receipt.json");
        let hook_calls = Cell::new(0_u8);

        let error = crate::private_evidence_core::write_object_bound_bytes_create_new_with_hooks(
            &target,
            b"sensitive receipt",
            0o644,
            Some(forbidden.path()),
            || hook_calls.set(hook_calls.get() + 1),
            || hook_calls.set(hook_calls.get() + 1),
            || hook_calls.set(hook_calls.get() + 1),
        )
        .expect_err("the core mutation boundary must preserve private-mode admission");

        assert_eq!(error, ObjectBoundPublicationError::ModeInvalid);
        assert_eq!(hook_calls.get(), 0, "invalid mode must fail before core test seams run");
        assert!(!target.exists(), "invalid mode must not create a record");
    }
}
