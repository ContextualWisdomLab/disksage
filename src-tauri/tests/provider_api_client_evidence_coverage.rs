//! Credential-free coverage for provider API JSON evidence projection.
//!
//! These tests exercise the public conversion boundary from authenticated-provider response JSON
//! plus locally verified destination digests into immutable sync evidence. They perform no network
//! I/O and use no provider credentials.

use disksage_lib::cloud::CloudProvider;
use disksage_lib::cloud_transfer::{
    CloudCopyReceipt, RemoteChecksumAlgorithm, SyncEvidenceKind, LEGACY_RECEIPT_VERSION,
};
use disksage_lib::content_digest::ContentDigests;
use disksage_lib::provider_api_client::{
    evidence_from_provider_api_json, ProviderRemoteLocator,
};

fn receipt(provider: CloudProvider) -> CloudCopyReceipt {
    CloudCopyReceipt {
        version: LEGACY_RECEIPT_VERSION,
        receipt_id: "receipt-id".into(),
        candidate_fingerprint: "candidate-fingerprint".into(),
        provider,
        source: "/source/report.pdf".into(),
        destination: "/cloud/report.pdf".into(),
        bytes: 42,
        blake3: "content-hash".into(),
        sha256: "sha256-hash".into(),
        quick_xor_base64: "quick-xor".into(),
        source_modified_ms: 10,
        copied_at_ms: 20,
        copy_verified: true,
        provider_sync_confirmed: false,
        lineage_fingerprint: None,
        lineage: None,
    }
}

fn matching_digests() -> ContentDigests {
    ContentDigests {
        blake3: "content-hash".into(),
        sha256: "SHA256-HASH".into(),
        quick_xor_base64: "quick-xor".into(),
    }
}

#[test]
fn provider_and_local_digest_mismatches_fail_before_remote_evidence_projection() {
    let onedrive_receipt = receipt(CloudProvider::Onedrive);
    let google_locator = ProviderRemoteLocator::GoogleDriveFileId("google-id".into());
    assert_eq!(
        evidence_from_provider_api_json(
            &onedrive_receipt,
            &google_locator,
            "{}",
            &matching_digests(),
            30,
        )
        .unwrap_err(),
        "provider-mismatch"
    );

    let locator = ProviderRemoteLocator::OneDriveItemId("item-1".into());
    let mismatched = ContentDigests {
        blake3: "different-content".into(),
        ..matching_digests()
    };
    assert_eq!(
        evidence_from_provider_api_json(&onedrive_receipt, &locator, "{}", &mismatched, 30)
            .unwrap_err(),
        "destination-content-mismatch"
    );
}

#[test]
fn onedrive_json_becomes_content_bound_object_id_evidence() {
    let receipt = receipt(CloudProvider::Onedrive);
    let locator = ProviderRemoteLocator::OneDriveItemId("item-1".into());
    let json = r#"{
        "id": "item-1",
        "size": 42,
        "eTag": "revision-1",
        "file": {"hashes": {"quickXorHash": "quick-xor"}}
    }"#;

    let evidence =
        evidence_from_provider_api_json(&receipt, &locator, json, &matching_digests(), 30).unwrap();

    assert_eq!(evidence.receipt_id, "receipt-id");
    assert_eq!(evidence.provider, CloudProvider::Onedrive);
    assert_eq!(evidence.destination, "/cloud/report.pdf");
    assert_eq!(evidence.observed_bytes, 42);
    assert_eq!(evidence.destination_blake3, "content-hash");
    assert_eq!(evidence.confirmed_at_ms, 30);
    assert_eq!(evidence.kind, SyncEvidenceKind::ProviderApi);
    assert!(evidence.evidence_id.starts_with("provider-api:"));
    assert!(evidence.sync_complete);
    let remote = evidence.remote_content.unwrap();
    assert_eq!(remote.object_id, "item-1");
    assert_eq!(remote.revision, "revision-1");
    assert_eq!(remote.algorithm, RemoteChecksumAlgorithm::QuickXor);
    assert_eq!(remote.checksum, "quick-xor");
    assert!(!remote.location_bound);
    assert_eq!(remote.location_proof, None);
}

#[test]
fn google_drive_json_accepts_case_insensitive_sha256_and_rejects_object_id_drift() {
    let receipt = receipt(CloudProvider::GoogleDrive);
    let locator = ProviderRemoteLocator::GoogleDriveFileId("google-id".into());
    let matching_json = r#"{
        "id": "google-id",
        "version": "7",
        "size": "42",
        "sha256Checksum": "SHA256-HASH",
        "trashed": false
    }"#;

    let evidence = evidence_from_provider_api_json(
        &receipt,
        &locator,
        matching_json,
        &matching_digests(),
        40,
    )
    .unwrap();

    assert!(evidence.sync_complete);
    let remote = evidence.remote_content.unwrap();
    assert_eq!(remote.object_id, "google-id");
    assert_eq!(remote.revision, "7");
    assert_eq!(remote.algorithm, RemoteChecksumAlgorithm::Sha256);
    assert_eq!(remote.checksum, "SHA256-HASH");
    assert!(!remote.location_bound);

    let drifted_json = r#"{
        "id": "different-id",
        "version": "7",
        "size": "42",
        "sha256Checksum": "SHA256-HASH",
        "trashed": false
    }"#;
    assert_eq!(
        evidence_from_provider_api_json(
            &receipt,
            &locator,
            drifted_json,
            &matching_digests(),
            41,
        )
        .unwrap_err(),
        "provider-object-id-mismatch"
    );
}
