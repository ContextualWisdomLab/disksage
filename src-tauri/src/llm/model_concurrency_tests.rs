//! Concurrency regression tests for the model-artifact no-clobber boundary.

use super::model::finalize_verified_staging_with_hooks;
use super::{download_to, ModelSpec};
use same_file::Handle;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

const FIXTURE_PAYLOAD: &[u8] = b"deterministic-model-fixture";
const FIXTURE_SHA256: &str =
    "34cec159d295eff35a2ce56813c09e0466f4cad846edeb98a9dd94f06a9e7100";

/// Derive the production sibling staging name used by the installer.
fn staging_path(dest: &Path) -> PathBuf {
    let mut file_name = dest.file_name().unwrap().to_os_string();
    file_name.push(".part");
    dest.with_file_name(file_name)
}

/// Build trusted fixture metadata for direct finalization tests.
fn fixture_spec() -> ModelSpec {
    ModelSpec {
        name: "fixture-model",
        url: "https://example.invalid/model.gguf",
        sha256_hex: FIXTURE_SHA256,
        bytes: FIXTURE_PAYLOAD.len() as u64,
    }
}

/// Open one staging fixture and capture identity from that exact open file.
fn verified_fixture(path: &Path) -> (File, Handle) {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    let identity = Handle::from_file(file.try_clone().unwrap()).unwrap();
    (file, identity)
}

/// Start a one-shot loopback server that serves the exact fixture payload.
fn exact_fixture_server() -> (&'static str, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let url = Box::leak(format!("http://{address}/model.gguf").into_boxed_str());
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 2048];
        let _ = stream.read(&mut request).unwrap();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            FIXTURE_PAYLOAD.len()
        )
        .unwrap();
        stream.write_all(FIXTURE_PAYLOAD).unwrap();
        stream.flush().unwrap();
    });
    (url, server)
}

#[test]
fn concurrent_destination_creation_is_preserved_and_never_replaced() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("fixture.gguf");
    let staging = staging_path(&destination);
    let server_destination = destination.clone();
    let server_staging = staging.clone();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let url = Box::leak(format!("http://{address}/model.gguf").into_boxed_str());

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 2048];
        let _ = stream.read(&mut request).unwrap();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            FIXTURE_PAYLOAD.len()
        )
        .unwrap();
        stream.flush().unwrap();

        let deadline = Instant::now() + Duration::from_secs(3);
        while !server_staging.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            server_staging.exists(),
            "client must create staging before the concurrent destination appears"
        );

        fs::write(&server_destination, b"concurrent-owner").unwrap();
        stream.write_all(FIXTURE_PAYLOAD).unwrap();
        stream.flush().unwrap();
    });

    let spec = ModelSpec {
        name: "fixture-model",
        url,
        sha256_hex: FIXTURE_SHA256,
        bytes: FIXTURE_PAYLOAD.len() as u64,
    };

    assert_eq!(
        download_to(&spec, &destination),
        Err("model-finalize-failed".to_string())
    );
    server.join().unwrap();

    assert_eq!(fs::read(&destination).unwrap(), b"concurrent-owner");
    assert!(!staging.exists());
}

