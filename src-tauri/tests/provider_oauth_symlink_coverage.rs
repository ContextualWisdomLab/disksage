//! Filesystem-identity coverage for OAuth connection-document admission.
//!
//! The connection registry is local sensitive state. A symlink must never be followed even when
//! its target contains otherwise valid JSON.

#[cfg(unix)]
#[test]
fn connection_document_symlink_is_rejected_without_following_target() {
    use disksage_lib::provider_oauth::load_connections;
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("real-connections.json");
    std::fs::write(&target, br#"{"version":1,"connections":[]}"#).unwrap();
    let link = temp.path().join("cloud-oauth-connections.json");
    symlink(&target, &link).unwrap();

    assert_eq!(
        load_connections(&link).unwrap_err(),
        "oauth-connection-document-not-regular-file"
    );
    assert_eq!(
        std::fs::read_to_string(&target).unwrap(),
        r#"{"version":1,"connections":[]}"#
    );
}
