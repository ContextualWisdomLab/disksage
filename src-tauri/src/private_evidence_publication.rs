pub use crate::private_evidence_core::{
    write_private_json_create_new, PrivateEvidenceReceipt, MAX_PRIVATE_EVIDENCE_BYTES,
};

#[cfg(unix)]
pub(crate) use crate::private_evidence_core::{
    write_object_bound_bytes_create_new_with_hooks, ObjectBoundPublicationError,
};

#[cfg(unix)]
fn map_directory_publication_error(error: String) -> ObjectBoundPublicationError {
    match error.as_str() {
        "private-directory-publication-anchor-missing" => ObjectBoundPublicationError::ParentMissing,
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
        "private-directory-publication-file-name-invalid"
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
        "private-directory-publication-invalidation-failed" => {
            ObjectBoundPublicationError::InvalidationFailed
        }
        _ => ObjectBoundPublicationError::ParentUnavailable,
    }
}

/// Publish a private create-new record while allowing the canonical filesystem owner to provision
/// missing owner-private ancestors when no forbidden-root policy is required.
///
/// Both publication paths admit only the reusable private-record modes 0400 and 0600. Callers with
/// a forbidden root retain the original parent-must-exist contract because the directory-provisioning
/// primitive does not yet carry a descriptor-bound forbidden-root policy. The no-policy path requires
/// every existing final parent to be exactly 0700 and creates missing descendants at 0700 before
/// publishing through the same pinned descriptor chain.
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
        return crate::private_evidence_core::write_object_bound_bytes_create_new(
            path,
            encoded,
            unix_mode,
            forbidden_root,
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
    fn no_policy_publication_provisions_missing_private_parent() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let target = temp.path().join("receipts/provider-cache/receipt.json");

        write_object_bound_bytes_create_new(&target, b"receipt", 0o400, None).unwrap();

        assert_eq!(
            fs::metadata(temp.path().join("receipts/provider-cache"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o400);
        assert_eq!(fs::read(target).unwrap(), b"receipt");
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
}
