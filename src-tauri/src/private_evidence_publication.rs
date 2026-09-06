pub use crate::private_evidence_core::{
    write_private_json_create_new, PrivateEvidenceReceipt, MAX_PRIVATE_EVIDENCE_BYTES,
};

#[cfg(unix)]
pub(crate) use crate::private_evidence_core::ObjectBoundPublicationError;

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
