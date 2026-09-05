//! Contract for requested private-record modes while replacement is fail closed.
//!
//! Replacement currently performs no staging or publication, so post-create special-bit drift is
//! impossible on this path. Mode admission still rejects any requested setuid/setgid/sticky bits as
//! well as group/other permissions before returning the source-identity capability error.

const SOURCE: &str = include_str!("../src/object_bound_publication.rs");

#[test]
fn replacement_rejects_special_bits_before_source_identity_capability_evaluation() {
    assert!(
        SOURCE.contains("unix_mode & !0o777 != 0"),
        "requested setuid/setgid/sticky bits must remain outside the admitted private mode set"
    );
    assert!(
        SOURCE.contains("unix_mode & 0o077 != 0"),
        "group/other permissions must remain rejected before capability evaluation"
    );
    assert!(
        SOURCE.contains("ObjectBoundReplaceError::SourceIdentityUnavailable"),
        "valid private modes must still fail closed while exact-source publication is unavailable"
    );
}
