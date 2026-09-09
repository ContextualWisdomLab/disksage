mod batch_cli_under_test {
    include!("../src/bin/disksage-icloud-local-eviction-batch.rs");

    pub fn selected_provider_result(
        roots: &[disksage_lib::cloud::CloudRoot],
        requested: &std::path::Path,
    ) -> Result<(), String> {
        select_root(roots, requested).map(|_| ())
    }
}

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use std::path::Path;

#[cfg(not(windows))]
const CLOUD_ROOT: &str = "/Cloud";
#[cfg(windows)]
const CLOUD_ROOT: &str = r"C:\Cloud";

#[test]
fn batch_cli_rejects_non_icloud_provider_before_planning() {
    let roots = vec![CloudRoot {
        id: "onedrive-personal".into(),
        provider: CloudProvider::Onedrive,
        account_scope: CloudAccountScope::Personal,
        label: "OneDrive".into(),
        path: CLOUD_ROOT.into(),
        readable: true,
        access_issue: None,
    }];

    assert_eq!(
        batch_cli_under_test::selected_provider_result(&roots, Path::new(CLOUD_ROOT)).unwrap_err(),
        "icloud-local-eviction-root-required"
    );
}
