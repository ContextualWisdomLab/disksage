//! Public-contract coverage for deterministic cloud-root validation.
//!
//! The tests use temporary local directories only. They do not discover real cloud accounts,
//! invoke provider APIs, mutate files, or require credentials.

use disksage_lib::cloud::{
    cloud_root_path_matches, discover_cloud_roots, discover_cloud_roots_report,
    prepare_cloud_archive_source, validate_cloud_root_readable, validate_source_root_readable,
    CloudAccountScope, CloudPlanOptions, CloudProvider, CloudRoot, ContentMetadata, FileFact,
};
use unicode_normalization::UnicodeNormalization;

fn cloud_root(path: &std::path::Path, readable: bool, access_issue: Option<&str>) -> CloudRoot {
    CloudRoot {
        id: "coverage-root".into(),
        provider: CloudProvider::GoogleDrive,
        account_scope: CloudAccountScope::Unknown,
        label: "Coverage Root".into(),
        path: path.to_string_lossy().into_owned(),
        readable,
        access_issue: access_issue.map(str::to_owned),
    }
}

#[test]
fn source_and_destination_roots_fail_closed_before_planning() {
    let temp = tempfile::tempdir().unwrap();
    let directory = temp.path().join("cloud-root");
    std::fs::create_dir(&directory).unwrap();
    let regular_file = temp.path().join("not-a-directory");
    std::fs::write(&regular_file, b"content").unwrap();
    let missing = temp.path().join("missing");

    assert!(validate_source_root_readable(&directory).is_ok());
    assert!(validate_source_root_readable(&regular_file)
        .unwrap_err()
        .starts_with("source-root-not-directory:"));
    assert!(validate_source_root_readable(&missing)
        .unwrap_err()
        .starts_with("source-root-not-directory:"));

    assert!(validate_cloud_root_readable(&cloud_root(&directory, true, None)).is_ok());
    assert_eq!(
        validate_cloud_root_readable(&cloud_root(
            &directory,
            false,
            Some("permission-denied")
        ))
        .unwrap_err(),
        format!(
            "cloud-root-unreadable:{}:permission-denied",
            directory.to_string_lossy()
        )
    );
    assert_eq!(
        validate_cloud_root_readable(&cloud_root(&directory, false, None)).unwrap_err(),
        format!(
            "cloud-root-unreadable:{}:not-verified",
            directory.to_string_lossy()
        )
    );
    assert!(validate_cloud_root_readable(&cloud_root(&missing, true, None))
        .unwrap_err()
        .starts_with("cloud-root-unreadable:"));
}

#[test]
fn cloud_root_matching_handles_exact_canonical_and_unicode_equivalent_paths() {
    let temp = tempfile::tempdir().unwrap();
    let real = temp.path().join("real");
    std::fs::create_dir(&real).unwrap();
    let alias = temp.path().join("alias");

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        assert!(cloud_root_path_matches(&real, &alias));
    }

    assert!(cloud_root_path_matches(&real, &real));
    assert!(!cloud_root_path_matches(
        &real,
        &temp.path().join("different")
    ));

    let composed = temp.path().join("Café");
    let decomposed_text = "Café".nfd().collect::<String>();
    let decomposed = temp.path().join(decomposed_text);
    assert_ne!(composed, decomposed);
    assert!(cloud_root_path_matches(&composed, &decomposed));
}

#[test]
fn provider_and_scope_wire_values_remain_stable() {
    assert_eq!(CloudProvider::Icloud.as_str(), "icloud");
    assert_eq!(CloudProvider::Onedrive.as_str(), "onedrive");
    assert_eq!(CloudProvider::GoogleDrive.as_str(), "google-drive");

    assert_eq!(CloudAccountScope::Personal.as_str(), "personal");
    assert_eq!(CloudAccountScope::Organization.as_str(), "organization");
    assert_eq!(CloudAccountScope::Shared.as_str(), "shared");
    assert_eq!(CloudAccountScope::Unknown.as_str(), "unknown");
}

