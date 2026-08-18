//! Public-boundary coverage for provider evidence limits and accepted identity shapes.
//!
//! The tests exercise exact admission boundaries without writing provider evidence
//! to disk or invoking a provider/network path.

use disksage_lib::cloud::CloudProvider;
use disksage_lib::cloud_transfer::{
    ProviderSyncEvidence, RemoteChecksumAlgorithm, RemoteContentProof, SyncEvidenceKind,
};
use disksage_lib::provider_evidence::{
    create_sync_evidence_record, validate_sync_evidence_record,
};

const MAX_DESTINATION_BYTES: usize = 32 * 1024;
const MAX_EVIDENCE_ID_BYTES: usize = 1_024;

fn exact_max_destination() -> String {
    #[cfg(windows)]
    {
        let value = format!("C:\\{}", "d".repeat(MAX_DESTINATION_BYTES - 3));
        assert_eq!(value.len(), MAX_DESTINATION_BYTES);
        value
    }
    #[cfg(not(windows))]
    {
        let value = format!("/{}", "d".repeat(MAX_DESTINATION_BYTES - 1));
        assert_eq!(value.len(), MAX_DESTINATION_BYTES);
        value
    }
}

fn api_evidence() -> ProviderSyncEvidence {
    ProviderSyncEvidence {
        receipt_id: "A".repeat(64),
        provider: CloudProvider::Onedrive,
        destination: exact_max_destination(),
        observed_bytes: 42,
        destination_blake3: "B".repeat(64),
        confirmed_at_ms: 30,
        kind: SyncEvidenceKind::ProviderApi,
        evidence_id: "e".repeat(MAX_EVIDENCE_ID_BYTES),
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
fn provider_evidence_accepts_exact_size_limits_and_uppercase_hex_identity() {
    let evidence = api_evidence();
    assert_eq!(evidence.destination.len(), MAX_DESTINATION_BYTES);
    assert_eq!(evidence.evidence_id.len(), MAX_EVIDENCE_ID_BYTES);

    let record = create_sync_evidence_record(&evidence)
        .expect("exact public limits and uppercase hexadecimal identity must remain admissible");
    validate_sync_evidence_record(&record).expect("fresh exact-limit evidence record must validate");
}

#[test]
fn provider_evidence_rejects_one_byte_over_each_public_limit() {
    let mut oversized_destination = api_evidence();
    oversized_destination.destination.push('x');
    assert_eq!(
        create_sync_evidence_record(&oversized_destination).unwrap_err(),
        "provider-evidence-destination-invalid"
    );

    let mut oversized_id = api_evidence();
    oversized_id.evidence_id.push('x');
    assert_eq!(
        create_sync_evidence_record(&oversized_id).unwrap_err(),
        "provider-evidence-id-invalid"
    );
}

#[test]
fn provider_evidence_rejects_control_character_at_the_exact_id_limit() {
    let mut evidence = api_evidence();
    evidence.evidence_id.replace_range(MAX_EVIDENCE_ID_BYTES - 1..MAX_EVIDENCE_ID_BYTES, "\n");
    assert_eq!(evidence.evidence_id.len(), MAX_EVIDENCE_ID_BYTES);
    assert_eq!(
        create_sync_evidence_record(&evidence).unwrap_err(),
        "provider-evidence-id-invalid"
    );
}
