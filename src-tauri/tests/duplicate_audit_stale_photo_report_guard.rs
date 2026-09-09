use disksage_lib::content_digest::ContentDigests;
use disksage_lib::duplicate_audit::{
    exact_duplicate_reclaim_approval_phrase, execute_exact_duplicate_reclaim_from_report,
    ExactDuplicateAuditCluster, ExactDuplicateAuditMember, ExactDuplicateAuditReport,
    ExactDuplicateProductionMetadata, EXACT_DUPLICATE_AUDIT_VERSION,
};
use std::collections::BTreeMap;
use std::path::Path;

fn stale_managed_photo_report() -> ExactDuplicateAuditReport {
    let metadata = ExactDuplicateProductionMetadata {
        production_time_ms: 1,
        production_time_source: "filesystem:modified-fallback".into(),
        production_time_confidence: "low".into(),
        embedded_production_time_ms: None,
        filename_date_ms: None,
        title: None,
        authors: Vec::new(),
        context: Vec::new(),
        duration_ms: None,
        embedded_evidence: Vec::new(),
        metadata_probe_complete: true,
    };
    let member = ExactDuplicateAuditMember {
        member_fingerprint: "member".into(),
        metadata_fingerprint: "metadata".into(),
        relative_path: "Library.photoslibrary/original.jpg".into(),
        logical_bytes: 4,
        filesystem_created_ms: 1,
        filesystem_modified_ms: 1,
        production_metadata: metadata,
        storage_identity_fingerprint: None,
        source_stable: true,
        path_identity_verified: false,
        write_performed: false,
    };
    let cluster = ExactDuplicateAuditCluster {
        cluster_fingerprint: "cluster".into(),
        content_digests: ContentDigests {
            blake3: "blake3".into(),
            sha256: "sha256".into(),
            quick_xor_base64: "quickxor".into(),
        },
        logical_bytes_per_file: 4,
        file_count: 2,
        logical_duplicate_bytes: 8,
        logical_redundant_bytes: 4,
        distinct_storage_identity_count: None,
        physical_reclaimable_bytes: None,
        requires_human_canonical_selection: true,
        automatic_delete_allowed: false,
        members: vec![member],
    };
    ExactDuplicateAuditReport {
        schema_version: EXACT_DUPLICATE_AUDIT_VERSION,
        observed_at_ms: 1,
        source_root: "/tmp/disksage-stale-photo-report".into(),
        source_scope_fingerprint: "scope".into(),
        min_bytes: 1,
        max_entries: 10,
        evidence_complete: true,
        entries_seen: 2,
        file_count: 2,
        size_collision_candidate_count: 2,
        content_hashed_file_count: 2,
        cluster_count: 1,
        duplicate_file_count: 2,
        logical_duplicate_bytes: 8,
        logical_redundant_bytes: 4,
        physical_reclaimable_bytes: None,
        metadata_evidence_complete: true,
        production_time_source_counts: BTreeMap::new(),
        issue_counts: BTreeMap::new(),
        audit_fingerprint: "audit".into(),
        production_metadata_evaluated: true,
        production_date_policy: "embedded>filename-explicit>filesystem-created>filesystem-modified".into(),
        exact_content_match_is_delete_approval: false,
        automatic_delete_allowed: false,
        mutation_performed: false,
        clusters: vec![cluster],
    }
}

#[test]
fn stale_reports_with_managed_photo_members_never_grant_reclaim_authority() {
    let report = stale_managed_photo_report();
    assert_eq!(exact_duplicate_reclaim_approval_phrase(&report), None);
    assert_eq!(
        execute_exact_duplicate_reclaim_from_report(
            Path::new(&report.source_root),
            &report,
            &report.audit_fingerprint,
            "stale approval",
            "reject stale managed-library authority",
            2,
        )
        .expect_err("managed photo-library evidence must fail closed before reclaim validation"),
        "duplicate-reclaim-system-managed-photo-library"
    );
}
