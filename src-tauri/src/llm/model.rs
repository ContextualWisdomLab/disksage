//! Model registry plus bounded, fail-closed model-artifact installation.
//!
//! DiskSage treats an on-device model as executable product supply-chain input.
//! The trusted model specification therefore binds an immutable upstream revision,
//! expected byte length, and SHA-256 digest. Downloads are streamed into a
//! create-new sibling staging file, verified, identity-bound to the open file
//! handle, and only then linked into the final destination without replacing an
//! existing file.

use same_file::Handle;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MODEL_STREAM_BUFFER_BYTES: usize = 64 * 1024;
const ERROR_INVALID_SPEC: &str = "model-spec-invalid";
const ERROR_DESTINATION_EXISTS: &str = "model-destination-exists";
const ERROR_STAGING_EXISTS: &str = "model-staging-exists";
const ERROR_STAGING_CREATE: &str = "model-staging-create-failed";
const ERROR_STREAM_READ: &str = "model-stream-read-failed";
const ERROR_STREAM_WRITE: &str = "model-stream-write-failed";
const ERROR_SIZE_MISMATCH: &str = "model-size-mismatch";
const ERROR_DIGEST_MISMATCH: &str = "model-sha256-mismatch";
const ERROR_STAGING_SYNC: &str = "model-staging-sync-failed";
const ERROR_FINALIZE: &str = "model-finalize-failed";
const ERROR_NETWORK: &str = "model-download-unavailable";

/// Immutable identity and integrity information for one downloadable model artifact.
///
/// Callers should treat every field as trusted application configuration, not as
/// network-provided metadata. `url` identifies a specific upstream revision,
/// `sha256_hex` identifies the expected bytes, and `bytes` places an independent
/// upper bound on what DiskSage will accept from the network.
#[derive(Debug, Clone, Copy)]
pub struct ModelSpec {
    /// Human-readable model name used in the local model filename and UI.
    pub name: &'static str,
    /// HTTPS URL bound to an immutable upstream repository revision.
    pub url: &'static str,
    /// Expected 64-character SHA-256 digest, encoded as hexadecimal text.
    pub sha256_hex: &'static str,
    /// Exact number of bytes that the accepted artifact must contain.
    pub bytes: u64,
}

/// Default offline-advisor model shipped by DiskSage's model registry.
///
/// The URL is pinned to Hugging Face revision
/// `a615a81362316d7b9f5a7a9c4313adfdf9b54588` rather than the mutable `main`
/// branch. Hugging Face reports the same SHA-256 and remote size for the
/// `qwen2.5-1.5b-instruct-q4_k_m.gguf` object at that revision.
pub const DEFAULT: ModelSpec = ModelSpec {
    name: "Qwen2.5-1.5B-Instruct-Q4_K_M",
    url: "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/a615a81362316d7b9f5a7a9c4313adfdf9b54588/qwen2.5-1.5b-instruct-q4_k_m.gguf",
    sha256_hex: "6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e",
    bytes: 1_117_320_736,
};

/// Return `true` only when `bytes` has the expected SHA-256 digest.
///
/// Hexadecimal letters in `expected_hex` may be upper- or lowercase. A malformed
/// or differently sized expected value simply returns `false`; callers that install
/// files additionally validate that a model specification contains exactly 64 hex
/// characters before reading any artifact bytes.
pub fn verify_sha256(bytes: &[u8], expected_hex: &str) -> bool {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let observed: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    observed.eq_ignore_ascii_case(expected_hex)
}

/// Download and safely install one model artifact at `dest`.
///
/// DiskSage accepts only HTTPS model URLs in production. The HTTP response is
/// streamed with an explicit `expected bytes + 1` reader limit so an upstream
/// server cannot cause a one-gigabyte model to be buffered entirely in memory or
/// silently append unbounded data. When the server supplies `Content-Length`, it
/// must exactly match the trusted specification before any file is created.
///
/// The destination is never overwritten. A sibling `<filename>.part` file is
/// created with create-new semantics, removed on validation or I/O failure only
/// while its pathname still identifies DiskSage's open file, and promoted with a
/// no-clobber hard link only after exact byte-count, SHA-256, flush, durable
/// file-sync, and source/destination identity checks pass. Returned errors are
/// stable codes and do not expose local paths, HTTP response bodies, or dynamic
/// network diagnostics.
pub fn download_to(spec: &ModelSpec, dest: &Path) -> Result<(), String> {
    validate_model_spec(spec)?;
    let mut response = ureq::get(spec.url)
        .call()
        .map_err(|_| ERROR_NETWORK.to_string())?;
    if let Some(content_length) = response.body().content_length() {
        if content_length != spec.bytes {
            return Err(ERROR_SIZE_MISMATCH.to_string());
        }
    }
    let read_limit = spec
        .bytes
        .checked_add(1)
        .ok_or_else(|| ERROR_INVALID_SPEC.to_string())?;
    let reader = response
        .body_mut()
        .with_config()
        .limit(read_limit)
        .reader();
    install_verified_reader(spec, reader, dest)
}

