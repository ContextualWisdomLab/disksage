//! Load-time integrity verification for the installed on-device model artifact.
//!
//! Download-time admission and load-time trust are deliberately separate. A file
//! that was once downloaded correctly can later be replaced, truncated, or
//! redirected. This module re-opens the local artifact read-only and proves that
//! its current regular-file bytes still match the immutable model specification.

use super::model::ModelSpec;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::Path;

const MODEL_READ_BUFFER_BYTES: usize = 64 * 1024;
const ERROR_UNAVAILABLE: &str = "model-installed-unavailable";
const ERROR_NOT_REGULAR: &str = "model-installed-not-regular";
const ERROR_SIZE_MISMATCH: &str = "model-installed-size-mismatch";
const ERROR_READ_FAILED: &str = "model-installed-read-failed";
const ERROR_DIGEST_MISMATCH: &str = "model-installed-digest-mismatch";

/// Verify that an installed model file is the exact artifact named by `spec`.
///
/// This is the load-time gate used before llama.cpp initialization. The path is
/// inspected without following symbolic links, directories and other non-regular
/// objects are rejected, and the opened file handle must report the exact trusted
/// byte length before any model bytes are hashed. SHA-256 is then recomputed using
/// a fixed 64 KiB buffer. Failures return stable path-free error codes and never
/// include local paths, operating-system diagnostics, or model bytes.
pub(crate) fn verify_installed_model(spec: &ModelSpec, path: &Path) -> Result<(), String> {
    let path_metadata = std::fs::symlink_metadata(path)
        .map_err(|_| ERROR_UNAVAILABLE.to_string())?;
    if path_metadata.file_type().is_symlink() || !path_metadata.is_file() {
        return Err(ERROR_NOT_REGULAR.to_string());
    }

    let file = File::open(path).map_err(|_| ERROR_READ_FAILED.to_string())?;
    let handle_metadata = file.metadata().map_err(|_| ERROR_READ_FAILED.to_string())?;
    if handle_metadata.len() != spec.bytes {
        return Err(ERROR_SIZE_MISMATCH.to_string());
    }

    verify_reader(spec, file)
}

