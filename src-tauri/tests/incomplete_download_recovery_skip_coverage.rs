//! Fail-closed recovery coverage for candidates that must never reach content validation.
//!
//! The fixtures exercise evidence, active-use, and relative-path authority boundaries without
//! opening candidate content or performing extraction, rename, discard, or any other mutation.

use disksage_lib::cloud_local_eviction::ActiveUseEvidence;
use disksage_lib::incomplete_download::{
    IncompleteDownloadAuditItem, IncompleteDownloadAuditReport, IncompleteDownloadState,
    DEFAULT_STALE_AFTER_DAYS, INCOMPLETE_DOWNLOAD_AUDIT_VERSION,
};
use disksage_lib::incomplete_download_recovery::{
    summarize_incomplete_download_recovery, validate_incomplete_download_recovery,
    RecoveryItemStatus, RecoveryValidationLimits,
};
use std::collections::BTreeMap;

fn active_use(evidence_complete: bool, active: bool) -> ActiveUseEvidence {
    ActiveUseEvidence {
        method: "coverage-fixture".into(),
        evidence_complete,
        active,
        observed_pids: Vec::new(),
        results_truncated: false,
        error: None,
    }
}

fn candidate(
    fingerprint: &str,
    relative_path: &str,
    evidence_complete: bool,
    active_use: ActiveUseEvidence,
) -> IncompleteDownloadAuditItem {
    IncompleteDownloadAuditItem {
        candidate_fingerprint: fingerprint.into(),
        relative_path: relative_path.into(),
        logical_bytes: 10,
        allocated_bytes: 10,
        filesystem_created_ms: 1,
        filesystem_modified_ms: 1,
        modified_age_days: 31,
        staleness_basis: "filesystem-modified-time".into(),
        state: IncompleteDownloadState::StaleIdleRecoveryCandidate,
        active_use,
        evidence_complete,
        evidence_issues: Vec::new(),
        detected_mime_type: None,
        detected_extension: Some("crdownload".into()),
        structural_zip_candidate_count: 0,
        structural_zip_candidates: Vec::new(),
        structural_zip_recoverable_bytes: 0,
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

#[test]
fn recovery_skips_incomplete_active_and_unsafe_candidates_before_content_access() {
    let temp = tempfile::tempdir().unwrap();
    let canonical_root = std::fs::canonicalize(temp.path()).unwrap();
    let audit = audit(
        canonical_root.to_string_lossy().into_owned(),
        vec![
            candidate(
                "a-incomplete",
                "missing-incomplete.crdownload",
                false,
                active_use(true, false),
            ),
            candidate(
                "b-active",
                "missing-active.crdownload",
                true,
                active_use(true, true),
            ),
            candidate(
                "c-unsafe",
                "../escape.crdownload",
                true,
                active_use(true, false),
            ),
        ],
    );

    let report = validate_incomplete_download_recovery(
        temp.path(),
        &audit,
        2,
        RecoveryValidationLimits::default(),
    )
    .unwrap();

    assert_eq!(report.candidate_count, 3);
    assert_eq!(report.skipped_count, 3);
    assert!(!report.evidence_complete);
    assert!(!report.mutation_performed);
    assert_eq!(
        report.issue_counts.get("audit-item-evidence-incomplete"),
        Some(&1)
    );
    assert_eq!(report.issue_counts.get("audit-item-active"), Some(&1));
    assert_eq!(
        report
            .issue_counts
            .get("recovery-candidate-relative-path-unsafe"),
        Some(&1)
    );

    assert_eq!(report.items[0].status, RecoveryItemStatus::SkippedEvidenceIncomplete);
    assert_eq!(report.items[1].status, RecoveryItemStatus::SkippedActive);
    assert_eq!(report.items[2].status, RecoveryItemStatus::SkippedEvidenceIncomplete);
    assert!(report
        .items
        .iter()
        .all(|item| item.validations.is_empty() && item.requires_human_recovery_action));
    assert!(report
        .items
        .iter()
        .all(|item| !item.automatic_rename_allowed && !item.automatic_discard_allowed));

    let summary = summarize_incomplete_download_recovery(&report);
    assert!(summary.human_recovery_action_required);
    assert!(!summary.automatic_rename_allowed);
    assert!(!summary.automatic_discard_allowed);
    let encoded = serde_json::to_string(&summary).unwrap();
    for sensitive in [
        canonical_root.to_string_lossy().as_ref(),
        "missing-incomplete.crdownload",
        "missing-active.crdownload",
        "../escape.crdownload",
    ] {
        assert!(!encoded.contains(sensitive), "summary leaked {sensitive}");
    }
}
