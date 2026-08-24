//! Credential-free coverage for the Naruon readiness file-admission boundary.
//!
//! These tests exercise only local temporary files. They intentionally stop before any
//! provider, account, cloud-write, or source-eviction authority can be reached.

use std::fs::{self, OpenOptions};
use std::path::Path;

use disksage_lib::naruon_cloud_copy_readiness::{
    read_and_validate_naruon_cloud_copy_readiness, NARUON_CLOUD_COPY_READINESS_MAX_INPUT_BYTES,
};
use tempfile::tempdir;

#[test]
fn readiness_file_reader_rejects_relative_missing_directory_and_empty_inputs() {
    assert_eq!(
        read_and_validate_naruon_cloud_copy_readiness(Path::new("relative-readiness.json"))
            .unwrap_err(),
        "naruon-copy-readiness-input-path-not-absolute"
    );

    let temp = tempdir().unwrap();
    let missing = temp.path().join("missing.json");
    assert_eq!(
        read_and_validate_naruon_cloud_copy_readiness(&missing).unwrap_err(),
        "naruon-copy-readiness-input-metadata-unavailable"
    );
    assert_eq!(
        read_and_validate_naruon_cloud_copy_readiness(temp.path()).unwrap_err(),
        "naruon-copy-readiness-input-not-regular-file"
    );

    let empty = temp.path().join("empty.json");
    fs::write(&empty, []).unwrap();
    assert_eq!(
        read_and_validate_naruon_cloud_copy_readiness(&empty).unwrap_err(),
        "naruon-copy-readiness-input-size-invalid"
    );
}

#[test]
fn readiness_file_reader_rejects_oversized_and_malformed_regular_files() {
    let temp = tempdir().unwrap();
    let oversized = temp.path().join("oversized.json");
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&oversized)
        .unwrap();
    file.set_len(NARUON_CLOUD_COPY_READINESS_MAX_INPUT_BYTES + 1)
        .unwrap();
    drop(file);
    assert_eq!(
        read_and_validate_naruon_cloud_copy_readiness(&oversized).unwrap_err(),
        "naruon-copy-readiness-input-size-invalid"
    );

    let malformed = temp.path().join("malformed.json");
    fs::write(&malformed, b"{not-json").unwrap();
    assert_eq!(
        read_and_validate_naruon_cloud_copy_readiness(&malformed).unwrap_err(),
        "naruon-copy-readiness-json-invalid"
    );
}

#[cfg(unix)]
#[test]
fn readiness_file_reader_does_not_follow_symlinks() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let target = temp.path().join("target.json");
    fs::write(&target, b"{}").unwrap();
    let link = temp.path().join("link.json");
    symlink(&target, &link).unwrap();

    assert_eq!(
        read_and_validate_naruon_cloud_copy_readiness(&link).unwrap_err(),
        "naruon-copy-readiness-input-not-regular-file"
    );
}
