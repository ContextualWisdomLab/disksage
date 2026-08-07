//! Concurrency regression tests for the model-artifact no-clobber boundary.

use super::{download_to, ModelSpec};
use std::fs;
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
