//! Public-boundary exact-head coverage for iCloud local-eviction batch planning.
//!
//! The batch planner must reject invalid authority/count inputs before filesystem work and must
//! convert a genuinely unavailable iCloud item into bounded, path-free evidence instead of
//! silently treating it as executable.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::cloud_local_eviction_batch::{
    plan_icloud_local_eviction_batch, MAX_BATCH_ITEMS,
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