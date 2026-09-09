#![cfg(unix)]

use disksage_lib::shared_temp_reclaim::{
    execute_shared_temp_reclaim, plan_shared_temp_reclaim, seal_completed_temp_artifact,
    SharedTempReclaimApproval, SHARED_TEMP_RECLAIM_VERSION,
};

#[test]
fn permanent_shared_temp_reclaim_stays_fail_closed_until_object_bound() {
    let artifact = tempfile::Builder::new()
        .prefix("disksage-reclaim-fail-closed-")
        .tempdir_in(if cfg!(target_os = "macos") {
            "/private/tmp"
        } else {
            "/tmp"
        })
        .unwrap();
    std::fs::write(artifact.path().join("result.bin"), vec![1_u8; 16 * 1024]).unwrap();
    seal_completed_temp_artifact(artifact.path(), "disksage:test", 10).unwrap();
    let plan = plan_shared_temp_reclaim(artifact.path(), 11).unwrap();
    assert!(!plan.eligible_after_human_approval, "{:?}", plan.blockers);
    assert!(plan.exact_approval_phrase.is_none());
    let approval = SharedTempReclaimApproval {
        version: SHARED_TEMP_RECLAIM_VERSION,
        approval_id: "forged".into(),
        plan_fingerprint: plan.plan_fingerprint.clone(),
        exact_approval_phrase: "forged".into(),
        approved_at_ms: 12,
        approved_by: "human:forged".into(),
        rationale: "must not grant mutation authority".into(),
    };

    let artifact_path = artifact.keep();
    let private = tempfile::tempdir().unwrap();
    let journal_path = private.path().join("journal.jsonl");
    let receipt_path = private.path().join("receipt.json");

    assert_eq!(
        execute_shared_temp_reclaim(&plan, &approval, &journal_path, &receipt_path, 13)
            .unwrap_err(),
        "shared-temp-permanent-execution-disabled"
    );
    assert!(artifact_path.exists());
    assert!(!journal_path.exists());
    assert!(!receipt_path.exists());

    std::fs::remove_dir_all(artifact_path).unwrap();
}
