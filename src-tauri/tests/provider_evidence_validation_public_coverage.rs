//! Public-boundary coverage for provider synchronization evidence validation.
//!
//! These cases exercise fail-closed record construction and validation without
//! reaching into provider internals or mutating the evidence persistence path.

use disksage_lib::cloud::CloudProvider;
use disksage_lib::cloud_transfer::{
    ProviderSyncEvidence, RemoteChecksumAlgorithm, RemoteContentProof, SyncEvidenceKind,
};
use disksage_lib::provider_evidence::{
    create_sync_evidence_record, validate_sync_evidence_record, PROVIDER_EVIDENCE_RECORD_VERSION,
};

fn absolute_destination() -> String {
    #[cfg(windows)]
    {
        r"C:\cloud\report.pdf".to_string()
    }
    #[cfg(not(windows))]
    {
        "/cloud/report.pdf".to_string()
    }
}

fn parent_destination() -> String {
    #[cfg(windows)]
    {
        r"C:\cloud\..\report.pdf".to_string()
    }
    #[cfg(not(windows))]
    {
        "/cloud/../report.pdf".to_string()
    }
}

fn api_evidence() -> ProviderSyncEvidence {
    ProviderSyncEvidence {
        receipt_id: "a".repeat(64),
        provider: CloudProvider::Onedrive,
        destination: absolute_destination(),
        observed_bytes: 42,
        destination_blake3: "b".repeat(64),
        confirmed_at_ms: 30,
        kind: SyncEvidenceKind::ProviderApi,
        evidence_id: format!("provider-api:{}", "c".repeat(64)),
        sync_complete: true,
        remote_content: Some(RemoteContentProof {
            object_id: "remote-id".into(),
            revision: "revision-1".into(),
            algorithm: RemoteChecksumAlgorithm::QuickXor,
            checksum: "quick-xor".into(),
            location_bound: true,
            location_proof: Some(format!("onedrive-path-v1:{}", "d".repeat(64))),
        }),
    }
}

#[test]
fn record_creation_rejects_each_bounded_identity_and_destination_violation() {
    let mut cases = Vec::new();

    let mut short_receipt = api_evidence();
    short_receipt.receipt_id = "a".repeat(63);
    cases.push((short_receipt, "provider-evidence-receipt-id-invalid"));

    let mut non_hex_receipt = api_evidence();
    non_hex_receipt.receipt_id = "g".repeat(64);
    cases.push((non_hex_receipt, "provider-evidence-receipt-id-invalid"));

    let mut empty_destination = api_evidence();
    empty_destination.destination.clear();
    cases.push((empty_destination, "provider-evidence-destination-invalid"));

    let mut relative_destination = api_evidence();
    relative_destination.destination = "cloud/report.pdf".into();
    cases.push((relative_destination, "provider-evidence-destination-invalid"));

    let mut parent_path = api_evidence();
    parent_path.destination = parent_destination();
    cases.push((parent_path, "provider-evidence-destination-invalid"));

    let mut oversized_destination = api_evidence();
    oversized_destination.destination = format!("/{}", "d".repeat(32 * 1024));
    #[cfg(windows)]
    {
        oversized_destination.destination = format!(r"C:\{}", "d".repeat(32 * 1024));
    }
    cases.push((oversized_destination, "provider-evidence-destination-invalid"));

    let mut bad_destination_hash = api_evidence();
    bad_destination_hash.destination_blake3 = "z".repeat(64);
    cases.push((
        bad_destination_hash,
        "provider-evidence-destination-hash-invalid",
    ));

    for (evidence, expected) in cases {
        assert_eq!(create_sync_evidence_record(&evidence).unwrap_err(), expected);
    }
}

#[test]
fn record_creation_rejects_unbounded_or_inconsistent_evidence_ids_and_kinds() {
    let mut empty_id = api_evidence();
    empty_id.evidence_id.clear();
    assert_eq!(
        create_sync_evidence_record(&empty_id).unwrap_err(),
        "provider-evidence-id-invalid"
    );

    let mut oversized_id = api_evidence();
    oversized_id.evidence_id = "e".repeat(1_025);
    assert_eq!(
        create_sync_evidence_record(&oversized_id).unwrap_err(),
        "provider-evidence-id-invalid"
    );

    let mut control_id = api_evidence();
    control_id.evidence_id = "provider-api:\nforged".into();
    assert_eq!(
        create_sync_evidence_record(&control_id).unwrap_err(),
        "provider-evidence-id-invalid"
    );

    let mut native_with_remote = api_evidence();
    native_with_remote.kind = SyncEvidenceKind::ProviderNativeStatus;
    assert_eq!(
        create_sync_evidence_record(&native_with_remote).unwrap_err(),
        "provider-evidence-native-remote-content-unexpected"
    );

    let mut api_without_remote = api_evidence();
    api_without_remote.remote_content = None;
    assert_eq!(
        create_sync_evidence_record(&api_without_remote).unwrap_err(),
        "provider-evidence-api-remote-content-missing"
    );

    let mut native = api_evidence();
    native.kind = SyncEvidenceKind::ProviderNativeStatus;
    native.remote_content = None;
    let record = create_sync_evidence_record(&native).expect("native status without API proof is valid");
    validate_sync_evidence_record(&record).expect("fresh native-status record must validate");
}

#[test]
fn record_validation_rejects_version_record_id_and_evidence_tampering() {
    let record = create_sync_evidence_record(&api_evidence()).expect("valid evidence must construct");
    validate_sync_evidence_record(&record).expect("fresh record must validate");

    let mut unsupported_version = record.clone();
    unsupported_version.version = PROVIDER_EVIDENCE_RECORD_VERSION + 1;
    assert_eq!(
        validate_sync_evidence_record(&unsupported_version).unwrap_err(),
        "provider-evidence-record-version-unsupported"
    );

    let mut malformed_record_id = record.clone();
    malformed_record_id.record_id = "x".repeat(64);
    assert_eq!(
        validate_sync_evidence_record(&malformed_record_id).unwrap_err(),
        "provider-evidence-record-integrity-mismatch"
    );

    let mut changed_evidence = record;
    changed_evidence.evidence.confirmed_at_ms += 1;
    assert_eq!(
        validate_sync_evidence_record(&changed_evidence).unwrap_err(),
        "provider-evidence-record-integrity-mismatch"
    );
}
