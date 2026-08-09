//! Deterministic I/O-seam tests for installed-model admission.
//!
//! Filesystem permission behavior differs across operating systems and privileged
//! CI containers. These tests therefore inject the model opener directly so the
//! stable error contract and pre-open rejection branches remain deterministic.

use super::installed_model::{
    prepare_verified_installed_model, verify_observed_model, InstalledModelObservation,
};
use super::model::ModelSpec;
use std::cell::Cell;
use std::io::{self, Cursor};

const FIXTURE_PAYLOAD: &[u8] = b"deterministic-model-fixture";
const FIXTURE_SHA256: &str =
    "34cec159d295eff35a2ce56813c09e0466f4cad846edeb98a9dd94f06a9e7100";

/// Build the trusted model specification shared by deterministic opener tests.
fn fixture_spec() -> ModelSpec {
    ModelSpec {
        name: "fixture-model",
        url: "https://example.invalid/model.gguf",
        sha256_hex: FIXTURE_SHA256,
        bytes: FIXTURE_PAYLOAD.len() as u64,
    }
}

#[test]
fn opener_failure_is_redacted_without_relying_on_host_permissions() {
    let observation = InstalledModelObservation {
        is_symbolic_link: false,
        is_regular_file: true,
        observed_bytes: FIXTURE_PAYLOAD.len() as u64,
    };

    let result = verify_observed_model(
        &fixture_spec(),
        observation,
        || -> io::Result<Cursor<&'static [u8]>> {
            Err(io::Error::other("private deterministic opener failure"))
        },
    );

    assert_eq!(result, Err("model-installed-read-failed".to_string()));
}

#[test]
fn non_regular_and_size_drift_are_rejected_before_opening() {
    let opened = Cell::new(false);
    let non_regular = InstalledModelObservation {
        is_symbolic_link: false,
        is_regular_file: false,
        observed_bytes: FIXTURE_PAYLOAD.len() as u64,
    };
    let non_regular_result = verify_observed_model(&fixture_spec(), non_regular, || {
        opened.set(true);
        Ok(Cursor::new(FIXTURE_PAYLOAD))
    });
    assert_eq!(
        non_regular_result,
        Err("model-installed-not-regular".to_string())
    );
    assert!(!opened.get());

    let size_drift = InstalledModelObservation {
        is_symbolic_link: false,
        is_regular_file: true,
        observed_bytes: FIXTURE_PAYLOAD.len() as u64 - 1,
    };
    let size_result = verify_observed_model(&fixture_spec(), size_drift, || {
        opened.set(true);
        Ok(Cursor::new(FIXTURE_PAYLOAD))
    });
    assert_eq!(
        size_result,
        Err("model-installed-size-mismatch".to_string())
    );
    assert!(!opened.get());
}

#[cfg(unix)]
#[test]
fn verified_load_path_survives_source_path_replacement() {
    use std::fs;

    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("fixture.gguf");
    let moved_path = directory.path().join("moved.gguf");
    fs::write(&source_path, FIXTURE_PAYLOAD).unwrap();

    let verified = prepare_verified_installed_model(&fixture_spec(), &source_path).unwrap();
    fs::rename(&source_path, &moved_path).unwrap();
    fs::write(&source_path, b"attacker-controlled-replacement").unwrap();

    assert_eq!(fs::read(verified.load_path()).unwrap(), FIXTURE_PAYLOAD);
}

#[cfg(windows)]
#[test]
fn verified_load_guard_blocks_source_mutation_and_replacement() {
    use std::fs;

    let directory = tempfile::tempdir().unwrap();
    let source_path = directory.path().join("fixture.gguf");
    fs::write(&source_path, FIXTURE_PAYLOAD).unwrap();

    let verified = prepare_verified_installed_model(&fixture_spec(), &source_path).unwrap();
    assert!(fs::write(&source_path, b"attacker-controlled-replacement").is_err());
    assert!(fs::remove_file(&source_path).is_err());
    assert_eq!(verified.load_path(), source_path.as_path());
}
