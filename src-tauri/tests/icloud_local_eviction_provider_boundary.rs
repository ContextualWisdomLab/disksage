#![cfg(not(coverage))]

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use std::fs;

#[test]
fn icloud_eviction_rejects_a_non_icloud_root_before_provider_observation() {
    let root_dir = tempfile::tempdir().expect("temporary cloud root");
    let target = root_dir.path().join("customer-owned.bin");
    fs::write(&target, b"customer data").expect("write target fixture");
    let root = CloudRoot {
        id: "onedrive:test".into(),
        provider: CloudProvider::Onedrive,
        account_scope: CloudAccountScope::Personal,
        label: "OneDrive fixture".into(),
        path: root_dir.path().to_string_lossy().into_owned(),
        readable: true,
        access_issue: None,
    };

    let error = disksage_lib::cloud_local_eviction::plan_icloud_local_eviction(
        &root,
        &target,
        1_000,
    )
    .expect_err("provider-specific iCloud eviction must reject a OneDrive root");

    assert_eq!(error, "icloud-local-eviction-provider-mismatch");
}
