//! Fail-closed private-record replacement boundary.
//!
//! DiskSage can create immutable private evidence through descriptor-relative create-new primitives,
//! but replacing an existing private record is a different authority. POSIX `renameat` identifies
//! its source by directory-relative name, so revalidating a staging pathname immediately before the
//! syscall still leaves a check-to-mutation interval in which a same-UID process can substitute a
//! different object. Windows likewise has no accepted implementation in this owner yet.
//!
//! Until a platform implementation can bind final publication to the exact reviewed source object,
//! replacement fails before creating, writing, renaming, unlinking, or otherwise mutating any
//! filesystem object. This is intentional product behavior, not a transient fallback.

use std::path::Path;

/// Stable failure classes exposed to DiskSage domain adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ObjectBoundReplaceError {
    /// Requested Unix permissions are not owner-private or contain unsupported special bits.
    ModeInvalid,
    /// The platform owner cannot yet prove that final publication consumes the reviewed source object.
    SourceIdentityUnavailable,
    /// Replacement is not implemented for this platform owner.
    UnsupportedPlatform,
}

impl ObjectBoundReplaceError {
    /// Stable machine-readable failure identifier for domain adapters and audit evidence.
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::ModeInvalid => "object-bound-replace-mode-invalid",
            Self::SourceIdentityUnavailable => {
                "object-bound-replace-source-identity-unavailable"
            }
            Self::UnsupportedPlatform => "object-bound-replace-unsupported-platform",
        }
    }
}

/// Refuse replacement until final mutation can be conditioned on the exact reviewed source object.
///
/// Unix still validates the requested private mode before returning the capability error so callers
/// cannot accidentally normalize an invalid permission request into a future valid one. The function
/// performs no filesystem lookup or mutation for an otherwise valid request. Create-new evidence
/// publication remains owned by `private_directory_publication` / `private_evidence` and is unaffected.
pub(crate) fn replace_object_bound_bytes(
    path: &Path,
    encoded: &[u8],
    unix_mode: u32,
) -> Result<(), ObjectBoundReplaceError> {
    #[cfg(unix)]
    {
        if unix_mode & !0o777 != 0 || unix_mode & 0o077 != 0 {
            return Err(ObjectBoundReplaceError::ModeInvalid);
        }
        let _ = (path, encoded);
        Err(ObjectBoundReplaceError::SourceIdentityUnavailable)
    }

    #[cfg(not(unix))]
    {
        let _ = (path, encoded, unix_mode);
        Err(ObjectBoundReplaceError::UnsupportedPlatform)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn valid_private_replacement_fails_before_any_filesystem_mutation() {
        let root = tempfile::tempdir().expect("tempdir");
        let parent = root.path().join("private");
        std::fs::create_dir(&parent).expect("create private parent");
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700))
            .expect("set private parent mode");
        let record = parent.join("connections.json");
        std::fs::write(&record, b"old").expect("seed record");

        let before = std::fs::read(&record).expect("read old record");
        let before_names = std::fs::read_dir(&parent)
            .expect("read parent")
            .map(|entry| entry.expect("entry").file_name())
            .collect::<Vec<_>>();

        let error = replace_object_bound_bytes(&record, b"new", 0o600)
            .expect_err("replacement must remain unavailable");

        assert_eq!(error, ObjectBoundReplaceError::SourceIdentityUnavailable);
        assert_eq!(std::fs::read(&record).expect("read preserved record"), before);
        let after_names = std::fs::read_dir(&parent)
            .expect("read parent after refusal")
            .map(|entry| entry.expect("entry").file_name())
            .collect::<Vec<_>>();
        assert_eq!(after_names, before_names, "refusal must not create staging names");
    }

    #[test]
    fn invalid_private_mode_is_rejected_before_capability_evaluation() {
        let root = tempfile::tempdir().expect("tempdir");
        let record = root.path().join("connections.json");

        for invalid_mode in [0o644, 0o660, 0o1600, 0o2600, 0o4600] {
            let error = replace_object_bound_bytes(&record, b"private", invalid_mode)
                .expect_err("non-private or special-bit mode must fail closed");
            assert_eq!(error, ObjectBoundReplaceError::ModeInvalid);
        }
        assert!(!record.exists());
        assert_eq!(std::fs::read_dir(root.path()).expect("read root").count(), 0);
    }

    #[test]
    fn source_identity_unavailable_has_stable_error_code() {
        assert_eq!(
            ObjectBoundReplaceError::SourceIdentityUnavailable.code(),
            "object-bound-replace-source-identity-unavailable"
        );
    }
}

#[cfg(all(test, not(unix)))]
mod non_unix_tests {
    use super::*;

    #[test]
    fn unsupported_platform_fails_without_touching_target() {
        let root = tempfile::tempdir().expect("tempdir");
        let record = root.path().join("connections.json");
        std::fs::write(&record, b"old").expect("seed record");

        let error = replace_object_bound_bytes(&record, b"new", 0o600)
            .expect_err("replacement must fail closed");

        assert_eq!(error, ObjectBoundReplaceError::UnsupportedPlatform);
        assert_eq!(std::fs::read(&record).expect("read record"), b"old");
    }
}
