//! Read-only recovery coverage for real PNG and ZIP content validators.
//!
//! The fixtures are generated in temporary directories, are never renamed or deleted by DiskSage,
//! and deliberately exercise validation success, invalid content, unsupported content, and bounded
//! ZIP-limit failure through the public recovery API.

use disksage_lib::cloud_local_eviction::ActiveUseEvidence;
use disksage_lib::incomplete_download::{
    IncompleteDownloadAuditItem, IncompleteDownloadAuditReport, IncompleteDownloadState,
    DEFAULT_STALE_AFTER_DAYS, INCOMPLETE_DOWNLOAD_AUDIT_VERSION,
};
use disksage_lib::incomplete_download_recovery::{
    validate_incomplete_download_recovery, ContentValidationStatus, RecoveryItemStatus,
    RecoveryValidationKind, RecoveryValidationLimits,
};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

fn passive_use_evidence() -> ActiveUseEvidence {
    ActiveUseEvidence {
        method: "coverage-fixture".into(),
        evidence_complete: true,
        active: false,
        observed_pids: Vec::new(),
        results_truncated: false,
        error: None,
    }
}

fn modified_ms(path: &Path) -> u64 {
    std::fs::metadata(path)
        .unwrap()
        .modified()
        .unwrap_or(SystemTime::UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn candidate(
    fingerprint: &str,
    path: &Path,
    detected_mime_type: Option<&str>,
    structural_zip_candidates: Vec<String>,
) -> IncompleteDownloadAuditItem {
    let logical_bytes = std::fs::metadata(path).unwrap().len();
    IncompleteDownloadAuditItem {
        candidate_fingerprint: fingerprint.into(),
        relative_path: path.file_name().unwrap().to_string_lossy().into_owned(),
        logical_bytes,
        allocated_bytes: logical_bytes,
        filesystem_created_ms: 0,
        filesystem_modified_ms: modified_ms(path),
        modified_age_days: 31,
        staleness_basis: "filesystem-modified-time".into(),
        state: IncompleteDownloadState::StaleIdleRecoveryCandidate,
        active_use: passive_use_evidence(),
        evidence_complete: true,
        evidence_issues: Vec::new(),
        detected_mime_type: detected_mime_type.map(str::to_owned),
        detected_extension: Some("crdownload".into()),
        structural_zip_candidate_count: structural_zip_candidates.len() as u64,
        structural_zip_recoverable_bytes: 0,
        structural_zip_candidates,
        whole_file_structurally_complete_zip: false,
        zip_eocd_count: 0,
        zip_eocd_offsets: Vec::new(),
        download_acquired_dates: Vec::new(),
        download_agents: Vec::new(),
        download_origin_hosts: Vec::new(),
        production_time_evidence_present: false,
        final_sibling_relative_path: None,
        final_sibling_exists: false,
        final_sibling_bytes: None,
        recovery_candidate: true,
        partial_content_recovery_possible: false,
        requires_human_review: true,
        automatic_discard_allowed: false,
    }
}

fn audit(source_root: String, items: Vec<IncompleteDownloadAuditItem>) -> IncompleteDownloadAuditReport {
    let candidate_count = items.len();
    let candidate_bytes = items.iter().map(|item| item.logical_bytes).sum();
    IncompleteDownloadAuditReport {
        schema_version: INCOMPLETE_DOWNLOAD_AUDIT_VERSION,
        observed_at_ms: 1,
        source_root,
        source_scope_fingerprint: "a".repeat(64),
        stale_after_days: DEFAULT_STALE_AFTER_DAYS,
        evidence_complete: true,
        entries_seen: candidate_count,
        issue_counts: BTreeMap::new(),
        file_count: candidate_count,
        logical_bytes: candidate_bytes,
        allocated_bytes: candidate_bytes,
        active_count: 0,
        active_bytes: 0,
        evidence_incomplete_count: 0,
        evidence_incomplete_bytes: 0,
        recent_idle_count: 0,
        recent_idle_bytes: 0,
        stale_idle_count: candidate_count,
        stale_idle_bytes: candidate_bytes,
        recovery_candidate_count: candidate_count,
        recovery_candidate_bytes: candidate_bytes,
        structural_zip_candidate_item_count: 0,
        structural_zip_recoverable_bytes: 0,
        whole_file_structurally_complete_zip_count: 0,
        whole_file_structurally_complete_zip_bytes: 0,
        detected_type_count: 0,
        acquisition_date_evidence_count: 0,
        production_time_evidence_count: 0,
        final_sibling_count: 0,
        discard_review_bytes: candidate_bytes,
        audit_fingerprint: "b".repeat(64),
        mutation_performed: false,
        items,
    }
}

fn write_png(path: &Path) {
    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[1, 2, 3, 255]).unwrap();
    }
    std::fs::write(path, encoded).unwrap();
}