/// Validate trusted model metadata before network or filesystem work begins.
fn validate_model_spec(spec: &ModelSpec) -> Result<(), String> {
    let valid_url = spec.url.starts_with("https://")
        || (cfg!(test) && spec.url.starts_with("http://127.0.0.1:"));
    let valid_digest = spec.sha256_hex.len() == 64
        && spec
            .sha256_hex
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit());
    if spec.name.trim().is_empty()
        || !valid_url
        || !valid_digest
        || spec.bytes == 0
        || spec.bytes == u64::MAX
    {
        return Err(ERROR_INVALID_SPEC.to_string());
    }
    Ok(())
}

/// Derive a staging path by appending `.part` to the complete destination filename.
fn staging_path(dest: &Path) -> Result<PathBuf, String> {
    let file_name = dest
        .file_name()
        .ok_or_else(|| ERROR_STAGING_CREATE.to_string())?;
    let mut staging_name = file_name.to_os_string();
    staging_name.push(".part");
    Ok(dest.with_file_name(staging_name))
}

/// Open an identity handle only for a regular file, never for a symlink alias.
fn regular_file_handle(path: &Path) -> Option<Handle> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.file_type().is_file() {
        return None;
    }
    Handle::from_path(path).ok()
}

/// Return `true` only when a regular-file pathname still names `expected`.
fn path_matches_handle(path: &Path, expected: &Handle) -> bool {
    regular_file_handle(path)
        .map(|current| current.eq(expected))
        .unwrap_or(false)
}

/// Remove `path` only when it still identifies the file owned by DiskSage.
fn cleanup_owned_path(path: &Path, expected: &Handle) {
    if path_matches_handle(path, expected) {
        let _ = fs::remove_file(path);
    }
}

/// Derive current identity from an open file before cleaning its pathname.
fn cleanup_open_file_path(path: &Path, file: &File) {
    let Ok(cloned_file) = file.try_clone() else {
        return;
    };
    let Ok(expected) = Handle::from_file(cloned_file) else {
        return;
    };
    cleanup_owned_path(path, &expected);
}

/// Finalize a verified staging file while exposing deterministic race seams to tests.
///
/// Production passes no-op hooks. The hooks exist so concurrency regressions can
/// mutate pathnames at exact boundaries without sleeps or probabilistic scheduling.
/// Every cleanup is identity-bound: foreign replacements are preserved unless the
/// path still identifies the exact file DiskSage observed for that cleanup action.
pub(super) fn finalize_verified_staging_with_hooks<F1, F2, F3>(
    staging: &Path,
    dest: &Path,
    verified_identity: &Handle,
    after_staging_preflight: F1,
    after_link_creation: F2,
    after_destination_binding: F3,
) -> Result<(), String>
where
    F1: FnOnce(),
    F2: FnOnce(),
    F3: FnOnce(),
{
    let result = (|| {
        if !path_matches_handle(staging, verified_identity) {
            return Err(ERROR_FINALIZE.to_string());
        }
        after_staging_preflight();

        fs::hard_link(staging, dest).map_err(|_| ERROR_FINALIZE.to_string())?;
        after_link_creation();

        let destination_identity = match regular_file_handle(dest) {
            Some(identity) => identity,
            None => {
                cleanup_owned_path(dest, verified_identity);
                return Err(ERROR_FINALIZE.to_string());
            }
        };
        if !destination_identity.eq(verified_identity) {
            cleanup_owned_path(dest, &destination_identity);
            return Err(ERROR_FINALIZE.to_string());
        }
        after_destination_binding();

        if !path_matches_handle(staging, verified_identity) {
            cleanup_owned_path(dest, verified_identity);
            return Err(ERROR_FINALIZE.to_string());
        }
        if fs::remove_file(staging).is_err() {
            cleanup_owned_path(dest, verified_identity);
            return Err(ERROR_FINALIZE.to_string());
        }
        if !path_matches_handle(dest, verified_identity) {
            return Err(ERROR_FINALIZE.to_string());
        }
        Ok(())
    })();

    if result.is_err() {
        cleanup_owned_path(staging, verified_identity);
    }
    result
}

