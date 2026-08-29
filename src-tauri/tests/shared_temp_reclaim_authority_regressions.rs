#![cfg(unix)]

use disksage_lib::shared_temp_reclaim::{
    approve_shared_temp_reclaim, execute_shared_temp_reclaim, plan_shared_temp_reclaim,
    seal_completed_temp_artifact,
};
use std::fs;

fn artifact(prefix: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in(if cfg!(target_os = "macos") {
            "/private/tmp"
        } else {
            "/tmp"
        })
        .expect("temporary artifact")
}

fn approved_artifact(prefix: &str) -> (tempfile::TempDir, disksage_lib::shared_temp_reclaim::SharedTempReclaimPlan, disksage_lib::shared_temp_reclaim::SharedTempReclaimApproval) {
    let artifact = artifact(prefix);
    fs::write(artifact.path().join("result.bin"), b"same").expect("write payload");
    seal_completed_temp_artifact(artifact.path(), "disksage:test", 10).expect("seal artifact");
    let plan = plan_shared_temp_reclaim(artifact.path(), 11).expect("plan artifact");
    let phrase = plan.exact_approval_phrase.clone().expect("approval phrase");
    let approval = approve_shared_temp_reclaim(
        &plan,
        &phrase,
        12,
        "human:test",
        "verified completed temporary artifact",
    )
    .expect("approve artifact");
    (artifact, plan, approval)
}

#[test]
fn same_size_in_place_overwrite_invalidates_shared_temp_approval() {
    let (artifact, plan, approval) = approved_artifact("disksage-same-size-drift-");
    fs::write(artifact.path().join("result.bin"), b"diff").expect("overwrite payload at same length");

    let private = tempfile::tempdir().expect("private receipt directory");
    let journal = private.path().join("journal.jsonl");
    let receipt = private.path().join("receipt.json");
    let error = execute_shared_temp_reclaim(&plan, &approval, &journal, &receipt, 13)
        .expect_err("same-size content drift must require fresh approval");

    assert_eq!(error, "shared-temp-live-plan-mismatch");
    assert!(artifact.path().exists(), "drifted artifact must not be deleted");
}

#[test]
fn symlink_journal_is_rejected_before_shared_temp_deletion() {
    let (artifact, plan, approval) = approved_artifact("disksage-journal-authority-");
    let private = tempfile::tempdir().expect("private receipt directory");
    let victim = private.path().join("victim.txt");
    fs::write(&victim, b"do-not-touch").expect("write journal redirection victim");
    let journal = private.path().join("journal.jsonl");
    std::os::unix::fs::symlink(&victim, &journal).expect("create journal symlink");
    let receipt = private.path().join("receipt.json");

    let error = execute_shared_temp_reclaim(&plan, &approval, &journal, &receipt, 13)
        .expect_err("journal symlinks must fail before mutation");

    assert_eq!(error, "shared-temp-journal-path-unsafe");
    assert_eq!(fs::read(&victim).unwrap(), b"do-not-touch");
    assert!(artifact.path().exists(), "unsafe journal must not allow deletion");
    assert!(!receipt.exists(), "no success receipt may be created");
}
