//! Integration regression for privacy-safe Podman issue codes.
//!
//! Headless probe failures are untrusted local diagnostic strings. A missing delimiter must never
//! allow a path, socket, machine name, or command detail to cross the desktop IPC boundary.

use disksage_lib::podman_desktop::redact_podman_reclaim_plan;
use disksage_lib::podman_reclaim::{
    PodmanHostCompactionPlan, PodmanReclaimAssessment, PodmanReclaimPlan,
    PODMAN_RECLAIM_SCHEMA_KIND,
};

/// Builds the smallest public plan needed to exercise issue-code projection.
fn plan_with_issue(issue: &str) -> PodmanReclaimPlan {
    PodmanReclaimPlan {
        schema_kind: PODMAN_RECLAIM_SCHEMA_KIND,
        schema_version: 3,
        platform: "macos",
        evidence_complete: false,
        elapsed_ms: 1,
        machine: None,
        raw_image: None,
        guest_filesystem: None,
        store: None,
        system_df: None,
        unused_images: None,
        dangling_prune_approval_phrase: None,
        host_compaction: PodmanHostCompactionPlan {
            supported: false,
            machine_identity_sha256: None,
            backing_file_identity_sha256: None,
            backing_file_freshness_sha256: None,
            active_container_count: None,
            stop_command: None,
            compaction_command: None,
            exact_approval_phrase: None,
            rollback_policy: "require-runtime-native-rollback-before-execution",
            restart_policy: "restore-observed-running-state-after-success-or-rollback",
            blockers: vec!["active-container-count-must-be-zero".to_string()],
            execution_performed: false,
        },
        assessment: PodmanReclaimAssessment {
            physically_reclaimable_bytes: None,
            podman_reported_reclaimable_bytes: None,
            raw_allocated_minus_guest_used_bytes: None,
            status: "unverified".to_string(),
            reason_codes: vec![],
            recommended_actions: vec![],
        },
        issues: vec![issue.to_string()],
    }
}

/// Rejects delimiter-free local paths instead of serializing them as desktop issue codes.
#[test]
fn delimiter_free_private_issue_detail_falls_back_to_stable_code() {
    let evidence = redact_podman_reclaim_plan(plan_with_issue(
        "/Users/alice/.local/share/containers/private-machine.sock",
    ));

    assert_eq!(evidence.issue_codes, vec!["podman-evidence-error"]);
    let json = serde_json::to_string(&evidence).expect("desktop evidence must serialize");
    assert!(!json.contains("alice"));
    assert!(!json.contains("private-machine"));
    assert!(!json.contains("/Users/"));
}

/// Any projected issue forces completeness false even if an upstream caller contradicts it.
#[test]
fn projected_issue_codes_fail_completeness_closed() {
    let mut plan = plan_with_issue("podman-info-failed:/run/user/501/private.sock");
    plan.evidence_complete = true;

    let evidence = redact_podman_reclaim_plan(plan);

    assert_eq!(evidence.issue_codes, vec!["podman-info-failed"]);
    assert!(!evidence.evidence_complete);
}
