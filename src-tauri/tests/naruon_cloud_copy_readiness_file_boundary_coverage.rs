//! Credential-free file-boundary coverage for path-free NarUon readiness envelopes.
//!
//! The reader is exercised with real temporary filesystem objects. The tests never contact a
//! provider, read user content, write cloud state, or grant cloud-write/source-eviction authority.

use std::path::Path;

use disksage_lib::naruon_cloud_copy_readiness::{
    read_and_validate_naruon_cloud_copy_readiness, NARUON_CLOUD_COPY_READINESS_MAX_INPUT_BYTES,
};

#[test]
fn reader_rejects_unsafe_or_unbounded_inputs_before_json_decode() {
    assert_eq!(
        read_and_validate_naruon_cloud_copy_readiness(Path::new("relative-readiness.json"))
            .unwrap_err(),
        "naruon-copy-readiness-input-path-not-absolute"
    );

    let directory = tempfile::tempdir().unwrap();
    assert!(directory.path().is_absolute());

    let missing = directory.path().join("missing.json");
    assert_eq!(
        read_and_validate_naruon_cloud_copy_readiness(&missing).unwrap_err(),
        "naruon-copy-readiness-input-metadata-unavailable"
    );

    assert_eq!(
        read_and_validate_naruon_cloud_copy_readiness(directory.path()).unwrap_err(),
        "naruon-copy-readiness-input-not-regular-file"
    );

    let empty = directory.path().join("empty.json");
    std::fs::File::create(&empty).unwrap();
    assert_eq!(
        read_and_validate_naruon_cloud_copy_readiness(&empty).unwrap_err(),
        "naruon-copy-readiness-input-size-invalid"
    );

    let oversized = directory.path().join("oversized.json");
    let oversized_file = std::fs::File::create(&oversized).unwrap();
    oversized_file
        .set_len(NARUON_CLOUD_COPY_READINESS_MAX_INPUT_BYTES + 1)
        .unwrap();
    assert_eq!(
        read_and_validate_naruon_cloud_copy_readiness(&oversized).unwrap_err(),
        "naruon-copy-readiness-input-size-invalid"
    );
}

#[test]
fn reader_rejects_malformed_json_at_the_bounded_regular_file_boundary() {
    let directory = tempfile::tempdir().unwrap();
    let malformed = directory.path().join("malformed.json");
    std::fs::write(&malformed, b"{not-json}").unwrap();

    assert_eq!(
        read_and_validate_naruon_cloud_copy_readiness(&malformed).unwrap_err(),
        "naruon-copy-readiness-json-invalid"
    );
}

#[cfg(unix)]
#[test]
fn reader_rejects_symlink_input_without_following_the_target() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("target.json");
    let link = directory.path().join("readiness-link.json");
    std::fs::write(&target, b"{not-json}").unwrap();
    std::os::unix::fs::symlink(&target, &link).unwrap();

    assert_eq!(
        read_and_validate_naruon_cloud_copy_readiness(&link).unwrap_err(),
        "naruon-copy-readiness-input-not-regular-file"
    );
}