/// Prove that finalization cannot copy or delete a staging pathname replaced after creation.
#[cfg(unix)]
#[test]
fn concurrent_staging_path_replacement_is_rejected_and_preserved() {
    const CONCURRENT_OWNER_BYTES: &[u8] = b"attacker-controlled-bytes!!";

    assert_eq!(CONCURRENT_OWNER_BYTES.len(), FIXTURE_PAYLOAD.len());

    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("fixture.gguf");
    let staging = staging_path(&destination);
    let server_staging = staging.clone();

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let url = Box::leak(format!("http://{address}/model.gguf").into_boxed_str());

    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 2048];
        let _ = stream.read(&mut request).unwrap();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            FIXTURE_PAYLOAD.len()
        )
        .unwrap();
        stream.flush().unwrap();

        let deadline = Instant::now() + Duration::from_secs(3);
        while !server_staging.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(
            server_staging.exists(),
            "client must create staging before its pathname is replaced"
        );

        fs::remove_file(&server_staging).unwrap();
        fs::write(&server_staging, CONCURRENT_OWNER_BYTES).unwrap();
        stream.write_all(FIXTURE_PAYLOAD).unwrap();
        stream.flush().unwrap();
    });

    let spec = ModelSpec {
        name: "fixture-model",
        url,
        sha256_hex: FIXTURE_SHA256,
        bytes: FIXTURE_PAYLOAD.len() as u64,
    };

    let result = download_to(&spec, &destination);
    server.join().unwrap();

    assert_eq!(result, Err("model-finalize-failed".to_string()));
    assert!(!destination.exists());
    assert_eq!(fs::read(&staging).unwrap(), CONCURRENT_OWNER_BYTES);
}

/// Prove that staging replacement between preflight and destination creation is preserved.
#[cfg(unix)]
#[test]
fn staging_replacement_after_preflight_is_rejected_and_preserved() {
    const CONCURRENT_OWNER_BYTES: &[u8] = b"attacker-controlled-bytes!!";

    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("fixture.gguf");
    let staging = staging_path(&destination);
    fs::write(&staging, FIXTURE_PAYLOAD).unwrap();
    let (verified_file, verified_identity) = verified_fixture(&staging);
    let spec = fixture_spec();

    let result = finalize_verified_staging_with_hooks(
        &spec,
        &staging,
        &destination,
        &verified_file,
        &verified_identity,
        || {
            fs::remove_file(&staging).unwrap();
            fs::write(&staging, CONCURRENT_OWNER_BYTES).unwrap();
        },
        || {},
        || {},
    );

    assert_eq!(result, Err("model-finalize-failed".to_string()));
    assert!(!destination.exists());
    assert_eq!(fs::read(&staging).unwrap(), CONCURRENT_OWNER_BYTES);
}

/// Prove that a destination replaced after create-new ownership capture is preserved.
#[cfg(unix)]
#[test]
fn destination_replaced_after_creation_identity_capture_is_preserved() {
    const CONCURRENT_DESTINATION_BYTES: &[u8] = b"concurrent-destination-owner";

    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("fixture.gguf");
    let staging = staging_path(&destination);
    fs::write(&staging, FIXTURE_PAYLOAD).unwrap();
    let (verified_file, verified_identity) = verified_fixture(&staging);
    let spec = fixture_spec();

    let result = finalize_verified_staging_with_hooks(
        &spec,
        &staging,
        &destination,
        &verified_file,
        &verified_identity,
        || {},
        || {
            fs::remove_file(&destination).unwrap();
            fs::write(&destination, CONCURRENT_DESTINATION_BYTES).unwrap();
        },
        || {},
    );

    assert_eq!(result, Err("model-finalize-failed".to_string()));
    assert_eq!(
        fs::read(&destination).unwrap(),
        CONCURRENT_DESTINATION_BYTES
    );
    assert!(!staging.exists());
}

/// Prove that failed cleanup never deletes a destination replaced after verified binding.
#[cfg(unix)]
#[test]
fn destination_replaced_after_verified_binding_is_preserved() {
    const CONCURRENT_DESTINATION_BYTES: &[u8] = b"concurrent-destination-owner";

    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("fixture.gguf");
    let staging = staging_path(&destination);
    fs::write(&staging, FIXTURE_PAYLOAD).unwrap();
    let (verified_file, verified_identity) = verified_fixture(&staging);
    let spec = fixture_spec();

    let result = finalize_verified_staging_with_hooks(
        &spec,
        &staging,
        &destination,
        &verified_file,
        &verified_identity,
        || {},
        || {},
        || {
            fs::remove_file(&destination).unwrap();
            fs::write(&destination, CONCURRENT_DESTINATION_BYTES).unwrap();
        },
    );

    assert_eq!(result, Err("model-finalize-failed".to_string()));
    assert_eq!(
        fs::read(&destination).unwrap(),
        CONCURRENT_DESTINATION_BYTES
    );
    assert!(!staging.exists());
}

