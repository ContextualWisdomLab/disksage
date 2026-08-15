//! Unix path-identity coverage for cloud-root matching when UTF-8 fallback is unavailable.
//!
//! Exact OS-path identity remains valid, while distinct non-UTF-8 paths must fail closed when
//! filesystem canonicalization cannot establish that they name the same object.

#[cfg(unix)]
use disksage_lib::cloud::cloud_root_path_matches;
#[cfg(unix)]
use std::ffi::OsString;
#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
#[cfg(unix)]
use std::path::PathBuf;

#[cfg(unix)]
fn non_utf8_path(marker: u8) -> PathBuf {
    let mut bytes = b"/definitely-not-present/disksage-cloud-root-".to_vec();
    bytes.push(marker);
    PathBuf::from(OsString::from_vec(bytes))
}

#[cfg(unix)]
#[test]
fn cloud_root_matching_keeps_exact_non_utf8_identity_but_rejects_distinct_unknown_paths() {
    let first = non_utf8_path(0xff);
    let second = non_utf8_path(0xfe);

    assert!(first.to_str().is_none());
    assert!(second.to_str().is_none());
    assert!(cloud_root_path_matches(&first, &first));
    assert!(!cloud_root_path_matches(&first, &second));
    assert!(!cloud_root_path_matches(&first, std::path::Path::new("/definitely-not-present/utf8")));
}
