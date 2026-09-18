#![cfg(unix)]

use disksage_lib::log_archive::{archive_logs, LogArchiveOptions, DEFAULT_EXCLUDE_SUFFIXES};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Read, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tempfile::tempdir;

fn write_verify_only_zstd(script_path: &Path) {
    let script = "#!/bin/sh\nset -eu\ncase \"$1\" in\n  -t)\n    exit 0\n    ;;\n  -dc)\n    cat \"$3\"\n    ;;\n  -q)\n    # Recovery must consume an already-published archive, never recompress it.\n    exit 65\n    ;;\n  *)\n    exit 64\n    ;;\nesac\n";
    fs::write(script_path, script).expect("fake zstd should be writable");
    let mut permissions = fs::metadata(script_path)
        .expect("fake zstd metadata should exist")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(script_path, permissions).expect("fake zstd should be executable");
}

fn set_old_enough(path: &Path) {
    let modified = SystemTime::now()
        .checked_sub(Duration::from_secs(2 * 60 * 60))
        .expect("two hours ago should be representable");
    File::options()
        .write(true)
        .open(path)
        .expect("fixture should be writable")
        .set_modified(modified)
        .expect("fixture mtime should be controllable");
}

fn archive_path_for(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.zst", path.display()))
}

fn options_for(root: &Path, journal: &Path, zstd: &Path) -> LogArchiveOptions {
    LogArchiveOptions {
        root: root.to_path_buf(),
        older_than_days: 0,
        execute: true,
        journal_path: journal.to_path_buf(),
        min_stable_secs: 1,
        zstd_bin: zstd.to_path_buf(),
        exclude_suffixes: DEFAULT_EXCLUDE_SUFFIXES
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
    }
}

fn sha256_file(path: &Path) -> String {
    let mut file = File::open(path).expect("fixture should be readable for digest evidence");
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = file.read(&mut buffer).expect("fixture digest read should succeed");
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    format!("{:x}", hasher.finalize())
}

fn unix_object_id(path: &Path) -> String {
    let metadata = fs::symlink_metadata(path).expect("fixture identity metadata should exist");
    format!("unix:{}:{}", metadata.dev(), metadata.ino())
}

fn pending_entry(path: &Path, bytes: u64, ts_ms: u64) -> serde_json::Value {
    let object_id = unix_object_id(path);
    let digest = sha256_file(path);
    json!({
        "ts_ms": ts_ms,
        "op": "log_archive_compress",
        "path": path.to_string_lossy(),
        "bytes": bytes,
        "outcome": format!(
            "archive_verified_retirement_pending|object_id={object_id}|sha256={digest}"
        )
    })
}

fn append_json_line(writer: &mut impl Write, entry: &serde_json::Value) {
    serde_json::to_writer(&mut *writer, entry).expect("journal entry must serialize");
    writer.write_all(b"\n").expect("journal newline must be writable");
}

#[test]
fn large_ledger_uses_latest_transition_and_canonical_trash_success_consumes_lease() {
    let tmp = tempdir().expect("temporary root should be available");
    let root = tmp.path().join("logs");
    fs::create_dir_all(&root).expect("fixture root should be creatable");

    let consumed = root.join("consumed.jsonl");
    let pending = root.join("pending.jsonl");
    let consumed_bytes = b"lease already consumed by canonical trash success\n".repeat(256);
    let pending_bytes = b"lease still pending and eligible for retry\n".repeat(256);
    fs::write(&consumed, &consumed_bytes).expect("consumed source should be writable");
    fs::write(&pending, &pending_bytes).expect("pending source should be writable");
    set_old_enough(&consumed);
    set_old_enough(&pending);
    fs::write(archive_path_for(&consumed), &consumed_bytes)
        .expect("consumed archive should be writable");
    fs::write(archive_path_for(&pending), &pending_bytes)
        .expect("pending archive should be writable");

    let journal = tmp.path().join("journal.jsonl");
    let journal_file = File::create(&journal).expect("ledger should be creatable");
    let mut writer = BufWriter::new(journal_file);
    append_json_line(
        &mut writer,
        &pending_entry(&consumed, consumed_bytes.len() as u64, 1),
    );
    append_json_line(
        &mut writer,
        &pending_entry(&pending, pending_bytes.len() as u64, 2),
    );

    // Keep this realistic enough to catch whole-ledger-per-candidate implementations while
    // remaining cheap for a single operation-scoped streaming pass.
    for index in 0_u64..20_000 {
        append_json_line(
            &mut writer,
            &json!({
                "ts_ms": 10 + index,
                "op": "log_archive_compress",
                "path": format!("/unrelated/session-{index}.jsonl"),
                "bytes": 17_u64,
                "outcome": "failed:unrelated"
            }),
        );
    }
    append_json_line(
        &mut writer,
        &json!({
            "ts_ms": 30_100_u64,
            "op": "trash_delete",
            "path": consumed.to_string_lossy(),
            "bytes": consumed_bytes.len() as u64,
            "outcome": "ok"
        }),
    );
    writer.flush().expect("ledger should flush");

    let zstd = tmp.path().join("verify-only-zstd");
    write_verify_only_zstd(&zstd);
    let report = archive_logs(&options_for(&root, &journal, &zstd))
        .expect("large-ledger recovery should return a report");

    assert_eq!(
        report.archived, 1,
        "only the still-pending source may resume retirement; report={report:?}"
    );
    assert!(
        consumed.exists(),
        "canonical trash success must consume the older lease even if the same filesystem object is visible again"
    );
    assert!(
        !pending.exists(),
        "the independent unconsumed identity-bound lease should still be eligible for reversible retirement"
    );
    assert_eq!(fs::read(archive_path_for(&consumed)).unwrap(), consumed_bytes);
    assert_eq!(fs::read(archive_path_for(&pending)).unwrap(), pending_bytes);
}

#[test]
fn malformed_newer_ledger_record_fails_closed_for_retry_authority() {
    let tmp = tempdir().expect("temporary root should be available");
    let root = tmp.path().join("logs");
    fs::create_dir_all(&root).expect("fixture root should be creatable");

    let source = root.join("session.jsonl");
    let source_bytes = b"malformed journal must not preserve destructive authority\n".repeat(256);
    fs::write(&source, &source_bytes).expect("source should be writable");
    set_old_enough(&source);
    let archive = archive_path_for(&source);
    fs::write(&archive, &source_bytes).expect("archive should be writable");

    let journal = tmp.path().join("journal.jsonl");
    let mut writer = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&journal)
        .expect("ledger should be writable");
    append_json_line(
        &mut writer,
        &pending_entry(&source, source_bytes.len() as u64, 1),
    );
    writer
        .write_all(b"{this is not valid json and must revoke deletion authority\n")
        .expect("malformed ledger tail should be writable");
    writer.flush().expect("ledger should flush");

    let zstd = tmp.path().join("verify-only-zstd");
    write_verify_only_zstd(&zstd);
    let report = archive_logs(&options_for(&root, &journal, &zstd))
        .expect("malformed-ledger recovery should fail closed in the report");

    assert_eq!(
        report.archived, 0,
        "a malformed ledger must never preserve or manufacture destructive retry authority"
    );
    assert!(source.exists(), "source must survive ambiguous recovery evidence");
    assert_eq!(fs::read(&source).unwrap(), source_bytes);
    assert_eq!(fs::read(&archive).unwrap(), source_bytes);
}
