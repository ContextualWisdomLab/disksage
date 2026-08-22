//! Public-boundary exact-head coverage for iCloud local-eviction batch planning and execution.
//!
//! The batch planner must reject invalid authority/count inputs before filesystem work, unavailable
//! items must become bounded path-free evidence, and the public approval/execution boundaries must
//! reject plans that are incomplete or otherwise non-authoritative before they can create records.

#![cfg(not(coverage))]

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::cloud_local_eviction_batch::{
    approve_icloud_local_eviction_batch, execute_icloud_local_eviction_batch,
    plan_icloud_local_eviction_batch, IcloudLocalEvictionBatchApproval,
    IcloudLocalEvictionBatchPlan, ICLOUD_LOCAL_EVICTION_BATCH_VERSION, MAX_BATCH_ITEMS,
};
use std::path::PathBuf;

fn cloud_root(provider: CloudProvider, path: String) -> CloudRoot {
    CloudRoot {
        id: format!("{}:coverage", provider.as_str()),
        provider,
        account_scope: CloudAccountScope::Personal,
        label: "coverage root".into(),
        path,
        readable: true,
        access_issue: None,
    }
}

#[test]
fn public_batch_planner_rejects_non_icloud_authority_before_path_probe() {
    let root = cloud_root(CloudProvider::GoogleDrive, "/not-a-real-cloud-root".into());
    let paths = vec![PathBuf::from("/not-a-real-cloud-root/item.bin")];

    let error = plan_icloud_local_eviction_batch(&root, &paths, 70_001)
        .expect_err("non-iCloud authority must fail before filesystem planning");

    assert_eq!(error, "icloud-local-eviction-batch-requires-icloud-root");
}

#[test]
fn public_batch_planner_rejects_empty_and_oversized_manifests_before_path_probe() {
    let root = cloud_root(CloudProvider::Icloud, "/not-a-real-icloud-root".into());

    let empty_error = plan_icloud_local_eviction_batch(&root, &[], 70_002)
        .expect_err("an empty manifest cannot authorize a batch");
    assert_eq!(empty_error, "icloud-local-eviction-batch-input-count-invalid");

    let oversized: Vec<_> = (0..=MAX_BATCH_ITEMS)
        .map(|index| PathBuf::from(format!("/not-a-real-icloud-root/item-{index}.bin")))
        .collect();
    let oversized_error = plan_icloud_local_eviction_batch(&root, &oversized, 70_003)
        .expect_err("an oversized manifest must fail before filesystem planning");
    assert_eq!(
        oversized_error,
        "icloud-local-eviction-batch-input-count-invalid"
    );
}

#[test]
fn public_batch_planner_rejects_duplicate_manifest_paths_before_path_probe() {
    let root = cloud_root(CloudProvider::Icloud, "/not-a-real-icloud-root".into());
    let repeated = PathBuf::from("/not-a-real-icloud-root/reviewed-item.bin");
    let paths = vec![repeated.clone(), repeated];

    let error = plan_icloud_local_eviction_batch(&root, &paths, 70_003)
        .expect_err("one manifest path must not be represented twice in one human approval scope");

    assert_eq!(error, "icloud-local-eviction-batch-duplicate-input-path");
}

#[test]
fn public_batch_planner_projects_missing_item_as_bounded_unavailable_evidence() {
    let root_dir = tempfile::tempdir().expect("temporary iCloud-shaped root");
    let missing = root_dir.path().join("missing-reviewed-item.bin");
    let root = cloud_root(
        CloudProvider::Icloud,
        root_dir.path().to_string_lossy().into_owned(),
    );

    let plan = plan_icloud_local_eviction_batch(&root, std::slice::from_ref(&missing), 70_004)
        .expect("unavailable items remain represented in a read-only batch plan");

    assert_eq!(plan.input_count, 1);
    assert_eq!(plan.planned_count, 0);
    assert_eq!(plan.unavailable_count, 1);
    assert!(!plan.eligible_after_human_approval);
    assert_eq!(
        plan.blockers,
        vec!["icloud-local-eviction-batch-has-no-planned-items"]
    );
    assert_eq!(plan.unavailable[0].input_index, 0);
    let missing_display = missing.to_string_lossy();
    assert!(
        !plan.unavailable[0]
            .error_code
            .contains(missing_display.as_ref()),
        "unavailable evidence must not disclose the reviewed path"
    );
    assert!(
        plan.unavailable[0].error_code.len() <= 128,
        "unavailable evidence must stay bounded"
    );
}

#[test]
fn public_batch_approval_rejects_a_valid_but_noneligible_unavailable_only_plan() {
    let root_dir = tempfile::tempdir().expect("temporary iCloud-shaped root");
    let missing = root_dir.path().join("missing-reviewed-item.bin");
    let root = cloud_root(
        CloudProvider::Icloud,
        root_dir.path().to_string_lossy().into_owned(),
    );
    let plan = plan_icloud_local_eviction_batch(&root, std::slice::from_ref(&missing), 70_005)
        .expect("missing input should produce a valid but non-executable evidence record");

    assert!(!plan.eligible_after_human_approval);
    let error = approve_icloud_local_eviction_batch(
        &plan,
        &root,
        &plan.batch_fingerprint,
        70_006,
        "human:coverage-operator",
        "Reviewed the exact unavailable-only plan",
    )
    .expect_err("human attribution cannot make an evidence-incomplete batch executable");

    assert_eq!(error, "icloud-local-eviction-batch-fingerprint-mismatch");
}

#[test]
fn public_batch_execution_rejects_invalid_plan_before_record_publication() {
    let record_parent = tempfile::tempdir().expect("temporary record parent");
    let record_dir = record_parent.path().join("records-that-must-not-be-created");
    let root = cloud_root(
        CloudProvider::Icloud,
        record_parent.path().join("icloud-root").to_string_lossy().into_owned(),
    );
    let invalid_plan = IcloudLocalEvictionBatchPlan {
        version: ICLOUD_LOCAL_EVICTION_BATCH_VERSION,
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        cloud_root: root.path.clone(),
        observed_at_ms: 70_007,
        input_count: 0,
        planned_count: 0,
        unavailable_count: 0,
        total_logical_bytes: 0,
        total_allocated_bytes: 0,
        items: Vec::new(),
        unavailable: Vec::new(),
        batch_fingerprint: "0".repeat(64),
        eligible_after_human_approval: false,
        blockers: Vec::new(),
        notices: Vec::new(),
    };
    let approval = IcloudLocalEvictionBatchApproval {
        version: ICLOUD_LOCAL_EVICTION_BATCH_VERSION,
        approval_id: "1".repeat(64),
        batch_fingerprint: invalid_plan.batch_fingerprint.clone(),
        approved_at_ms: 70_008,
        approved_by: "human:coverage-operator".into(),
        rationale: "Reject invalid batch before publication".into(),
    };

    let error = execute_icloud_local_eviction_batch(
        &root,
        &invalid_plan,
        &approval,
        &invalid_plan.batch_fingerprint,
        &record_dir,
        70_009,
    )
    .expect_err("an invalid plan must fail before record publication");

    assert_eq!(error, "icloud-local-eviction-batch-plan-shape-invalid");
    assert!(
        !record_dir.exists(),
        "invalid execution input must not create an authority-record directory"
    );
}
