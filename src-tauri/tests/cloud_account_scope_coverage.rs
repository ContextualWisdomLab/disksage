//! Credential-free account-scope coverage for synthetic cloud-provider roots.
//!
//! These fixtures exercise provider classification only. They never contact a provider, inspect
//! file contents, or authorize any cloud mutation.

use disksage_lib::cloud::{
    discover_cloud_roots_report, CloudAccountScope, CloudProvider,
};

#[test]
fn synthetic_provider_accounts_cover_personal_organization_unknown_and_shared_scopes() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let cloud_storage = home.join("Library/CloudStorage");
    std::fs::create_dir_all(&cloud_storage).unwrap();

    for account in ["personal", "default", "teamspace"] {
        std::fs::create_dir(cloud_storage.join(format!("OneDrive-{account}"))).unwrap();
    }

    for account in ["user@corp.example", "user@internal", "user@gmail.com"] {
        let account_root = cloud_storage.join(format!("GoogleDrive-{account}"));
        std::fs::create_dir_all(&account_root).unwrap();
        std::fs::create_dir(account_root.join("My Drive")).unwrap();
        if account == "user@gmail.com" {
            std::fs::create_dir(account_root.join("공유 드라이브")).unwrap();
        }
    }

    let report = discover_cloud_roots_report(home);
    assert!(report.issues.is_empty(), "{:?}", report.issues);

    let scope_for = |provider: CloudProvider, label_fragment: &str| {
        report
            .roots
            .iter()
            .find(|root| root.provider == provider && root.label.contains(label_fragment))
            .map(|root| root.account_scope)
            .unwrap_or_else(|| panic!("missing synthetic root for {label_fragment}"))
    };

    assert_eq!(
        scope_for(CloudProvider::Onedrive, "personal"),
        CloudAccountScope::Personal
    );
    assert_eq!(
        scope_for(CloudProvider::Onedrive, "default"),
        CloudAccountScope::Unknown
    );
    assert_eq!(
        scope_for(CloudProvider::Onedrive, "teamspace"),
        CloudAccountScope::Organization
    );
    assert_eq!(
        scope_for(CloudProvider::GoogleDrive, "user@corp.example · My Drive"),
        CloudAccountScope::Organization
    );
    assert_eq!(
        scope_for(CloudProvider::GoogleDrive, "user@internal · My Drive"),
        CloudAccountScope::Unknown
    );
    assert_eq!(
        scope_for(CloudProvider::GoogleDrive, "user@gmail.com · 공유 드라이브"),
        CloudAccountScope::Shared
    );
}
