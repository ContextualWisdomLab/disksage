use disksage_lib::cloud::CloudProvider;
use disksage_lib::cloud_transfer::{
    ProviderSyncEvidence, RemoteChecksumAlgorithm, RemoteContentProof, SyncEvidenceKind,
};
use disksage_lib::provider_evidence::{
    create_sync_evidence_record, validate_sync_evidence_record, PROVIDER_EVIDENCE_RECORD_VERSION,
};

fn provider_api_evidence() -> ProviderSyncEvidence {
    ProviderSyncEvidence {
        receipt_id: "a".repeat(64),
        provider: CloudProvider::Onedrive,
        destination: "/cloud/report.pdf".into(),
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
fn provider_evidence_rejects_each_identity_and_path_shape_boundary() {
    let mut evidence = provider_api_evidence();
    evidence.receipt_id = "not-a-digest".into();
    assert_eq!(
        create_sync_evidence_record(&evidence).unwrap_err(),
        "provider-evidence-receipt-id-invalid"
    );

    let mut evidence = provider_api_evidence();
    evidence.destination.clear();
    assert_eq!(
        create_sync_evidence_record(&evidence).unwrap_err(),
        "provider-evidence-destination-invalid"
    );

    let mut evidence = provider_api_evidence();
    evidence.destination = "relative/report.pdf".into();
    assert_eq!(
        create_sync_evidence_record(&evidence).unwrap_err(),
        "provider-evidence-destination-invalid"
    );

    let mut evidence = provider_api_evidence();
    evidence.destination = "/cloud/../secret/report.pdf".into();
    assert_eq!(
        create_sync_evidence_record(&evidence).unwrap_err(),
        "provider-evidence-destination-invalid"
    );

    let mut evidence = provider_api_evidence();
    evidence.destination = format!("/{}", "x".repeat(32 * 1024));
    assert_eq!(
        create_sync_evidence_record(&evidence).unwrap_err(),
        "provider-evidence-destination-invalid"
    );

    let mut evidence = provider_api_evidence();
    evidence.destination_blake3 = "z".repeat(64);
    assert_eq!(
        create_sync_evidence_record(&evidence).unwrap_err(),
        "provider-evidence-destination-hash-invalid"
    );
}

#[test]
fn provider_evidence_rejects_bounded_evidence_id_and_remote_content_mismatches() {
    let mut evidence = provider_api_evidence();
    evidence.evidence_id.clear();
    assert_eq!(
        create_sync_evidence_record(&evidence).unwrap_err(),
        "provider-evidence-id-invalid"
    );

    let mut evidence = provider_api_evidence();
    evidence.evidence_id = "x".repeat(1_025);
    assert_eq!(
        create_sync_evidence_record(&evidence).unwrap_err(),
        "provider-evidence-id-invalid"
    );

    let mut evidence = provider_api_evidence();
    evidence.evidence_id = "provider-api\nopaque".into();
    assert_eq!(
        create_sync_evidence_record(&evidence).unwrap_err(),
        "provider-evidence-id-invalid"
    );

    let mut native = provider_api_evidence();
    native.kind = SyncEvidenceKind::ProviderNativeStatus;
    assert_eq!(
        create_sync_evidence_record(&native).unwrap_err(),
        "provider-evidence-native-remote-content-unexpected"
    );

    native.remote_content = None;
    assert!(create_sync_evidence_record(&native).is_ok());

    let mut api_without_remote = provider_api_evidence();
    api_without_remote.remote_content = None;
    assert_eq!(
        create_sync_evidence_record(&api_without_remote).unwrap_err(),
        "provider-evidence-api-remote-content-missing"
    );
}

#[test]
fn provider_evidence_record_validation_rejects_version_and_integrity_drift() {
    let record = create_sync_evidence_record(&provider_api_evidence()).unwrap();
    assert_eq!(record.version, PROVIDER_EVIDENCE_RECORD_VERSION);
    validate_sync_evidence_record(&record).unwrap();

    let mut unsupported = record.clone();
    unsupported.version += 1;
    assert_eq!(
        validate_sync_evidence_record(&unsupported).unwrap_err(),
        "provider-evidence-record-version-unsupported"
    );

    let mut malformed_id = record.clone();
    malformed_id.record_id = "g".repeat(64);
    assert_eq!(
        validate_sync_evidence_record(&malformed_id).unwrap_err(),
        "provider-evidence-record-integrity-mismatch"
    );

    let mut wrong_digest = record.clone();
    wrong_digest.record_id = "0".repeat(64);
    assert_eq!(
        validate_sync_evidence_record(&wrong_digest).unwrap_err(),
        "provider-evidence-record-integrity-mismatch"
    );

    let mut changed_evidence = record;
    changed_evidence.evidence.observed_bytes += 1;
    assert_eq!(
        validate_sync_evidence_record(&changed_evidence).unwrap_err(),
        "provider-evidence-record-integrity-mismatch"
    );
}
