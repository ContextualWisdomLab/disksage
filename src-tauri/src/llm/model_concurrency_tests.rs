//! Concurrency regression tests for the model-artifact no-clobber boundary.

use super::model::finalize_verified_file_with_hooks;
use super::{download_to, ModelSpec};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::thread;

const FIXTURE_PAYLOAD: &[u8] = b"deterministic-model-fixture";
const FIXTURE_SHA256: &str =
    "34cec159d295eff35a2ce56813c09e0466f4cad846edeb98a9dd94f06a9e7100";

/// Derive the legacy sibling `.part` name that foreign actors may still create.
fn legacy_part_path(dest: &Path) -> PathBuf {
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

/// Create one unnamed verified staging file in `directory`.
fn verified_unnamed_fixture(directory: &Path) -> File {
    let mut file = tempfile::tempfile_in(directory).unwrap();
    file.write_all(FIXTURE_PAYLOAD).unwrap();
    file.flush().unwrap();
    file.sync_all().unwrap();
    file
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

/// The durable create-new finalizer refuses a destination owned by another actor.
///
/// The installer has an earlier advisory existence check, but authorization is not
/// derived from that observation. This direct finalizer regression exercises the
/// actual mutation boundary deterministically: once another actor owns the path,
/// create-new must fail and preserve the foreign bytes unchanged.
#[test]
fn destination_created_before_create_new_is_preserved_and_never_replaced() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("fixture.gguf");
    let staging = verified_unnamed_fixture(directory.path());
    let spec = fixture_spec();
    fs::write(&destination, b"concurrent-owner").unwrap();

    let result = finalize_verified_file_with_hooks(
        &spec,
        &destination,
        &staging,
        || {},
        || {},
        || {},
        || {},
    );

    assert_eq!(result, Err("model-finalize-failed".to_string()));
    assert_eq!(fs::read(&destination).unwrap(), b"concurrent-owner");
}

/// A foreign legacy `.part` pathname can change during transfer without authority.
#[test]
fn concurrent_staging_path_replacement_is_structurally_isolated() {
    const FIRST_OWNER: &[u8] = b"first-foreign-owner";
    const SECOND_OWNER: &[u8] = b"second-foreign-owner";

    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("fixture.gguf");
    let foreign_part = legacy_part_path(&destination);
    fs::write(&foreign_part, FIRST_OWNER).unwrap();
    let server_part = foreign_part.clone();

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
        stream
            .write_all(&FIXTURE_PAYLOAD[..FIXTURE_PAYLOAD.len() - 1])
            .unwrap();
        stream.flush().unwrap();
        fs::remove_file(&server_part).unwrap();
        fs::write(&server_part, SECOND_OWNER).unwrap();
        stream
            .write_all(&FIXTURE_PAYLOAD[FIXTURE_PAYLOAD.len() - 1..])
            .unwrap();
        stream.flush().unwrap();
    });

    let spec = ModelSpec {
        name: "fixture-model",
        url,
        sha256_hex: FIXTURE_SHA256,
        bytes: FIXTURE_PAYLOAD.len() as u64,
    };
    download_to(&spec, &destination).unwrap();
    server.join().unwrap();

    assert_eq!(fs::read(&destination).unwrap(), FIXTURE_PAYLOAD);
    assert_eq!(fs::read(&foreign_part).unwrap(), SECOND_OWNER);
}

/// A pre-existing foreign `.part` pathname is ignored and preserved.
#[test]
fn foreign_part_path_is_not_an_installer_authority_boundary() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("fixture.gguf");
    let foreign_part = legacy_part_path(&destination);
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

/// A destination replaced after create-new ownership capture is preserved.
#[cfg(unix)]
#[test]
fn destination_replaced_after_creation_identity_capture_is_preserved() {
    const CONCURRENT_DESTINATION_BYTES: &[u8] = b"concurrent-destination-owner";

    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("fixture.gguf");
    let staging = verified_unnamed_fixture(directory.path());
    let spec = fixture_spec();

    let result = finalize_verified_file_with_hooks(
        &spec,
        &destination,
        &staging,
        || {},
        || {
            fs::remove_file(&destination).unwrap();
            fs::write(&destination, CONCURRENT_DESTINATION_BYTES).unwrap();
        },
        || {},
        || {},
    );

    assert_eq!(result, Err("model-finalize-failed".to_string()));
    assert_eq!(
        fs::read(&destination).unwrap(),
        CONCURRENT_DESTINATION_BYTES
    );
}

