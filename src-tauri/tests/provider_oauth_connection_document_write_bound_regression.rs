#![allow(dead_code, unused_imports)]

// Compile the production OAuth module into this integration-test crate so the regression can
// exercise its private persistence boundary without widening the shipped API surface.
include!("../src/provider_oauth.rs");

// The included production module resolves `crate::cloud`; re-export the shipped cloud types under
// the same crate-local path while keeping the test credential-free and network-free.
mod cloud {
    pub use disksage_lib::cloud::*;
}

#[test]
fn connection_document_writer_rejects_oversized_payload_before_publication() {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("first-use-app-data");
    let path = parent.join("connections.json");

    #[cfg(windows)]
    let cloud_path = r"C:\Cloud".to_string();
    #[cfg(not(windows))]
    let cloud_path = "/Cloud".to_string();

    let root = CloudRoot {
        id: "x".repeat(MAX_CONNECTION_DOCUMENT_BYTES as usize + 1024),
        provider: CloudProvider::GoogleDrive,
        account_scope: crate::cloud::CloudAccountScope::Unknown,
        label: "Cloud".into(),
        path: cloud_path,
        readable: true,
        access_issue: None,
    };
    let connection = OAuthConnection {
        connection_id: connection_id(&root),
        provider: root.provider,
        cloud_root_id: root.id.clone(),
        cloud_root_path: root.path.clone(),
        client_id: "1234567890-abcxyz.apps.googleusercontent.com".into(),
        scope: requested_scope(root.provider).unwrap().into(),
        connected_at_ms: 123,
    };

    assert_eq!(
        save_connections(&path, &[connection]).unwrap_err(),
        "oauth-connection-document-too-large"
    );
    assert!(
        !parent.exists(),
        "rejected oversized metadata must not create its first-use authority directory"
    );
    assert!(
        !path.exists(),
        "rejected oversized metadata must not publish an unreadable durable document"
    );
}
