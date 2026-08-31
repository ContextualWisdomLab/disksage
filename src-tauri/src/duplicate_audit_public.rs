//! Public exact-duplicate boundary with fail-closed policy for legacy private reports.
//!
//! Reports created before managed Photos-library exclusion existed can still be structurally valid.
//! Reapplying the current exclusion at approval and execution prevents stale evidence from granting
//! permanent-delete authority inside `.photoslibrary` or `.photolibrary` packages.

use std::path::{Component, Path};

pub(crate) use crate::duplicate_audit_implementation::active_duplicate_candidates;

pub use crate::duplicate_audit_implementation::{
    collect_exact_duplicate_audit, ExactDuplicateAuditCluster, ExactDuplicateAuditMember,
    ExactDuplicateAuditReport, ExactDuplicateAuditSummary, ExactDuplicateProductionMetadata,
    ExactDuplicateReclaimExecution, DEFAULT_MAX_ENTRIES, DEFAULT_MIN_BYTES,
    EXACT_DUPLICATE_AUDIT_VERSION, MAX_ENTRIES,
};

fn managed_photo_component(path: &Path) -> bool {
    path.components().any(|component| {
        let Component::Normal(name) = component else {
            return false;
        };
        let name = name.to_string_lossy().to_ascii_lowercase();
        name.ends_with(".photoslibrary") || name.ends_with(".photolibrary")
    })
}

fn report_contains_managed_photo_library(report: &ExactDuplicateAuditReport) -> bool {
    managed_photo_component(Path::new(&report.source_root))
        || report.clusters.iter().any(|cluster| {
            cluster
                .members
                .iter()
                .any(|member| managed_photo_component(Path::new(&member.relative_path)))
        })
}

fn live_report_scope_is_safe(source_root: &Path, report: &ExactDuplicateAuditReport) -> bool {
    let Ok(canonical_root) = std::fs::canonicalize(source_root) else {
        return false;
    };
    if managed_photo_component(&canonical_root) {
        return false;
    }
    report.clusters.iter().all(|cluster| {
        cluster.members.iter().all(|member| {
            let candidate = canonical_root.join(&member.relative_path);
            std::fs::canonicalize(candidate).is_ok_and(|canonical_candidate| {
                canonical_candidate.starts_with(&canonical_root)
                    && !managed_photo_component(&canonical_candidate)
            })
        })
    })
}

/// Validate both the immutable report structure and current destructive-policy exclusions.
pub fn exact_duplicate_audit_integrity_valid(report: &ExactDuplicateAuditReport) -> bool {
    !report_contains_managed_photo_library(report)
        && crate::duplicate_audit_implementation::exact_duplicate_audit_integrity_valid(report)
}

/// Never issue an approval phrase for legacy evidence that crosses a managed Photos-library scope.
pub fn exact_duplicate_reclaim_approval_phrase(
    report: &ExactDuplicateAuditReport,
) -> Option<String> {
    if report_contains_managed_photo_library(report) {
        None
    } else {
        crate::duplicate_audit_implementation::exact_duplicate_reclaim_approval_phrase(report)
    }
}

/// Redact destructive authority from summaries of legacy managed-library reports as well.
pub fn summarize_exact_duplicate_audit(
    report: &ExactDuplicateAuditReport,
) -> ExactDuplicateAuditSummary {
    let mut summary =
        crate::duplicate_audit_implementation::summarize_exact_duplicate_audit(report);
    if report_contains_managed_photo_library(report) {
        summary.reclaim_plan_fingerprint = None;
        summary.exact_reclaim_approval_phrase = None;
        summary
            .notices
            .push("system-managed-photo-library-reclaim-disabled".into());
    }
    summary
}

/// Collect fresh evidence, then pass it through the same current-policy execution boundary used for
/// immutable private reports.
#[cfg(not(coverage))]
pub fn execute_exact_duplicate_reclaim(
    source_root: &Path,
    min_bytes: u64,
    max_entries: usize,
    approved_audit_fingerprint: &str,
    confirmation_phrase: &str,
    rationale: &str,
    executed_at_ms: u64,
) -> Result<ExactDuplicateReclaimExecution, String> {
    let report = crate::duplicate_audit_implementation::collect_exact_duplicate_audit(
        source_root,
        executed_at_ms,
        min_bytes,
        max_entries,
    )?;
    execute_exact_duplicate_reclaim_from_report(
        source_root,
        &report,
        approved_audit_fingerprint,
        confirmation_phrase,
        rationale,
        executed_at_ms,
    )
}

/// Re-apply current managed-library policy before validating or acting on any historical report.
#[cfg(not(coverage))]
pub fn execute_exact_duplicate_reclaim_from_report(
    source_root: &Path,
    report: &ExactDuplicateAuditReport,
    approved_audit_fingerprint: &str,
    confirmation_phrase: &str,
    rationale: &str,
    executed_at_ms: u64,
) -> Result<ExactDuplicateReclaimExecution, String> {
    if report_contains_managed_photo_library(report) {
        return Err("duplicate-reclaim-system-managed-photo-library".into());
    }
    if !live_report_scope_is_safe(source_root, report) {
        return Err("duplicate-reclaim-live-source-scope-unsafe".into());
    }
    crate::duplicate_audit_implementation::execute_exact_duplicate_reclaim_from_report(
        source_root,
        report,
        approved_audit_fingerprint,
        confirmation_phrase,
        rationale,
        executed_at_ms,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_photo_components_are_case_insensitive_and_component_bounded() {
        assert!(managed_photo_component(Path::new(
            "nested/Library.PhOtOsLiBrArY/original.jpg"
        )));
        assert!(managed_photo_component(Path::new(
            "legacy/Library.photolibrary/database"
        )));
        assert!(!managed_photo_component(Path::new(
            "nested/not-a.photoslibrary-backup/original.jpg"
        )));
    }

    #[cfg(unix)]
    #[test]
    fn live_scope_rejects_parent_symlink_redirected_into_managed_photo_library() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let managed = tempfile::tempdir().unwrap();
        let managed_library = managed.path().join("Library.photoslibrary");
        std::fs::create_dir(&managed_library).unwrap();
        std::fs::write(managed_library.join("copy-a.bin"), b"same").unwrap();
        std::fs::write(managed_library.join("copy-b.bin"), b"same").unwrap();
        std::fs::create_dir(root.path().join("copies")).unwrap();
        std::fs::write(root.path().join("copies/copy-a.bin"), b"same").unwrap();
        std::fs::write(root.path().join("copies/copy-b.bin"), b"same").unwrap();
        let report = collect_exact_duplicate_audit(root.path(), 42, 1, 100).unwrap();

        std::fs::remove_file(root.path().join("copies/copy-a.bin")).unwrap();
        std::fs::remove_file(root.path().join("copies/copy-b.bin")).unwrap();
        std::fs::remove_dir(root.path().join("copies")).unwrap();
        symlink(&managed_library, root.path().join("copies")).unwrap();

        assert!(!live_report_scope_is_safe(root.path(), &report));
    }
}
