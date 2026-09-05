use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::cloud_local_eviction_batch::plan_icloud_local_eviction_batch;
use std::path::PathBuf;

#[test]
fn non_icloud_batch_root_is_rejected_before_item_planning() {
    let root = CloudRoot {
        id: "onedrive:test".into(),
        provider: CloudProvider::OneDrive,
        account_scope: CloudAccountScope::Personal,
        label: "OneDrive test".into(),
        path: "/cloud".into(),
        readable: true,
        access_issue: None,
    };

    let result = plan_icloud_local_eviction_batch(
        &root,
        &[PathBuf::from("/cloud/item.bin")],
        1,
    );

    assert_eq!(
        result.expect_err("iCloud batch boundary must reject non-iCloud roots"),
        "icloud-local-eviction-batch-provider-mismatch"
    );
}
