use disksage_lib::cloud::CloudProvider;
use disksage_lib::cloud_transfer::{
    ProviderSyncEvidence, ProviderSyncState, RemoteChecksumAlgorithm, RemoteContentProof,
    SyncEvidenceKind,
};
use disksage_lib::provider_evidence::{read_immutable_sync_evidence, write_immutable_sync_evidence};

const EXPECTED_MAX_RECORDS_PER_RECEIPT: usize = 128;

fn evidence(confirmed_at_ms: u64) -> ProviderSyncEvidence {
    ProviderSyncEvidence {
        receipt_id: "a".repeat(64),
        provider: CloudProvider::Onedrive,
        destination: "/cloud/report.pdf".into(),
        observed_bytes: 42,
        destination_blake3: "b".repeat(64),
        confirmed_at_ms,
        kind: SyncEvidenceKind::ProviderApi,
        evidence_id: format!("provider-api-{confirmed_at_ms}:{}", "c".repeat(64)),
        sync_complete: true,
        sync_state: ProviderSyncState::Complete,
        remote_content: Some(RemoteContentProof {
            object_id: "remote-id".into(),
            revision: format!("revision-{confirmed_at_ms}"),
            algorithm: RemoteChecksumAlgorithm::QuickXor,
            checksum: format!("quick-xor-{confirmed_at_ms}"),
            location_bound: true,
            location_proof: Some(format!("onedrive-path-v1:{}", "d".repeat(64))),
        }),
    }
}

#[test]
fn recurring_attestation_retains_a_bounded_receipt_history() {
    let directory = tempfile::tempdir().expect("temporary evidence directory");

    for confirmed_at_ms in 1..=(EXPECTED_MAX_RECORDS_PER_RECEIPT as u64 + 2) {
        write_immutable_sync_evidence(directory.path(), &evidence(confirmed_at_ms))
            .expect("bounded recurring evidence write");
    }

    let mut records = std::fs::read_dir(directory.path())
        .expect("read evidence directory")
        .map(|entry| entry.expect("evidence directory entry").path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    records.sort();

    assert_eq!(records.len(), EXPECTED_MAX_RECORDS_PER_RECEIPT);
    assert!(records.iter().all(|path| read_immutable_sync_evidence(path).is_ok()));

    let oldest_remaining = read_immutable_sync_evidence(&records[0])
        .expect("oldest retained evidence must remain valid");
    assert_eq!(oldest_remaining.evidence.confirmed_at_ms, 3);
    let newest = read_immutable_sync_evidence(records.last().expect("newest retained path"))
        .expect("newest retained evidence must remain valid");
    assert_eq!(
        newest.evidence.confirmed_at_ms,
        EXPECTED_MAX_RECORDS_PER_RECEIPT as u64 + 2
    );
}

#[test]
fn clock_regression_never_prunes_the_record_just_written() {
    let directory = tempfile::tempdir().expect("temporary evidence directory");

    for confirmed_at_ms in 100..(100 + EXPECTED_MAX_RECORDS_PER_RECEIPT as u64) {
        write_immutable_sync_evidence(directory.path(), &evidence(confirmed_at_ms))
            .expect("seed bounded evidence history");
    }

    let (written_record, written_path) =
        write_immutable_sync_evidence(directory.path(), &evidence(1))
            .expect("clock-regressed evidence write");

    assert!(
        written_path.exists(),
        "a successful immutable evidence write must not return a path that retention deleted"
    );
    let reread = read_immutable_sync_evidence(&written_path)
        .expect("the just-written evidence must remain readable after retention");
    assert_eq!(reread.record_id, written_record.record_id);
    assert_eq!(reread.evidence.confirmed_at_ms, 1);

    let mut retained_times = std::fs::read_dir(directory.path())
        .expect("read evidence directory")
        .map(|entry| entry.expect("evidence directory entry").path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .map(|path| {
            read_immutable_sync_evidence(&path)
                .expect("retained evidence must remain valid")
                .evidence
                .confirmed_at_ms
        })
        .collect::<Vec<_>>();
    retained_times.sort_unstable();

    assert_eq!(retained_times.len(), EXPECTED_MAX_RECORDS_PER_RECEIPT);
    assert_eq!(retained_times[0], 1);
    assert!(!retained_times.contains(&100));
    assert!(retained_times.contains(&101));
    assert!(retained_times.contains(&227));
}
