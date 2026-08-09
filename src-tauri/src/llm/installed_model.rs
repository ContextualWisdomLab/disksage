//! Load-time integrity verification for the installed on-device model artifact.
//!
//! Download-time admission and load-time trust are deliberately separate. A file
//! that was once downloaded correctly can later be replaced, truncated, or
//! redirected. This module opens the local artifact once, binds that open file to
//! the observed pathname, verifies its exact bytes, and retains the validated file
//! while llama.cpp opens a stable load path.

use super::model::ModelSpec;
use same_file::Handle;
use sha2::{Digest, Sha256};
use std::fs::File;
#[cfg(windows)]
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const MODEL_READ_BUFFER_BYTES: usize = 64 * 1024;
const ERROR_UNAVAILABLE: &str = "model-installed-unavailable";
const ERROR_NOT_REGULAR: &str = "model-installed-not-regular";
const ERROR_SIZE_MISMATCH: &str = "model-installed-size-mismatch";
const ERROR_READ_FAILED: &str = "model-installed-read-failed";
const ERROR_DIGEST_MISMATCH: &str = "model-installed-digest-mismatch";
const ERROR_IDENTITY_MISMATCH: &str = "model-installed-identity-mismatch";

/// Non-following path facts collected before an installed model is opened.
///
/// The observation deliberately contains no path or filename. It allows tests to
/// exercise every admission branch without depending on host permission behavior,
/// while production still obtains the facts from `symlink_metadata`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct InstalledModelObservation {
    /// Whether the source-controlled model path itself is a symbolic link.
    pub(super) is_symbolic_link: bool,
    /// Whether the path metadata identifies an ordinary regular file.
    pub(super) is_regular_file: bool,
    /// Byte length reported by the non-following path metadata snapshot.
    pub(super) observed_bytes: u64,
}

/// A verified open model file whose load path cannot silently retarget to another inode.
///
/// On Unix the load path is the process file-descriptor namespace, so pathname
/// replacement after verification still refers llama.cpp to the retained open file.
/// On Windows the retained read handle is opened with read-only sharing, which
/// prevents writers and delete/rename operations until llama.cpp finishes opening
/// the original path. The guard is intentionally kept private so callers cannot
/// separate the load path from the lifetime that makes it trustworthy.
pub(crate) struct VerifiedInstalledModel {
    _guard: File,
    load_path: PathBuf,
}

impl VerifiedInstalledModel {
    /// Return the path that llama.cpp may open while this verified guard is alive.
    pub(crate) fn load_path(&self) -> &Path {
        &self.load_path
    }
}

/// Verify that an installed model file is the exact artifact named by `spec`.
///
/// This compatibility helper performs the same single-open identity and byte
/// verification as the engine-facing guard and then drops the retained handle.
/// Engine code must use [`prepare_verified_installed_model`] so the verified file
/// remains bound through `LlamaModel::load_from_file`.
pub(crate) fn verify_installed_model(spec: &ModelSpec, path: &Path) -> Result<(), String> {
    prepare_verified_installed_model(spec, path).map(|_| ())
}

/// Prepare an installed model for race-resistant llama.cpp loading.
///
/// The function first rejects symbolic links, non-regular entries, and trusted-size
/// drift from non-following pathname metadata. It then opens the source once,
/// validates the opened file type and size, proves the pathname still resolves to
/// the same operating-system file identity, hashes the bytes through a fixed 64 KiB
/// buffer, rewinds the retained handle, and derives an OS-specific stable load path.
/// All failures are path-free stable codes.
pub(crate) fn prepare_verified_installed_model(
    spec: &ModelSpec,
    path: &Path,
) -> Result<VerifiedInstalledModel, String> {
    let path_metadata = std::fs::symlink_metadata(path)
        .map_err(|_| ERROR_UNAVAILABLE.to_string())?;
    let observation = InstalledModelObservation {
        is_symbolic_link: path_metadata.file_type().is_symlink(),
        is_regular_file: path_metadata.is_file(),
        observed_bytes: path_metadata.len(),
    };
    validate_observation(spec, observation)?;

    let mut source = open_verified_source(path).map_err(|_| ERROR_READ_FAILED.to_string())?;
    let opened_metadata = source
        .metadata()
        .map_err(|_| ERROR_READ_FAILED.to_string())?;
    if !opened_metadata.is_file() {
        return Err(ERROR_NOT_REGULAR.to_string());
    }
    if opened_metadata.len() != spec.bytes {
        return Err(ERROR_SIZE_MISMATCH.to_string());
    }

    let current_metadata = std::fs::symlink_metadata(path)
        .map_err(|_| ERROR_IDENTITY_MISMATCH.to_string())?;
    if current_metadata.file_type().is_symlink() || !current_metadata.is_file() {
        return Err(ERROR_IDENTITY_MISMATCH.to_string());
    }

    let opened_identity = Handle::from_file(
        source
            .try_clone()
            .map_err(|_| ERROR_READ_FAILED.to_string())?,
    )
    .map_err(|_| ERROR_READ_FAILED.to_string())?;
    let current_identity =
        Handle::from_path(path).map_err(|_| ERROR_IDENTITY_MISMATCH.to_string())?;
    if opened_identity != current_identity {
        return Err(ERROR_IDENTITY_MISMATCH.to_string());
    }

    verify_reader(spec, &mut source)?;
    source
        .seek(SeekFrom::Start(0))
        .map_err(|_| ERROR_READ_FAILED.to_string())?;
    let load_path = stable_load_path(&source, path);

    Ok(VerifiedInstalledModel {
        _guard: source,
        load_path,
    })
}

