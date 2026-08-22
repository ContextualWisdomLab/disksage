//! Public-boundary coverage for cloud-root identity matching.
//!
//! Root selection must be stable across direct equality, filesystem aliases, and Unicode
//! normalization without ever treating unrelated or non-UTF-8 paths as equivalent.

use disksage_lib::cloud::cloud_root_path_matches;
use std::path::PathBuf;

#[test]
fn exact_path_identity_matches_without_filesystem_resolution() {
    let path = PathBuf::from("/definitely/not/a/real/disksage-cloud-root");
    assert!(cloud_root_path_matches(&path, &path));
}

#[cfg(unix)]
#[test]
fn canonical_filesystem_identity_matches_symlink_alias() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let real = temp.path().join("real-cloud-root");
    let alias = temp.path().join("cloud-root-alias");
    std::fs::create_dir(&real).unwrap();
    symlink(&real, &alias).unwrap();

    assert_ne!(real, alias);
    assert!(cloud_root_path_matches(&real, &alias));
}

#[test]
fn canonical_equivalent_unicode_paths_match_without_existing_entries() {
    let temp = tempfile::tempdir().unwrap();
    let composed = temp.path().join("Caf\u{e9}");
    let decomposed = temp.path().join("Cafe\u{301}");

    assert_ne!(composed, decomposed);
    assert!(!composed.exists());
    assert!(!decomposed.exists());
    assert!(cloud_root_path_matches(&composed, &decomposed));
}

#[test]
fn unrelated_utf8_paths_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let left = temp.path().join("cloud-root-a");
    let right = temp.path().join("cloud-root-b");

    assert!(!cloud_root_path_matches(&left, &right));
}

#[cfg(unix)]
#[test]
fn unrelated_non_utf8_paths_fail_closed() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let left = PathBuf::from(OsString::from_vec(b"/tmp/disksage-cloud-\xff-a".to_vec()));
    let right = PathBuf::from(OsString::from_vec(b"/tmp/disksage-cloud-\xff-b".to_vec()));

    assert_ne!(left, right);
    assert!(!cloud_root_path_matches(&left, &right));
}
