#![cfg(unix)]

use disksage_lib::duplicate_audit::collect_exact_duplicate_audit;
use disksage_lib::incomplete_download::{
    collect_incomplete_download_audit, DEFAULT_MAX_ENTRIES, DEFAULT_STALE_AFTER_DAYS,
};
use disksage_lib::incomplete_download_materialization::plan_incomplete_download_materialization;
use disksage_lib::incomplete_download_recovery::{
    validate_incomplete_download_recovery, RecoveryValidationLimits,
};
use disksage_lib::multipart_archive::collect_multipart_archive_audit;
use std::io::Write;
use std::os::unix::fs::symlink;
use std::path::Path;

const DAY_MS: u64 = 86_400_000;

fn system_time_ms(value: std::io::Result<std::time::SystemTime>) -> u64 {
    value
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        .unwrap_or_default()
}

fn write_zip(path: &Path, payload: &[u8]) {
    let mut bytes = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut bytes);
        let mut writer = zip::ZipWriter::new(cursor);
        writer
            .start_file(
                "payload.bin",
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(payload).unwrap();
        writer.finish().unwrap();
    }
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn read_only_root_entrypoints_reject_the_supplied_symlink_object() {
    let real_root = tempfile::tempdir().unwrap();
    write_zip(
        &real_root.path().join("recoverable.zip.crdownload"),
        b"validated payload",
    );

    let observed_at_ms = system_time_ms(std::fs::metadata(real_root.path()).unwrap().modified())
        + 31 * DAY_MS;
    let audit = collect_incomplete_download_audit(
        real_root.path(),
        observed_at_ms,
        DEFAULT_MAX_ENTRIES,
        DEFAULT_STALE_AFTER_DAYS,
    )
    .unwrap();
    let recovery = validate_incomplete_download_recovery(
        real_root.path(),
        &audit,
        observed_at_ms + 1,
        RecoveryValidationLimits::default(),
    )
    .unwrap();

    let link_parent = tempfile::tempdir().unwrap();
    let symlink_root = link_parent.path().join("selected-root");
    symlink(real_root.path(), &symlink_root).unwrap();

    assert_eq!(
        collect_exact_duplicate_audit(&symlink_root, observed_at_ms + 2, 1, 100).unwrap_err(),
        "duplicate-audit-root-unsafe"
    );
    assert_eq!(
        collect_multipart_archive_audit(&symlink_root, observed_at_ms + 2, 100).unwrap_err(),
        "multipart-audit-root-unsafe"
    );
    assert_eq!(
        collect_incomplete_download_audit(
            &symlink_root,
            observed_at_ms + 2,
            DEFAULT_MAX_ENTRIES,
            DEFAULT_STALE_AFTER_DAYS,
        )
        .unwrap_err(),
        "incomplete-download-audit-root-unsafe"
    );
    assert_eq!(
        validate_incomplete_download_recovery(
            &symlink_root,
            &audit,
            observed_at_ms + 2,
            RecoveryValidationLimits::default(),
        )
        .unwrap_err(),
        "recovery-validation-root-unsafe"
    );
    assert_eq!(
        plan_incomplete_download_materialization(
            &symlink_root,
            &audit,
            &recovery,
            observed_at_ms + 2,
        )
        .unwrap_err(),
        "materialization-root-unsafe"
    );
}
