//! Model registry plus bounded, fail-closed model-artifact installation.
//!
//! DiskSage treats an on-device model as executable product supply-chain input.
//! The trusted model specification therefore binds an immutable upstream revision,
//! expected byte length, and SHA-256 digest. Downloads are streamed into a
//! create-new sibling staging file, verified, and only then linked into the final
//! destination without replacing an existing file.

use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
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
/// created with create-new semantics, removed on validation or I/O failure, and
/// promoted with a no-clobber hard link only after exact byte-count, SHA-256, flush,
/// and durable file-sync checks pass. Returned errors are stable codes and do not
/// expose local paths, HTTP response bodies, or dynamic network diagnostics.
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

/// Remove a staging file after an unsuccessful installation attempt.
fn cleanup_staging(path: &Path) {
    let _ = fs::remove_file(path);
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

    let result = (|| {
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
        drop(output);

        fs::hard_link(&staging, dest).map_err(|_| ERROR_FINALIZE.to_string())?;
        if fs::remove_file(&staging).is_err() {
            let _ = fs::remove_file(dest);
            return Err(ERROR_FINALIZE.to_string());
        }
        Ok(())
    })();

    if result.is_err() {
        cleanup_staging(&staging);
    }
    result
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
    fn cleanup_staging_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let staging = dir.path().join("fixture.gguf.part");
        fs::write(&staging, b"temporary").unwrap();
        cleanup_staging(&staging);
        cleanup_staging(&staging);
        assert!(!staging.exists());
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
