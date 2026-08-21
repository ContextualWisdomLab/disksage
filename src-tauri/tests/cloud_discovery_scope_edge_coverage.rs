//! Deterministic branch coverage for cloud-provider discovery and account-scope classification.
//!
//! The fixture is entirely local. It creates synthetic provider-shaped directories only and never
//! reads a real cloud account, invokes a provider API, or mutates user data.

#![cfg(not(coverage))]

use disksage_lib::cloud::{
    discover_cloud_roots_report, CloudAccountScope, CloudProvider, CloudRootDiscoveryReport,
};
use std::fs;
use std::path::Path;

fn scope_for_label(
    report: &CloudRootDiscoveryReport,
    provider: CloudProvider,
    label_fragment: &str,
) -> CloudAccountScope {
    report
        .roots
        .iter()
        .find(|root| root.provider == provider && root.label.contains(label_fragment))
        .unwrap_or_else(|| panic!("missing {provider:?} root containing {label_fragment:?}"))
        .account_scope
}

#[test]
fn discovery_classifies_personal_organization_shared_and_unknown_accounts() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let cloud_storage = home.join("Library/CloudStorage");
    fs::create_dir_all(&cloud_storage).unwrap();

    for account in ["PERSONAL", "consumer", "개인", "alice@outlook.com"] {
        fs::create_dir(cloud_storage.join(format!("OneDrive-{account}"))).unwrap();
    }
    fs::create_dir(cloud_storage.join("OneDrive-user@corp.example")).unwrap();
    fs::create_dir(cloud_storage.join("OneDrive-")).unwrap();

    for (account, drive) in [
        ("person@googlemail.com", "My Drive"),
        ("person@gmail.com", "Shared Drive"),
        ("person@gmail.com", "Shared Drives"),
        ("person@gmail.com", "공유 드라이브"),
        ("person@corp.example", "My Drive"),
        ("person@localhost", "My Drive"),
    ] {
        fs::create_dir_all(
            cloud_storage
                .join(format!("GoogleDrive-{account}"))
                .join(drive),
        )
        .unwrap();
    }

    fs::create_dir(home.join("OneDrive")).unwrap();
    fs::create_dir(home.join("Google Drive")).unwrap();

    let report = discover_cloud_roots_report(home);

    for label in ["PERSONAL", "consumer", "개인", "alice@outlook.com"] {
        assert_eq!(
            scope_for_label(&report, CloudProvider::Onedrive, label),
            CloudAccountScope::Personal
        );
    }
    assert_eq!(
        scope_for_label(&report, CloudProvider::Onedrive, "user@corp.example"),
        CloudAccountScope::Organization
    );
    assert_eq!(
        scope_for_label(&report, CloudProvider::Onedrive, "default"),
        CloudAccountScope::Unknown
    );
    assert_eq!(
        scope_for_label(&report, CloudProvider::Onedrive, "OneDrive · OneDrive"),
        CloudAccountScope::Unknown
    );

    assert_eq!(
        scope_for_label(&report, CloudProvider::GoogleDrive, "person@googlemail.com · My Drive"),
        CloudAccountScope::Personal
    );
    assert_eq!(
        scope_for_label(&report, CloudProvider::GoogleDrive, "person@corp.example · My Drive"),
        CloudAccountScope::Organization
    );
    assert_eq!(
        scope_for_label(&report, CloudProvider::GoogleDrive, "person@localhost · My Drive"),
        CloudAccountScope::Unknown
    );
    for drive in ["Shared Drive", "Shared Drives", "공유 드라이브"] {
        assert_eq!(
            scope_for_label(
                &report,
                CloudProvider::GoogleDrive,
                &format!("person@gmail.com · {drive}")
            ),
            CloudAccountScope::Shared
        );
    }
    assert_eq!(
        scope_for_label(&report, CloudProvider::GoogleDrive, "Google Drive · Google Drive"),
        CloudAccountScope::Unknown
    );
}

#[test]
fn discovery_reports_read_only_and_malformed_provider_shapes_without_authorizing_them() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let cloud_storage = home.join("Library/CloudStorage");
    fs::create_dir_all(&cloud_storage).unwrap();

    let read_only = cloud_storage.join("OneDrive-readonly@corp.example");
    fs::create_dir(&read_only).unwrap();
    let mut permissions = fs::metadata(&read_only).unwrap().permissions();
    let original_permissions = permissions.clone();
    permissions.set_readonly(true);
    fs::set_permissions(&read_only, permissions).unwrap();

    let broken_google = cloud_storage.join("GoogleDrive-broken@gmail.com");
    fs::write(&broken_google, b"not a directory").unwrap();

    let report = discover_cloud_roots_report(home);

    assert!(!report
        .roots
        .iter()
        .any(|root| Path::new(&root.path) == read_only));
    assert!(report.issues.iter().any(|issue| {
        issue.provider == Some(CloudProvider::Onedrive)
            && Path::new(&issue.path) == read_only
            && issue.reason == "read-only"
    }));
    assert!(report.issues.iter().any(|issue| {
        issue.provider == Some(CloudProvider::GoogleDrive)
            && Path::new(&issue.path) == broken_google
            && issue.reason == "not-a-directory"
    }));

    fs::set_permissions(&read_only, original_permissions).unwrap();
}

#[cfg(unix)]
#[test]
fn discovery_deduplicates_two_spellings_of_the_same_icloud_directory() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    let canonical = home.join("Library/Mobile Documents/com~apple~CloudDocs");
    fs::create_dir_all(&canonical).unwrap();
    symlink(&canonical, home.join("iCloudDrive")).unwrap();

    let report = discover_cloud_roots_report(home);
    let icloud_roots: Vec<_> = report
        .roots
        .iter()
        .filter(|root| root.provider == CloudProvider::Icloud)
        .collect();

    assert_eq!(icloud_roots.len(), 1);
    assert_eq!(icloud_roots[0].account_scope, CloudAccountScope::Unknown);
}
