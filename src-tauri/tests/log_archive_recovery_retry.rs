#![cfg(unix)]

use disksage_lib::log_archive::{
    archive_logs, ArchiveOutcome, LogArchiveOptions, DEFAULT_EXCLUDE_SUFFIXES,
};
use serde_json::json;
use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tempfile::tempdir;

fn write_verify_only_zstd(script_path: &Path) {
    let script = "#!/bin/sh\nset -eu\ncase \"$1\" in\n  -t)\n    exit 0\n    ;;\n  -dc)\n    cat \"$3\"\n    ;;\n  -q)\n    # A recovery retry must consume the already-published archive, not recompress or overwrite it.\n    exit 65\n    ;;\n  *)\n    exit 64\n    ;;\nesac\n";
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

fn append_verified_retirement_pending(journal: &Path, source: &Path, bytes: u64) {
    let entry = json!({
        "ts_ms": 1_u64,
        "op": "log_archive_compress",
        "path": source.to_string_lossy(),
        "bytes": bytes,
        "outcome": "archive_verified_retirement_pending"
    });
    fs::write(
        journal,
        format!("{}\n", serde_json::to_string(&entry).expect("journal entry must serialize")),
    )
    .expect("recovery ledger fixture should be writable");
}

#[test]
fn verified_existing_archive_retries_identity_bound_source_retirement() {
    let tmp = tempdir().expect("temporary root should be available");
    let root = tmp.path().join("logs");
    fs::create_dir_all(&root).expect("fixture root should be creatable");

    let source = root.join("session.jsonl");
    let source_bytes = b"{\"session\":1,\"message\":\"retry retirement\"}\n".repeat(512);
    fs::write(&source, &source_bytes).expect("source fixture should be writable");
    set_old_enough(&source);

    // Model the durable state left after archive publication/verification succeeded but the
    // reversible Trash retirement did not complete. The source and verified archive both exist,
    // and the ledger records that this exact source path reached the verified-retirement boundary.
    let archive = archive_path_for(&source);
    fs::write(&archive, &source_bytes).expect("verified archive fixture should be writable");
    let archive_before = fs::read(&archive).expect("archive should be readable before retry");

    let zstd = tmp.path().join("verify-only-zstd");
    write_verify_only_zstd(&zstd);
    let journal = tmp.path().join("journal.jsonl");
    append_verified_retirement_pending(&journal, &source, source_bytes.len() as u64);
    let options = options_for(&root, &journal, &zstd);

    let report = archive_logs(&options).expect("recovery retry should return a report");

    assert_eq!(
        report.archived, 1,
        "a ledger-correlated, verified matching archive must allow identity-bound Trash retirement to resume; report={report:?}"
    );
    assert!(
        !source.exists(),
        "the unchanged reviewed source should be retired through the canonical Trash path"
    );
    assert_eq!(
        fs::read(&archive).expect("verified archive must remain readable"),
        archive_before,
        "recovery must never overwrite the already-published archive"
    );
    assert!(report.results.iter().any(|result| {
        result.path == source.to_string_lossy().as_ref()
            && matches!(result.outcome, ArchiveOutcome::Archived)
    }));
    let journal_text = fs::read_to_string(&journal).expect("destructive retry must be journaled");
    assert!(journal_text.contains("archive_verified_retirement_pending"));
    assert!(journal_text.contains("trash_delete"));
    assert!(journal_text.contains("\"outcome\":\"ok\""));

    // A successful retry must consume its one-shot ledger authority. Reusing the same pathname
    // for a new filesystem object, even with identical bytes, must not replay the old retirement.
    fs::write(&source, &source_bytes).expect("replacement source fixture should be writable");
    set_old_enough(&source);
    let replay = archive_logs(&options).expect("stale-ledger replay should return a report");
    assert_eq!(
        replay.archived, 0,
        "completed retirement evidence must not authorize a later filesystem object"
    );
    assert!(source.exists(), "new source object must survive stale ledger evidence");
}

#[test]
fn matching_archive_without_retirement_ledger_never_authorizes_source_retirement() {
    let tmp = tempdir().expect("temporary root should be available");
    let root = tmp.path().join("logs");
    fs::create_dir_all(&root).expect("fixture root should be creatable");

    let source = root.join("session.jsonl");
    let source_bytes = b"same bytes but no DiskSage retirement provenance\n".repeat(256);
    fs::write(&source, &source_bytes).expect("source fixture should be writable");
    set_old_enough(&source);

    let archive = archive_path_for(&source);
    fs::write(&archive, &source_bytes).expect("matching foreign archive fixture should be writable");

    let zstd = tmp.path().join("verify-only-zstd");
    write_verify_only_zstd(&zstd);
    let journal = tmp.path().join("journal.jsonl");

    let report = archive_logs(&options_for(&root, &journal, &zstd))
        .expect("unproven sibling should fail closed in the report");

    assert_eq!(report.archived, 0, "content equality alone must not grant deletion authority");
    assert_eq!(fs::read(&source).expect("source must remain"), source_bytes);
    assert_eq!(fs::read(&archive).expect("archive must remain"), source_bytes);
}

#[test]
fn foreign_existing_archive_never_authorizes_source_retirement() {
    let tmp = tempdir().expect("temporary root should be available");
    let root = tmp.path().join("logs");
    fs::create_dir_all(&root).expect("fixture root should be creatable");

    let source = root.join("session.jsonl");
    let source_bytes = b"reviewed source\n".repeat(512);
    fs::write(&source, &source_bytes).expect("source fixture should be writable");
    set_old_enough(&source);

    let archive = archive_path_for(&source);
    let foreign_archive = b"different archive bytes\n".repeat(512);
    fs::write(&archive, &foreign_archive).expect("foreign archive fixture should be writable");

    let zstd = tmp.path().join("verify-only-zstd");
    write_verify_only_zstd(&zstd);
    let journal = tmp.path().join("journal.jsonl");
    append_verified_retirement_pending(&journal, &source, source_bytes.len() as u64);

    let report = archive_logs(&options_for(&root, &journal, &zstd))
        .expect("mismatched sibling should fail closed in the report");

    assert_eq!(report.archived, 0, "foreign archive must not authorize retirement");
    assert_eq!(
        fs::read(&source).expect("source must remain"),
        source_bytes,
        "mismatched archive must leave the reviewed source untouched"
    );
    assert_eq!(
        fs::read(&archive).expect("foreign archive must remain"),
        foreign_archive,
        "DiskSage must not overwrite or delete a pre-existing foreign archive"
    );
}