/// Verify the exact byte count and SHA-256 digest from an already-open reader.
fn verify_reader<R: Read>(spec: &ModelSpec, mut reader: R) -> Result<(), String> {
    let mut hasher = Sha256::new();
    let mut observed_bytes = 0_u64;
    let mut buffer = [0_u8; MODEL_READ_BUFFER_BYTES];

    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|_| ERROR_READ_FAILED.to_string())?;
        if count == 0 {
            break;
        }
        let count = count as u64;
        if count > spec.bytes.saturating_sub(observed_bytes) {
            return Err(ERROR_SIZE_MISMATCH.to_string());
        }
        observed_bytes += count;
        hasher.update(&buffer[..count as usize]);
    }

    if observed_bytes != spec.bytes {
        return Err(ERROR_SIZE_MISMATCH.to_string());
    }
    let observed_digest: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if !observed_digest.eq_ignore_ascii_case(spec.sha256_hex) {
        return Err(ERROR_DIGEST_MISMATCH.to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::{self, Cursor};
    use std::path::PathBuf;

    const FIXTURE_PAYLOAD: &[u8] = b"deterministic-model-fixture";
    const FIXTURE_SHA256: &str =
        "34cec159d295eff35a2ce56813c09e0466f4cad846edeb98a9dd94f06a9e7100";
    const FIXTURE_SHA256_UPPER: &str =
        "34CEC159D295EFF35A2CE56813C09E0466F4CAD846EDEB98A9DD94F06A9E7100";

    /// Build a trusted fixture specification for load-time verification tests.
    fn fixture_spec(digest: &'static str) -> ModelSpec {
        ModelSpec {
            name: "fixture-model",
            url: "https://example.invalid/model.gguf",
            sha256_hex: digest,
            bytes: FIXTURE_PAYLOAD.len() as u64,
        }
    }

    /// Resolve one source-controlled path from the Cargo manifest directory.
    fn repository_path(relative_path: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri must have a repository parent")
            .join(relative_path)
    }

    /// Reader that emits fixture bytes once and then fails deterministically.
    struct FailingReader {
        sent: bool,
    }

    impl Read for FailingReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.sent {
                return Err(io::Error::other("private deterministic failure"));
            }
            self.sent = true;
            let count = FIXTURE_PAYLOAD.len().min(buffer.len());
            buffer[..count].copy_from_slice(&FIXTURE_PAYLOAD[..count]);
            Ok(count)
        }
    }

    /// Reader that emits more bytes than the trusted size without allocating a large fixture.
    struct RepeatingReader {
        remaining: usize,
    }

    impl Read for RepeatingReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.remaining == 0 {
                return Ok(0);
            }
            let count = self.remaining.min(buffer.len());
            buffer[..count].fill(b'x');
            self.remaining -= count;
            Ok(count)
        }
    }

    #[test]
    fn exact_installed_model_bytes_are_accepted_case_insensitively() {
        for digest in [FIXTURE_SHA256, FIXTURE_SHA256_UPPER] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("fixture.gguf");
            fs::write(&path, FIXTURE_PAYLOAD).unwrap();
            assert_eq!(verify_installed_model(&fixture_spec(digest), &path), Ok(()));
        }
    }

    #[test]
    fn missing_and_non_regular_paths_fail_closed() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing.gguf");
        assert_eq!(
            verify_installed_model(&fixture_spec(FIXTURE_SHA256), &missing),
            Err(ERROR_UNAVAILABLE.to_string())
        );
        assert_eq!(
            verify_installed_model(&fixture_spec(FIXTURE_SHA256), directory.path()),
            Err(ERROR_NOT_REGULAR.to_string())
        );
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_link_is_not_accepted_as_installed_artifact_identity() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("target.gguf");
        let link = directory.path().join("linked.gguf");
        fs::write(&target, FIXTURE_PAYLOAD).unwrap();
        symlink(&target, &link).unwrap();

        assert_eq!(
            verify_installed_model(&fixture_spec(FIXTURE_SHA256), &link),
            Err(ERROR_NOT_REGULAR.to_string())
        );
    }

    #[test]
    fn installed_size_and_digest_drift_are_rejected() {
        let cases: [(&[u8], &str); 3] = [
            (
                &FIXTURE_PAYLOAD[..FIXTURE_PAYLOAD.len() - 1],
                ERROR_SIZE_MISMATCH,
            ),
            (b"deterministic-model-fixture-extra", ERROR_SIZE_MISMATCH),
            (b"deterministic-model-fixturf", ERROR_DIGEST_MISMATCH),
        ];

        for (index, (bytes, expected)) in cases.into_iter().enumerate() {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join(format!("fixture-{index}.gguf"));
            fs::write(&path, bytes).unwrap();
            assert_eq!(
                verify_installed_model(&fixture_spec(FIXTURE_SHA256), &path),
                Err(expected.to_string())
            );
        }
    }

    #[test]
    fn reader_failures_are_redacted_to_a_stable_code() {
        assert_eq!(
            verify_reader(
                &fixture_spec(FIXTURE_SHA256),
                FailingReader { sent: false }
            ),
            Err(ERROR_READ_FAILED.to_string())
        );
    }

    #[test]
    fn reader_contract_checks_exact_size_before_digest() {
        assert_eq!(
            verify_reader(
                &fixture_spec(FIXTURE_SHA256),
                Cursor::new(&FIXTURE_PAYLOAD[..FIXTURE_PAYLOAD.len() - 1])
            ),
            Err(ERROR_SIZE_MISMATCH.to_string())
        );
        assert_eq!(
            verify_reader(
                &fixture_spec(FIXTURE_SHA256),
                RepeatingReader {
                    remaining: FIXTURE_PAYLOAD.len() + 1,
                }
            ),
            Err(ERROR_SIZE_MISMATCH.to_string())
        );
        assert_eq!(
            verify_reader(
                &fixture_spec(FIXTURE_SHA256),
                Cursor::new(b"deterministic-model-fixturf")
            ),
            Err(ERROR_DIGEST_MISMATCH.to_string())
        );
    }

    #[test]
    fn engine_requires_verified_default_model_before_llama_initialization() {
        let engine = fs::read_to_string(repository_path("src-tauri/src/llm/engine.rs")).unwrap();
        let verifier = "super::installed_model::verify_installed_model(&super::model::DEFAULT, model_path)?;";
        let verifier_index = engine
            .find(verifier)
            .expect("LlamaEngine::new must verify the pinned default model first");
        let backend_index = engine
            .find("LlamaBackend::init()")
            .expect("engine must initialize llama backend");
        let load_index = engine
            .find("LlamaModel::load_from_file")
            .expect("engine must load llama model");

        assert!(verifier_index < backend_index);
        assert!(verifier_index < load_index);
        assert_eq!(engine.matches("verify_installed_model(").count(), 1);
    }

    #[test]
    fn installed_model_documentation_is_durable() {
        let doctoring = fs::read_to_string(repository_path(
            "docs/doctoring/model-artifact-integrity.md",
        ))
        .unwrap();
        let changelog = fs::read_to_string(repository_path("CHANGELOG.md")).unwrap();

        for required in [
            "## Load-time verification boundary",
            "file existence is not integrity evidence",
            "64 KiB",
            ERROR_UNAVAILABLE,
            ERROR_NOT_REGULAR,
            ERROR_SIZE_MISMATCH,
            ERROR_READ_FAILED,
            ERROR_DIGEST_MISMATCH,
            "no model bytes or local paths become shareable evidence",
            "## Rollback and migration",
        ] {
            assert!(
                doctoring.contains(required),
                "doctoring must retain load-time integrity evidence: {required}"
            );
        }
        assert!(
            changelog.contains("verify the installed GGUF again before llama.cpp initialization")
        );
    }
}
