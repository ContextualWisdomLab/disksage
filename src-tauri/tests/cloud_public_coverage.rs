//! Public-contract coverage for deterministic cloud-root validation.
//!
//! The tests use temporary local directories only. They do not discover real cloud accounts,
//! invoke provider APIs, mutate files, or require credentials.

#![cfg(not(coverage))]

use disksage_lib::cloud::{
    cloud_root_path_matches, discover_cloud_roots, discover_cloud_roots_report,
    plan_cloud_archive_from_snapshot, prepare_cloud_archive_source, validate_cloud_root_readable,
    validate_source_root_readable, CloudAccountScope, CloudPlanOptions, CloudProvider, CloudRoot,
    ContentMetadata, FileFact,
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

#[cfg(unix)]
#[test]
fn cloud_root_matching_rejects_distinct_non_utf8_fallback_paths() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::path::PathBuf;

    let discovered = PathBuf::from(OsString::from_vec(vec![b'/', b't', b'm', b'p', b'/', 0xff]));
    let requested = PathBuf::from(OsString::from_vec(vec![b'/', b't', b'm', b'p', b'/', 0xfe]));

    assert_ne!(discovered, requested);
    assert!(!cloud_root_path_matches(&discovered, &requested));
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
fn source_snapshot_recognizes_supported_archive_families_without_io() {
    const DAY_MS: u64 = 24 * 60 * 60 * 1_000;

    let temp = tempfile::tempdir().unwrap();
    let source_root = temp.path().join("source");
    std::fs::create_dir(&source_root).unwrap();
    let now_ms = 20 * DAY_MS;
    let modified_ms = now_ms - DAY_MS;
    let metadata = ContentMetadata::default();
    let file = |name: &str| FileFact {
        path: source_root.join(name),
        bytes: 1,
        created_ms: modified_ms,
        modified_ms,
        content_metadata: metadata.clone(),
    };

    let files = vec![
        file("document.docx"),
        file("media.mp4"),
        file("archive.zip"),
        file("dataset.csv"),
        file("backup.bak"),
        file("creative.psd"),
        file("unsupported.rs"),
    ];
    let snapshot = prepare_cloud_archive_source(
        &files,
        &source_root,
        now_ms,
        CloudPlanOptions {
            min_size_bytes: 1,
            min_age_days: 0,
            limit: files.len(),
        },
    );

    assert_eq!(snapshot.candidate_count(), 6);
    assert_eq!(snapshot.candidate_bytes(), 6);
}

#[test]
fn destination_plan_enforces_the_candidate_limit_after_source_admission() {
    const DAY_MS: u64 = 24 * 60 * 60 * 1_000;

    let temp = tempfile::tempdir().unwrap();
    let source_root = temp.path().join("source");
    let destination_root = temp.path().join("cloud");
    std::fs::create_dir(&source_root).unwrap();
    std::fs::create_dir(&destination_root).unwrap();
    let now_ms = 20 * DAY_MS;
    let modified_ms = now_ms - DAY_MS;
    let prepared_metadata = ContentMetadata {
        title: Some("coverage fixture".into()),
        ..ContentMetadata::default()
    };
    let file = |name: &str| FileFact {
        path: source_root.join(name),
        bytes: 2,
        created_ms: modified_ms,
        modified_ms,
        content_metadata: prepared_metadata.clone(),
    };
    let files = vec![file("one.pdf"), file("two.pdf"), file("three.pdf")];

    let snapshot = prepare_cloud_archive_source(
        &files,
        &source_root,
        now_ms,
        CloudPlanOptions {
            min_size_bytes: 1,
            min_age_days: 0,
            limit: 2,
        },
    );

    assert_eq!(snapshot.candidate_count(), 3);
    assert_eq!(snapshot.candidate_bytes(), 6);

    let report = plan_cloud_archive_from_snapshot(
        &snapshot,
        &cloud_root(&destination_root, true, None),
    );
    assert_eq!(report.candidates.len(), 2);
    assert_eq!(report.candidate_bytes, 4);
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

#[cfg(unix)]
#[test]
fn discovery_rejects_read_only_provider_roots_fail_closed() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    std::fs::create_dir_all(home.join("Library/CloudStorage")).unwrap();
    let provider_root = home.join("OneDrive");
    std::fs::create_dir(&provider_root).unwrap();

    let mut read_only = std::fs::metadata(&provider_root).unwrap().permissions();
    read_only.set_mode(0o555);
    std::fs::set_permissions(&provider_root, read_only).unwrap();

    let report = discover_cloud_roots_report(home);

    let mut restored = std::fs::metadata(&provider_root).unwrap().permissions();
    restored.set_mode(0o755);
    std::fs::set_permissions(&provider_root, restored).unwrap();

    assert!(report
        .roots
        .iter()
        .all(|root| root.path != provider_root.to_string_lossy()));
    assert!(report.issues.iter().any(|issue| {
        issue.provider == Some(CloudProvider::Onedrive)
            && issue.path == provider_root.to_string_lossy()
            && issue.reason == "read-only"
    }));
}

#[cfg(unix)]
#[test]
fn discovery_deduplicates_provider_aliases_by_canonical_identity() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    std::fs::create_dir_all(home.join("Library/CloudStorage")).unwrap();
    let target = home.join("provider-target");
    std::fs::create_dir(&target).unwrap();
    std::os::unix::fs::symlink(&target, home.join("OneDrive")).unwrap();
    std::os::unix::fs::symlink(&target, home.join("OneDrive - Contoso")).unwrap();

    let report = discover_cloud_roots_report(home);
    let onedrive_roots = report
        .roots
        .iter()
        .filter(|root| root.provider == CloudProvider::Onedrive)
        .count();

    assert_eq!(onedrive_roots, 1);
    assert!(report.issues.is_empty(), "{:?}", report.issues);
}