/// Open the source with platform-appropriate mutation exclusion while verification is active.
#[cfg(windows)]
fn open_verified_source(path: &Path) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_SHARE_READ: u32 = 0x0000_0001;
    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(path)
}

/// Open the source read-only on Unix; the retained descriptor becomes the load authority.
#[cfg(not(windows))]
fn open_verified_source(path: &Path) -> std::io::Result<File> {
    File::open(path)
}

/// Build a stable load path for an already-open verified file.
#[cfg(target_os = "linux")]
fn stable_load_path(file: &File, _source_path: &Path) -> PathBuf {
    use std::os::fd::AsRawFd;

    PathBuf::from(format!("/proc/self/fd/{}", file.as_raw_fd()))
}

/// Build a stable load path for an already-open verified file on other Unix targets.
#[cfg(all(unix, not(target_os = "linux")))]
fn stable_load_path(file: &File, _source_path: &Path) -> PathBuf {
    use std::os::fd::AsRawFd;

    PathBuf::from(format!("/dev/fd/{}", file.as_raw_fd()))
}

/// Keep the original pathname on Windows while the retained handle denies write/delete sharing.
#[cfg(windows)]
fn stable_load_path(_file: &File, source_path: &Path) -> PathBuf {
    source_path.to_path_buf()
}

/// Fallback for unsupported non-Unix, non-Windows build targets.
#[cfg(not(any(unix, windows)))]
fn stable_load_path(_file: &File, source_path: &Path) -> PathBuf {
    source_path.to_path_buf()
}

/// Apply fail-closed path admission and verify bytes from an injected opener.
///
/// Deterministic tests may supply a reader or a synthetic opener failure. Rejected
/// symbolic links, non-regular files, and size drift never invoke the opener,
/// keeping the least-privilege boundary independently testable.
pub(super) fn verify_observed_model<R, F>(
    spec: &ModelSpec,
    observation: InstalledModelObservation,
    open: F,
) -> Result<(), String>
where
    R: Read,
    F: FnOnce() -> std::io::Result<R>,
{
    validate_observation(spec, observation)?;
    let reader = open().map_err(|_| ERROR_READ_FAILED.to_string())?;
    verify_reader(spec, reader)
}

/// Validate non-following pathname facts before any model bytes are opened.
fn validate_observation(
    spec: &ModelSpec,
    observation: InstalledModelObservation,
) -> Result<(), String> {
    if observation.is_symbolic_link || !observation.is_regular_file {
        return Err(ERROR_NOT_REGULAR.to_string());
    }
    if observation.observed_bytes != spec.bytes {
        return Err(ERROR_SIZE_MISMATCH.to_string());
    }
    Ok(())
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
    fn engine_requires_retained_verified_model_before_llama_initialization() {
        let engine = fs::read_to_string(repository_path("src-tauri/src/llm/engine.rs")).unwrap();
        let verifier = "super::installed_model::prepare_verified_installed_model(";
        let verifier_index = engine
            .find(verifier)
            .expect("LlamaEngine::new must retain the verified model handle first");
        let backend_index = engine
            .find("LlamaBackend::init()")
            .expect("engine must initialize llama backend");
        let load_index = engine
            .find("LlamaModel::load_from_file")
            .expect("engine must load llama model");

        assert!(verifier_index < backend_index);
        assert!(verifier_index < load_index);
        assert!(engine.contains("verified_model.load_path()"));
        assert_eq!(engine.matches("prepare_verified_installed_model(").count(), 1);
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
            ERROR_IDENTITY_MISMATCH,
            "stable descriptor path",
            "Windows read-sharing guard",
            "no model bytes or local paths become shareable evidence",
            "## Rollback and migration",
        ] {
            assert!(
                doctoring.contains(required),
                "doctoring must retain load-time integrity evidence: {required}"
            );
        }
        assert!(
            changelog.contains("retain the verified model handle through llama.cpp loading")
        );
    }
}