fn write_zip(path: &Path, payload: &[u8]) -> u64 {
    let file = File::create(path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    writer
        .start_file(
            "payload.bin",
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
    writer.write_all(payload).unwrap();
    writer.finish().unwrap();
    std::fs::metadata(path).unwrap().len()
}

#[test]
fn public_recovery_validates_png_and_zip_without_granting_mutation_authority() {
    let temp = tempfile::tempdir().unwrap();
    let png_path = temp.path().join("image.crdownload");
    let zip_path = temp.path().join("archive.crdownload");
    write_png(&png_path);
    let zip_len = write_zip(&zip_path, b"bounded-payload");

    let canonical = std::fs::canonicalize(temp.path()).unwrap();
    let report = validate_incomplete_download_recovery(
        temp.path(),
        &audit(
            canonical.to_string_lossy().into_owned(),
            vec![
                candidate("a-png", &png_path, Some("image/png"), Vec::new()),
                candidate(
                    "b-zip",
                    &zip_path,
                    Some("application/zip"),
                    vec![format!("start=0;end={zip_len};entries=1")],
                ),
            ],
        ),
        2,
        RecoveryValidationLimits::default(),
    )
    .unwrap();

    assert!(report.evidence_complete);
    assert_eq!(report.fully_validated_file_count, 2);
    assert_eq!(report.invalid_count, 0);
    assert_eq!(report.limit_exceeded_count, 0);
    assert_eq!(report.unsupported_count, 0);
    assert_eq!(report.skipped_count, 0);
    assert!(!report.mutation_performed);

    let png = report.items.iter().find(|item| item.candidate_fingerprint == "a-png").unwrap();
    assert_eq!(png.status, RecoveryItemStatus::FullyValidated);
    assert!(png.fully_validated_file);
    assert_eq!(png.validations.len(), 1);
    assert_eq!(png.validations[0].kind, RecoveryValidationKind::PngFullFile);
    assert_eq!(png.validations[0].status, ContentValidationStatus::Validated);
    assert_eq!(png.validations[0].decoded_frame_count, 1);

    let zip = report.items.iter().find(|item| item.candidate_fingerprint == "b-zip").unwrap();
    assert_eq!(zip.status, RecoveryItemStatus::FullyValidated);
    assert!(zip.fully_validated_file);
    assert_eq!(zip.validations.len(), 1);
    assert_eq!(zip.validations[0].kind, RecoveryValidationKind::ZipWholeFile);
    assert_eq!(zip.validations[0].status, ContentValidationStatus::Validated);
    assert_eq!(zip.validations[0].entry_count, 1);
    assert_eq!(zip.validations[0].validated_uncompressed_bytes, 15);

    for item in &report.items {
        assert!(item.requires_human_recovery_action);
        assert!(!item.automatic_rename_allowed);
        assert!(!item.automatic_discard_allowed);
    }
    assert!(png_path.exists());
    assert!(zip_path.exists());
}

#[test]
fn public_recovery_distinguishes_invalid_unsupported_and_zip_limit_exceeded_content() {
    let temp = tempfile::tempdir().unwrap();
    let invalid_png = temp.path().join("invalid-image.crdownload");
    let unsupported = temp.path().join("opaque.crdownload");
    let zip_path = temp.path().join("limited-archive.crdownload");
    std::fs::write(&invalid_png, b"not-a-png").unwrap();
    std::fs::write(&unsupported, b"opaque").unwrap();
    let zip_len = write_zip(&zip_path, b"payload");

    let canonical = std::fs::canonicalize(temp.path()).unwrap();
    let report = validate_incomplete_download_recovery(
        temp.path(),
        &audit(
            canonical.to_string_lossy().into_owned(),
            vec![
                candidate("a-invalid-png", &invalid_png, Some("image/png"), Vec::new()),
                candidate("b-unsupported", &unsupported, None, Vec::new()),
                candidate(
                    "c-limited-zip",
                    &zip_path,
                    Some("application/zip"),
                    vec![format!("start=0;end={zip_len};entries=2")],
                ),
            ],
        ),
        3,
        RecoveryValidationLimits {
            max_zip_entries: 1,
            ..RecoveryValidationLimits::default()
        },
    )
    .unwrap();

    assert!(report.evidence_complete);
    assert_eq!(report.invalid_count, 1);
    assert_eq!(report.unsupported_count, 1);
    assert_eq!(report.limit_exceeded_count, 1);
    assert_eq!(report.skipped_count, 0);
    assert!(!report.mutation_performed);

    let invalid = report
        .items
        .iter()
        .find(|item| item.candidate_fingerprint == "a-invalid-png")
        .unwrap();
    assert_eq!(invalid.status, RecoveryItemStatus::Invalid);
    assert_eq!(invalid.validations[0].reason_code, "png-decode-failed");

    let unsupported = report
        .items
        .iter()
        .find(|item| item.candidate_fingerprint == "b-unsupported")
        .unwrap();
    assert_eq!(unsupported.status, RecoveryItemStatus::Unsupported);
    assert_eq!(
        unsupported.reason_codes,
        vec!["no-supported-full-file-or-structural-range-validator"]
    );

    let limited = report
        .items
        .iter()
        .find(|item| item.candidate_fingerprint == "c-limited-zip")
        .unwrap();
    assert_eq!(limited.status, RecoveryItemStatus::LimitExceeded);
    assert_eq!(limited.validations.len(), 1);
    assert_eq!(limited.validations[0].reason_code, "zip-structural-entry-limit-exceeded");
    assert!(!limited.automatic_rename_allowed);
    assert!(!limited.automatic_discard_allowed);
}
