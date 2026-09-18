#![cfg(unix)]

use disksage_lib::log_archive::{
    archive_logs, ArchiveOutcome, LogArchiveOptions, DEFAULT_EXCLUDE_SUFFIXES,
};
use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime};
use tempfile::tempdir;

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
}

fn write_fake_zstd(script_path: &Path, signal_path: &Path) {
    let script = format!(
        "#!/bin/sh\nset -eu\ncase \"$1\" in\n  -q)\n    cp \"$4\" \"$3\"\n    ;;\n  -t)\n    exit 0\n    ;;\n  -dc)\n    printf ready > {}\n    cat \"$3\"\n    sleep 1\n    ;;\n  *)\n    exit 64\n    ;;\nesac\n",
        shell_quote(signal_path)
    );
    fs::write(script_path, script).expect("fake zstd should be writable");
    let mut permissions = fs::metadata(script_path)
        .expect("fake zstd metadata should exist")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(script_path, permissions).expect("fake zstd should be executable");
}

fn set_modified(path: &Path, modified: SystemTime) {
    File::options()
        .write(true)
        .open(path)
        .expect("fixture should be writable")
        .set_modified(modified)
        .expect("fixture mtime should be controllable");
}

#[test]
fn same_size_same_mtime_replacement_is_never_removed() {
    let tmp = tempdir().expect("temporary root should be available");
    let root = tmp.path().join("logs");
    fs::create_dir_all(&root).expect("fixture root should be creatable");

    let source = root.join("session.jsonl");
    let original = vec![b'A'; 16 * 1024];
    let replacement = vec![b'B'; original.len()];
    fs::write(&source, &original).expect("original fixture should be writable");
    let requested_mtime = SystemTime::now()
        .checked_sub(Duration::from_secs(2 * 60 * 60))
        .expect("two hours ago should be representable");
    set_modified(&source, requested_mtime);
    let stable_mtime = fs::metadata(&source)
        .expect("original metadata should exist")
        .modified()
        .expect("original mtime should be readable");

    let signal = tmp.path().join("decompress-started");
    let fake_zstd = tmp.path().join("fake-zstd");
    write_fake_zstd(&fake_zstd, &signal);

    let options = LogArchiveOptions {
        root: root.clone(),
        older_than_days: 0,
        execute: true,
        journal_path: tmp.path().join("journal.jsonl"),
        min_stable_secs: 1,
        zstd_bin: fake_zstd,
        exclude_suffixes: DEFAULT_EXCLUDE_SUFFIXES
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
    };

    let worker = thread::spawn(move || archive_logs(&options));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !signal.exists() {
        assert!(
            Instant::now() < deadline,
            "fake zstd never reached the decompression-verification phase"
        );
        thread::sleep(Duration::from_millis(10));
    }

    fs::remove_file(&source).expect("test should replace the reviewed path");
    fs::write(&source, &replacement).expect("replacement fixture should be writable");
    set_modified(&source, stable_mtime);

    let report = worker
        .join()
        .expect("archive worker should not panic")
        .expect("archive operation should return a report");

    assert!(
        source.exists(),
        "a replacement filesystem object with the same size/mtime must never be removed; report={report:?}"
    );
    assert_eq!(
        fs::read(&source).expect("replacement should remain readable"),
        replacement
    );
    assert_eq!(report.archived, 0, "replacement race must fail closed");
    assert!(report.results.iter().any(|result| {
        matches!(result.outcome, ArchiveOutcome::Failed(_))
    }));
}
