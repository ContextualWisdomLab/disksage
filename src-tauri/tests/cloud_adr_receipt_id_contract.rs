use disksage_lib::cloud_adr::{
    write_latest_goal_snapshot, write_latest_snapshot, CloudOffloadAdrSnapshot,
    CloudOffloadGoalSnapshot, CLOUD_ADR_SCHEMA_VERSION, CLOUD_GOAL_SCHEMA_VERSION,
};
use disksage_lib::cloud_transfer::{CloudOffloadGoalState, ProviderSyncState};
use std::collections::BTreeMap;

fn invalid_adr_snapshot(receipt_id: &str) -> CloudOffloadAdrSnapshot {
    CloudOffloadAdrSnapshot {
        schema_version: CLOUD_ADR_SCHEMA_VERSION,
        adr_id: "cloud-offload:test".into(),
        receipt_id: receipt_id.into(),
        goal_state: CloudOffloadGoalState::CopyVerified,
        provider_sync_state: ProviderSyncState::Unknown,
        sync_complete: false,
        decision: "retain-source-after-copy".into(),
        consequences: vec!["source-retained".into()],
        evidence_record_id: "b".repeat(64),
        updated_at_ms: 1,
    }
}

fn invalid_goal_snapshot(receipt_id: &str) -> CloudOffloadGoalSnapshot {
    CloudOffloadGoalSnapshot {
        schema_version: CLOUD_GOAL_SCHEMA_VERSION,
        goal_id: "disksage-cloud-offload".into(),
        status: "active".into(),
        receipt_id: receipt_id.into(),
        goal_state: CloudOffloadGoalState::CopyVerified,
        provider_sync_state: ProviderSyncState::Unknown,
        completion_gates: BTreeMap::new(),
        safety_invariant: "source-retained-until-an-explicit-trash-step".into(),
        evidence_record_id: None,
        updated_at_ms: 1,
    }
}

#[test]
fn latest_adr_snapshot_rejects_non_hex_receipt_id_before_path_construction() {
    let directory = tempfile::tempdir().expect("temporary ADR directory");
    let snapshot = invalid_adr_snapshot("../escape");

    let error = write_latest_snapshot(directory.path(), &snapshot)
        .expect_err("path-shaped receipt identifiers must fail closed");

    assert_eq!(error, "cloud-adr-receipt-id-invalid");
    assert_eq!(
        std::fs::read_dir(directory.path())
            .expect("read ADR directory")
            .count(),
        0,
        "invalid receipt identifiers must not create temporary or final files"
    );
}

#[test]
fn latest_goal_snapshot_rejects_non_hex_receipt_id_before_path_construction() {
    let directory = tempfile::tempdir().expect("temporary Goal directory");
    let snapshot = invalid_goal_snapshot("not-a-64-character-hex-receipt-id");

    let error = write_latest_goal_snapshot(directory.path(), &snapshot)
        .expect_err("untrusted receipt identifiers must fail closed");

    assert_eq!(error, "cloud-goal-receipt-id-invalid");
    assert_eq!(
        std::fs::read_dir(directory.path())
            .expect("read Goal directory")
            .count(),
        0,
        "invalid receipt identifiers must not create temporary or final files"
    );
}