#[test]
fn source_snapshot_applies_selection_policy_and_reports_totals() {
    const DAY_MS: u64 = 24 * 60 * 60 * 1_000;

    let temp = tempfile::tempdir().unwrap();
    let source_root = temp.path().join("source");
    std::fs::create_dir(&source_root).unwrap();
    let now_ms = 10 * DAY_MS;
    let prepared_metadata = ContentMetadata {
        title: Some("coverage fixture".into()),
        ..ContentMetadata::default()
    };
    let file = |path: std::path::PathBuf, bytes: u64, modified_ms: u64| FileFact {
        path,
        bytes,
        created_ms: modified_ms,
        modified_ms,
        content_metadata: prepared_metadata.clone(),
    };
    let files = vec![
        file(source_root.join("eligible.pdf"), 20, now_ms - 3 * DAY_MS),
        file(source_root.join("too-small.pdf"), 9, now_ms - 3 * DAY_MS),
        file(source_root.join("too-young.pdf"), 20, now_ms - DAY_MS),
        file(source_root.join("missing-date.pdf"), 20, 0),
        file(source_root.join("unsupported.rs"), 20, now_ms - 3 * DAY_MS),
        file(temp.path().join("outside.pdf"), 20, now_ms - 3 * DAY_MS),
    ];
    let options = CloudPlanOptions {
        min_size_bytes: 10,
        min_age_days: 2,
        limit: 7,
    };

    let snapshot = prepare_cloud_archive_source(&files, &source_root, now_ms, options);
    assert_eq!(snapshot.candidate_count(), 1);
    assert_eq!(snapshot.candidate_bytes(), 20);

    let defaults = CloudPlanOptions::default();
    assert_eq!(defaults.min_size_bytes, 256 * 1024 * 1024);
    assert_eq!(defaults.min_age_days, 90);
    assert_eq!(defaults.limit, 200);
}

#[test]
fn discovery_classifies_synthetic_provider_roots_without_touching_real_accounts() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();

    std::fs::create_dir_all(home.join("Library/Mobile Documents/com~apple~CloudDocs")).unwrap();
    std::fs::create_dir(home.join("iCloudDrive")).unwrap();

    let cloud_storage = home.join("Library/CloudStorage");
    std::fs::create_dir_all(cloud_storage.join("OneDrive-alice@outlook.com")).unwrap();
    let google = cloud_storage.join("GoogleDrive-user@gmail.com");
    std::fs::create_dir_all(google.join("My Drive")).unwrap();
    std::fs::create_dir(google.join("Shared Drives")).unwrap();
    std::fs::create_dir(google.join(".hidden")).unwrap();

    std::fs::create_dir(home.join("OneDrive - Contoso")).unwrap();
    std::fs::create_dir(home.join("Google Drive")).unwrap();

    let report = discover_cloud_roots_report(home);
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert!(report.roots.iter().all(|root| root.readable));
    assert!(report.roots.iter().all(|root| root.access_issue.is_none()));

    assert!(report.roots.iter().any(|root| {
        root.provider == CloudProvider::Icloud
            && root.account_scope == CloudAccountScope::Unknown
            && root.label == "iCloud Drive"
    }));
    assert!(report.roots.iter().any(|root| {
        root.provider == CloudProvider::Onedrive
            && root.account_scope == CloudAccountScope::Personal
            && root.label.contains("alice@outlook.com")
    }));
    assert!(report.roots.iter().any(|root| {
        root.provider == CloudProvider::Onedrive
            && root.account_scope == CloudAccountScope::Organization
            && root.label.contains("OneDrive - Contoso")
    }));
    assert!(report.roots.iter().any(|root| {
        root.provider == CloudProvider::GoogleDrive
            && root.account_scope == CloudAccountScope::Personal
            && root.label.ends_with("My Drive")
    }));
    assert!(report.roots.iter().any(|root| {
        root.provider == CloudProvider::GoogleDrive
            && root.account_scope == CloudAccountScope::Shared
            && root.label.ends_with("Shared Drives")
    }));
    assert!(!report.roots.iter().any(|root| root.label.contains(".hidden")));

    let roots = discover_cloud_roots(home);
    assert_eq!(roots, report.roots);
}

#[test]
fn discovery_reports_non_directory_provider_candidates_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    std::fs::create_dir_all(home.join("Library/CloudStorage")).unwrap();
    std::fs::write(home.join("OneDrive"), b"not-a-directory").unwrap();

    let report = discover_cloud_roots_report(home);
    assert!(report.roots.iter().all(|root| root.path != home.join("OneDrive").to_string_lossy()));
    assert!(report.issues.iter().any(|issue| {
        issue.provider == Some(CloudProvider::Onedrive) && issue.reason == "not-a-directory"
    }));
}