/// Stream trusted model bytes into a create-new staging file and promote them safely.
fn install_verified_reader<R: Read>(
    spec: &ModelSpec,
    mut reader: R,
    dest: &Path,
) -> Result<(), String> {
    validate_model_spec(spec)?;
    if fs::symlink_metadata(dest).is_ok() {
        return Err(ERROR_DESTINATION_EXISTS.to_string());
    }

    let staging = staging_path(dest)?;
    if fs::symlink_metadata(&staging).is_ok() {
        return Err(ERROR_STAGING_EXISTS.to_string());
    }

    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staging)
        .map_err(|_| ERROR_STAGING_CREATE.to_string())?;

    let verified_identity = match (|| {
        let mut hasher = Sha256::new();
        let mut observed_bytes = 0_u64;
        let mut buffer = [0_u8; MODEL_STREAM_BUFFER_BYTES];

        loop {
            let count = reader
                .read(&mut buffer)
                .map_err(|_| ERROR_STREAM_READ.to_string())?;
            if count == 0 {
                break;
            }
            observed_bytes = observed_bytes
                .checked_add(count as u64)
                .ok_or_else(|| ERROR_SIZE_MISMATCH.to_string())?;
            if observed_bytes > spec.bytes {
                return Err(ERROR_SIZE_MISMATCH.to_string());
            }
            output
                .write_all(&buffer[..count])
                .map_err(|_| ERROR_STREAM_WRITE.to_string())?;
            hasher.update(&buffer[..count]);
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

        output
            .flush()
            .map_err(|_| ERROR_STAGING_SYNC.to_string())?;
        output
            .sync_all()
            .map_err(|_| ERROR_STAGING_SYNC.to_string())?;
        let cloned_file = output
            .try_clone()
            .map_err(|_| ERROR_FINALIZE.to_string())?;
        Handle::from_file(cloned_file).map_err(|_| ERROR_FINALIZE.to_string())
    })() {
        Ok(identity) => identity,
        Err(error) => {
            cleanup_open_file_path(&staging, &output);
            return Err(error);
        }
    };

    finalize_verified_staging_with_hooks(
        &staging,
        dest,
        &verified_identity,
        || {},
        || {},
        || {},
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Cursor};
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    const FIXTURE_PAYLOAD: &[u8] = b"deterministic-model-fixture";
    const FIXTURE_SHA256: &str =
        "34cec159d295eff35a2ce56813c09e0466f4cad846edeb98a9dd94f06a9e7100";

    /// Build a trusted fixture specification for deterministic installer tests.
    fn fixture_spec(url: &'static str) -> ModelSpec {
        ModelSpec {
            name: "fixture-model",
            url,
            sha256_hex: FIXTURE_SHA256,
            bytes: FIXTURE_PAYLOAD.len() as u64,
        }
    }

    /// Start one loopback HTTP server and return its leaked URL plus join handle.
    fn serve_once(response: Vec<u8>) -> (&'static str, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let url = Box::leak(format!("http://{address}/model.gguf").into_boxed_str());
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 2048];
            let _ = stream.read(&mut request);
            stream.write_all(&response).unwrap();
            stream.flush().unwrap();
        });
        (url, handle)
    }

    /// Start one loopback peer that accepts the request and closes without HTTP headers.
    fn serve_broken_response() -> (&'static str, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let url = Box::leak(format!("http://{address}/model.gguf").into_boxed_str());
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 2048];
            let _ = stream.read(&mut request);
            drop(stream);
        });
        (url, handle)
    }

    /// Reader that returns fixture bytes once and then produces a deterministic I/O error.
    struct FailingReader {
        sent: bool,
    }

    impl Read for FailingReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.sent {
                return Err(io::Error::other("deterministic read failure"));
            }
            self.sent = true;
            let count = FIXTURE_PAYLOAD.len().min(buffer.len());
            buffer[..count].copy_from_slice(&FIXTURE_PAYLOAD[..count]);
            Ok(count)
        }
    }

    #[test]
    fn verify_sha256_matches_known_vector() {
        let want = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(verify_sha256(b"abc", want));
        assert!(verify_sha256(b"abc", &want.to_uppercase()));
    }

    #[test]
    fn verify_sha256_rejects_mismatch() {
        assert!(!verify_sha256(b"abc", "deadbeef"));
        assert!(!verify_sha256(
            b"xyz",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        ));
    }

    #[test]
    fn default_spec_is_wellformed_and_revision_pinned() {
        assert!(DEFAULT.url.starts_with("https://"));
        assert!(DEFAULT
            .url
            .contains("/resolve/a615a81362316d7b9f5a7a9c4313adfdf9b54588/"));
        assert!(!DEFAULT.url.contains("/resolve/main/"));
        assert_eq!(DEFAULT.sha256_hex.len(), 64);
        assert_eq!(DEFAULT.bytes, 1_117_320_736);
        assert!(!DEFAULT.name.is_empty());
    }

    #[test]
    fn model_spec_validation_is_fail_closed() {
        let good = fixture_spec("https://example.invalid/model.gguf");
        assert!(validate_model_spec(&good).is_ok());

        for invalid in [
            ModelSpec { name: " ", ..good },
            ModelSpec {
                url: "http://example.invalid/model.gguf",
                ..good
            },
            ModelSpec {
                sha256_hex: "abcd",
                ..good
            },
            ModelSpec {
                sha256_hex: "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
                ..good
            },
            ModelSpec { bytes: 0, ..good },
            ModelSpec {
                bytes: u64::MAX,
                ..good
            },
        ] {
            assert_eq!(
                validate_model_spec(&invalid),
                Err(ERROR_INVALID_SPEC.to_string())
            );
        }
    }

    #[test]
    fn staging_path_appends_part_without_discarding_the_real_extension() {
        assert_eq!(
            staging_path(Path::new("/tmp/model.gguf")).unwrap(),
            PathBuf::from("/tmp/model.gguf.part")
        );
        assert_eq!(staging_path(Path::new("/")).unwrap_err(), ERROR_STAGING_CREATE);
    }

    #[test]
    fn identity_aware_cleanup_is_idempotent_and_preserves_unowned_paths() {
        let directory = tempfile::tempdir().unwrap();
        let owned = directory.path().join("owned.gguf.part");
        let unowned = directory.path().join("unowned.gguf.part");
        let missing = directory.path().join("missing.gguf.part");
        fs::write(&owned, b"owned").unwrap();
        fs::write(&unowned, b"unowned").unwrap();
        let owned_identity = regular_file_handle(&owned).unwrap();

        assert!(path_matches_handle(&owned, &owned_identity));
        assert!(!path_matches_handle(&unowned, &owned_identity));
        assert!(!path_matches_handle(&missing, &owned_identity));

        cleanup_owned_path(&unowned, &owned_identity);
        cleanup_owned_path(&missing, &owned_identity);
        cleanup_owned_path(&owned, &owned_identity);
        cleanup_owned_path(&owned, &owned_identity);

        assert_eq!(fs::read(&unowned).unwrap(), b"unowned");
        assert!(!owned.exists());
    }

    #[cfg(unix)]
    #[test]
    fn pathname_identity_rejects_symlink_aliases() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let owned = directory.path().join("owned.gguf.part");
        let alias = directory.path().join("alias.gguf.part");
        fs::write(&owned, b"owned").unwrap();
        symlink(&owned, &alias).unwrap();
        let owned_identity = regular_file_handle(&owned).unwrap();

        assert!(!path_matches_handle(&alias, &owned_identity));
        cleanup_owned_path(&alias, &owned_identity);
        assert!(fs::symlink_metadata(&alias).unwrap().file_type().is_symlink());
    }

    #[test]
    fn verified_stream_install_materializes_only_exact_size_and_digest() {
        let spec = fixture_spec("https://example.invalid/model.gguf");
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("fixture.gguf");

        install_verified_reader(&spec, Cursor::new(FIXTURE_PAYLOAD), &dest).unwrap();

        assert_eq!(fs::read(&dest).unwrap(), FIXTURE_PAYLOAD);
        assert!(!staging_path(&dest).unwrap().exists());
    }

    #[test]
    fn verified_stream_install_rejects_short_long_and_wrong_digest_inputs() {
        let spec = fixture_spec("https://example.invalid/model.gguf");
        let cases: [(&[u8], &str); 3] = [
            (
                &FIXTURE_PAYLOAD[..FIXTURE_PAYLOAD.len() - 1],
                ERROR_SIZE_MISMATCH,
            ),
            (b"deterministic-model-fixture-extra", ERROR_SIZE_MISMATCH),
            (b"xxxxxxxxxxxxxxxxxxxxxxxxxxx", ERROR_DIGEST_MISMATCH),
        ];

        for (index, (payload, expected_error)) in cases.into_iter().enumerate() {
            let dir = tempfile::tempdir().unwrap();
            let dest = dir.path().join(format!("fixture-{index}.gguf"));
            assert_eq!(
                install_verified_reader(&spec, Cursor::new(payload), &dest),
                Err(expected_error.to_string())
            );
            assert!(!dest.exists());
            assert!(!staging_path(&dest).unwrap().exists());
        }
    }

    #[test]
    fn verified_stream_install_never_overwrites_destination_or_staging_file() {
        let spec = fixture_spec("https://example.invalid/model.gguf");
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("fixture.gguf");
        fs::write(&dest, b"keep-destination").unwrap();
        assert_eq!(
            install_verified_reader(&spec, Cursor::new(FIXTURE_PAYLOAD), &dest),
            Err(ERROR_DESTINATION_EXISTS.to_string())
        );
        assert_eq!(fs::read(&dest).unwrap(), b"keep-destination");

        fs::remove_file(&dest).unwrap();
        let staging = staging_path(&dest).unwrap();
        fs::write(&staging, b"keep-staging").unwrap();
        assert_eq!(
            install_verified_reader(&spec, Cursor::new(FIXTURE_PAYLOAD), &dest),
            Err(ERROR_STAGING_EXISTS.to_string())
        );
        assert_eq!(fs::read(&staging).unwrap(), b"keep-staging");
        assert!(!dest.exists());
    }

    #[test]
    fn verified_stream_install_cleans_staging_after_reader_failure() {
        let spec = fixture_spec("https://example.invalid/model.gguf");
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("fixture.gguf");
        assert_eq!(
            install_verified_reader(&spec, FailingReader { sent: false }, &dest),
            Err(ERROR_STREAM_READ.to_string())
        );
        assert!(!dest.exists());
        assert!(!staging_path(&dest).unwrap().exists());
    }

    #[test]
    fn verified_stream_install_reports_uncreatable_parent_without_path_leakage() {
        let spec = fixture_spec("https://example.invalid/model.gguf");
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("missing-parent").join("fixture.gguf");
        assert_eq!(
            install_verified_reader(&spec, Cursor::new(FIXTURE_PAYLOAD), &dest),
            Err(ERROR_STAGING_CREATE.to_string())
        );
        assert!(!dest.exists());
    }

    #[test]
    fn download_to_streams_a_bounded_exact_http_fixture_in_tests() {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            FIXTURE_PAYLOAD.len()
        )
        .into_bytes()
        .into_iter()
        .chain(FIXTURE_PAYLOAD.iter().copied())
        .collect();
        let (url, server) = serve_once(response);
        let spec = fixture_spec(url);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("downloaded.gguf");

        download_to(&spec, &dest).unwrap();
        server.join().unwrap();

        assert_eq!(fs::read(&dest).unwrap(), FIXTURE_PAYLOAD);
    }

    #[test]
    fn download_to_rejects_declared_length_drift_before_creating_staging() {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            FIXTURE_PAYLOAD.len() + 1
        )
        .into_bytes()
        .into_iter()
        .chain(FIXTURE_PAYLOAD.iter().copied())
        .chain(std::iter::once(b'!'))
        .collect();
        let (url, server) = serve_once(response);
        let spec = fixture_spec(url);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("downloaded.gguf");

        assert_eq!(
            download_to(&spec, &dest),
            Err(ERROR_SIZE_MISMATCH.to_string())
        );
        server.join().unwrap();
        assert!(!dest.exists());
        assert!(!staging_path(&dest).unwrap().exists());
    }

    #[test]
    fn download_to_redacts_transport_failures() {
        let (url, server) = serve_broken_response();
        let spec = fixture_spec(url);
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("downloaded.gguf");

        assert_eq!(download_to(&spec, &dest), Err(ERROR_NETWORK.to_string()));
        server.join().unwrap();
        assert!(!dest.exists());
    }

    #[test]
    fn download_to_rejects_invalid_spec_before_network_access() {
        let invalid = ModelSpec {
            name: "fixture-model",
            url: "http://example.invalid/model.gguf",
            sha256_hex: FIXTURE_SHA256,
            bytes: FIXTURE_PAYLOAD.len() as u64,
        };
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("downloaded.gguf");
        assert_eq!(
            download_to(&invalid, &dest),
            Err(ERROR_INVALID_SPEC.to_string())
        );
    }

    #[test]
    fn loopback_server_helper_accepts_a_tcp_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || listener.accept().unwrap().0);
        let client = TcpStream::connect(address).unwrap();
        drop(client);
        drop(handle.join().unwrap());
    }
}