/// Prove that same-inode staging mutation cannot bypass the trusted SHA-256 binding.
#[cfg(unix)]
#[test]
fn same_inode_staging_digest_mutation_is_rejected() {
    const MUTATED_BYTES: &[u8] = b"attacker-controlled-bytes!!";

    assert_eq!(MUTATED_BYTES.len(), FIXTURE_PAYLOAD.len());
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("fixture.gguf");
    let staging = staging_path(&destination);
    fs::write(&staging, FIXTURE_PAYLOAD).unwrap();
    let (verified_file, verified_identity) = verified_fixture(&staging);
    let spec = fixture_spec();

    let result = finalize_verified_staging_with_hooks(
        &spec,
        &staging,
        &destination,
        &verified_file,
        &verified_identity,
        || fs::write(&staging, MUTATED_BYTES).unwrap(),
        || {},
        || {},
    );

    assert_eq!(result, Err("model-finalize-failed".to_string()));
    assert!(!destination.exists());
    assert!(!staging.exists());
}

/// Prove that same-inode growth after verification is rejected before installation.
#[cfg(unix)]
#[test]
fn same_inode_staging_growth_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("fixture.gguf");
    let staging = staging_path(&destination);
    fs::write(&staging, FIXTURE_PAYLOAD).unwrap();
    let (verified_file, verified_identity) = verified_fixture(&staging);
    let spec = fixture_spec();

    let result = finalize_verified_staging_with_hooks(
        &spec,
        &staging,
        &destination,
        &verified_file,
        &verified_identity,
        || {
            OpenOptions::new()
                .append(true)
                .open(&staging)
                .unwrap()
                .write_all(b"!")
                .unwrap();
        },
        || {},
        || {},
    );

    assert_eq!(result, Err("model-finalize-failed".to_string()));
    assert!(!destination.exists());
    assert!(!staging.exists());
}

/// Prove that same-inode truncation after verification is rejected before installation.
#[cfg(unix)]
#[test]
fn same_inode_staging_truncation_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("fixture.gguf");
    let staging = staging_path(&destination);
    fs::write(&staging, FIXTURE_PAYLOAD).unwrap();
    let (verified_file, verified_identity) = verified_fixture(&staging);
    let spec = fixture_spec();

    let result = finalize_verified_staging_with_hooks(
        &spec,
        &staging,
        &destination,
        &verified_file,
        &verified_identity,
        || {
            let mut replacement = OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&staging)
                .unwrap();
            replacement
                .write_all(&FIXTURE_PAYLOAD[..FIXTURE_PAYLOAD.len() - 1])
                .unwrap();
        },
        || {},
        || {},
    );

    assert_eq!(result, Err("model-finalize-failed".to_string()));
    assert!(!destination.exists());
    assert!(!staging.exists());
}

/// A foreign `.part` pathname must never become installer authority or be modified.
///
/// This is intentionally RED while production still uses a named sibling staging
/// file: the hardened design must stage through an unnamed file owned only by its
/// open handle, install the verified model, and leave the unrelated `.part` bytes
/// untouched.
#[test]
fn foreign_part_path_is_not_an_installer_authority_boundary() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("fixture.gguf");
    let foreign_part = staging_path(&destination);
    fs::write(&foreign_part, b"foreign-owner").unwrap();
    let (url, server) = exact_fixture_server();
    let spec = ModelSpec {
        name: "fixture-model",
        url,
        sha256_hex: FIXTURE_SHA256,
        bytes: FIXTURE_PAYLOAD.len() as u64,
    };

    let result = download_to(&spec, &destination);
    server.join().unwrap();

    assert_eq!(result, Ok(()));
    assert_eq!(fs::read(&destination).unwrap(), FIXTURE_PAYLOAD);
    assert_eq!(fs::read(&foreign_part).unwrap(), b"foreign-owner");
}