/// A destination replaced after final content verification is preserved.
#[cfg(unix)]
#[test]
fn destination_replaced_after_verified_binding_is_preserved() {
    const CONCURRENT_DESTINATION_BYTES: &[u8] = b"concurrent-destination-owner";

    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("fixture.gguf");
    let staging = verified_unnamed_fixture(directory.path());
    let spec = fixture_spec();

    let result = finalize_verified_file_with_hooks(
        &spec,
        &destination,
        &staging,
        || {},
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
}

/// Same-file source mutation cannot bypass the second-pass trusted digest check.
#[test]
fn same_inode_staging_digest_mutation_is_rejected() {
    const MUTATED_BYTES: &[u8] = b"attacker-controlled-bytes!!";

    assert_eq!(MUTATED_BYTES.len(), FIXTURE_PAYLOAD.len());
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("fixture.gguf");
    let staging = verified_unnamed_fixture(directory.path());
    let mut mutator = staging.try_clone().unwrap();
    let spec = fixture_spec();

    let result = finalize_verified_file_with_hooks(
        &spec,
        &destination,
        &staging,
        || {
            mutator.seek(SeekFrom::Start(0)).unwrap();
            mutator.write_all(MUTATED_BYTES).unwrap();
            mutator.flush().unwrap();
        },
        || {},
        || {},
        || {},
    );

    assert_eq!(result, Err("model-finalize-failed".to_string()));
    assert!(!destination.exists());
}

/// Same-file source growth after first-pass verification is rejected.
#[test]
fn same_inode_staging_growth_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("fixture.gguf");
    let staging = verified_unnamed_fixture(directory.path());
    let mut mutator = staging.try_clone().unwrap();
    let spec = fixture_spec();

    let result = finalize_verified_file_with_hooks(
        &spec,
        &destination,
        &staging,
        || {
            mutator.seek(SeekFrom::End(0)).unwrap();
            mutator.write_all(b"!").unwrap();
            mutator.flush().unwrap();
        },
        || {},
        || {},
        || {},
    );

    assert_eq!(result, Err("model-finalize-failed".to_string()));
    assert!(!destination.exists());
}

/// Same-file source truncation after first-pass verification is rejected.
#[test]
fn same_inode_staging_truncation_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("fixture.gguf");
    let staging = verified_unnamed_fixture(directory.path());
    let mut mutator = staging.try_clone().unwrap();
    let spec = fixture_spec();

    let result = finalize_verified_file_with_hooks(
        &spec,
        &destination,
        &staging,
        || {
            mutator.set_len((FIXTURE_PAYLOAD.len() - 1) as u64).unwrap();
            mutator.seek(SeekFrom::Start(0)).unwrap();
        },
        || {},
        || {},
        || {},
    );

    assert_eq!(result, Err("model-finalize-failed".to_string()));
    assert!(!destination.exists());
}

/// Same-file destination mutation before final re-verification is rejected.
#[cfg(unix)]
#[test]
fn same_inode_destination_mutation_is_rejected() {
    const MUTATED_BYTES: &[u8] = b"attacker-controlled-bytes!!";

    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("fixture.gguf");
    let staging = verified_unnamed_fixture(directory.path());
    let spec = fixture_spec();

    let result = finalize_verified_file_with_hooks(
        &spec,
        &destination,
        &staging,
        || {},
        || {},
        || {
            let mut mutator = OpenOptions::new().write(true).open(&destination).unwrap();
            mutator.seek(SeekFrom::Start(0)).unwrap();
            mutator.write_all(MUTATED_BYTES).unwrap();
            mutator.flush().unwrap();
        },
        || {},
    );

    assert_eq!(result, Err("model-finalize-failed".to_string()));
    assert!(!destination.exists());
}
