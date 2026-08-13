use disksage_lib::cloud::{
    cloud_root_path_matches, validate_cloud_root_readable, validate_source_root_readable,
    CloudAccountScope, CloudProvider, CloudRoot,
};
use std::path::PathBuf;

fn cloud_root(path: PathBuf, readable: bool, access_issue: Option<&str>) -> CloudRoot {
    CloudRoot {
        id: "coverage-root".into(),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        label: "Coverage root".into(),
        path: path.to_string_lossy().into_owned(),
        readable,
        access_issue: access_issue.map(str::to_owned),
    }
}

#[test]
fn source_root_validation_distinguishes_directories_from_non_directories() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("plain-file");
    std::fs::write(&file, b"not a directory").unwrap();

    assert!(validate_source_root_readable(temp.path()).is_ok());
    let error = validate_source_root_readable(&file).unwrap_err();
    assert!(error.starts_with("source-root-not-directory:"));
}

#[test]
fn cloud_root_validation_fails_closed_before_and_after_discovery() {
    let temp = tempfile::tempdir().unwrap();

    let not_verified = cloud_root(temp.path().to_path_buf(), false, None);
    assert_eq!(
        validate_cloud_root_readable(&not_verified).unwrap_err(),
        format!("cloud-root-unreadable:{}:not-verified", temp.path().display())
    );

    let denied = cloud_root(
        temp.path().to_path_buf(),
        false,
        Some("permission-denied"),
    );
    assert_eq!(
        validate_cloud_root_readable(&denied).unwrap_err(),
        format!(
            "cloud-root-unreadable:{}:permission-denied",
            temp.path().display()
        )
    );

    let readable = cloud_root(temp.path().to_path_buf(), true, None);
    assert!(validate_cloud_root_readable(&readable).is_ok());

    let missing_path = temp.path().join("missing");
    let missing = cloud_root(missing_path.clone(), true, None);
    let error = validate_cloud_root_readable(&missing).unwrap_err();
    assert!(error.starts_with(&format!("cloud-root-unreadable:{}:", missing_path.display())));
}

#[test]
fn cloud_root_matching_accepts_canonical_unicode_but_rejects_distinct_paths() {
    let composed = PathBuf::from("/not-present/caf\u{e9}");
    let decomposed = PathBuf::from("/not-present/cafe\u{301}");
    let distinct = PathBuf::from("/not-present/other");

    assert!(cloud_root_path_matches(&composed, &composed));
    assert!(cloud_root_path_matches(&composed, &decomposed));
    assert!(!cloud_root_path_matches(&composed, &distinct));
}

#[cfg(unix)]
#[test]
fn cloud_root_matching_uses_filesystem_identity_for_distinct_alias_paths() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let discovered = temp.path().join("provider-root");
    let requested = temp.path().join("provider-root-alias");
    std::fs::create_dir(&discovered).unwrap();
    symlink(&discovered, &requested).unwrap();

    assert_ne!(discovered, requested);
    assert!(cloud_root_path_matches(&discovered, &requested));
}

#[test]
fn cloud_enum_labels_are_stable_for_serialized_evidence() {
    assert_eq!(CloudProvider::Icloud.as_str(), "icloud");
    assert_eq!(CloudProvider::Onedrive.as_str(), "onedrive");
    assert_eq!(CloudProvider::GoogleDrive.as_str(), "google-drive");

    assert_eq!(CloudAccountScope::Personal.as_str(), "personal");
    assert_eq!(CloudAccountScope::Organization.as_str(), "organization");
    assert_eq!(CloudAccountScope::Shared.as_str(), "shared");
    assert_eq!(CloudAccountScope::Unknown.as_str(), "unknown");
    assert_eq!(CloudAccountScope::default(), CloudAccountScope::Unknown);
}
