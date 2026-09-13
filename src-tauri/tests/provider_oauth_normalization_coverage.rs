use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::provider_oauth::{connection_for_root, OAuthConnection};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

fn connection_id(provider: CloudProvider, root_id: &str, root_path: &str) -> String {
    let mut hasher = Sha256::new();
    for value in [provider.as_str(), root_id, root_path] {
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

fn canonical_connection_id(provider: CloudProvider, root_id: &str, root_path: &str) -> String {
    connection_id(
        provider,
        &root_id.nfc().collect::<String>(),
        &root_path.nfc().collect::<String>(),
    )
}

fn root(root_id: &str, root_path: &str) -> CloudRoot {
    CloudRoot {
        id: root_id.into(),
        provider: CloudProvider::GoogleDrive,
        account_scope: CloudAccountScope::Personal,
        label: "Drive".into(),
        path: root_path.into(),
        readable: true,
        access_issue: None,
    }
}

fn connection(root_id: &str, root_path: &str, connection_id: String) -> OAuthConnection {
    OAuthConnection {
        connection_id,
        provider: CloudProvider::GoogleDrive,
        cloud_root_id: root_id.into(),
        cloud_root_path: root_path.into(),
        client_id: "google-client.apps.googleusercontent.com".into(),
        scope: "https://www.googleapis.com/auth/drive.metadata.readonly".into(),
        connected_at_ms: 1,
    }
}

#[test]
fn legacy_nfd_connection_matches_the_same_root_in_nfc() {
    let nfd_id = "root-e\u{301}";
    let nfd_path = "/tmp/Cafe\u{301}";
    let nfc_id = nfd_id.nfc().collect::<String>();
    let nfc_path = nfd_path.nfc().collect::<String>();
    let legacy = connection(
        nfd_id,
        nfd_path,
        connection_id(CloudProvider::GoogleDrive, nfd_id, nfd_path),
    );

    let selected = connection_for_root(&[legacy.clone()], &root(&nfc_id, &nfc_path)).unwrap();
    assert_eq!(selected, legacy);
}

#[test]
fn canonical_normalized_identity_wins_when_legacy_and_canonical_records_both_match() {
    let nfd_id = "root-e\u{301}";
    let nfd_path = "/tmp/Cafe\u{301}";
    let nfc_id = nfd_id.nfc().collect::<String>();
    let nfc_path = nfd_path.nfc().collect::<String>();
    let legacy = connection(
        nfd_id,
        nfd_path,
        connection_id(CloudProvider::GoogleDrive, nfd_id, nfd_path),
    );
    let canonical = connection(
        nfd_id,
        nfd_path,
        canonical_connection_id(CloudProvider::GoogleDrive, nfd_id, nfd_path),
    );

    let selected = connection_for_root(
        &[legacy, canonical.clone()],
        &root(&nfc_id, &nfc_path),
    )
    .unwrap();
    assert_eq!(selected, canonical);
}

#[test]
fn duplicate_legacy_normalization_matches_fail_closed_as_ambiguous() {
    let nfd_id = "root-e\u{301}";
    let nfd_path = "/tmp/Cafe\u{301}";
    let nfc_id = nfd_id.nfc().collect::<String>();
    let nfc_path = nfd_path.nfc().collect::<String>();
    let legacy = connection(
        nfd_id,
        nfd_path,
        connection_id(CloudProvider::GoogleDrive, nfd_id, nfd_path),
    );

    let error = connection_for_root(
        &[legacy.clone(), legacy],
        &root(&nfc_id, &nfc_path),
    )
    .unwrap_err();
    assert_eq!(error, "provider-oauth-connection-ambiguous");
}
