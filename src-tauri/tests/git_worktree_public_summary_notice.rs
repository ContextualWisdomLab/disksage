//! Public-summary behavior contract for completed sleeping Orca sessions.
//!
//! The cleanup notice is buyer guidance, not a global warning. It is emitted only when the same
//! audited entry is both an authenticated completed-PR commit and still owned by a sleeping Orca
//! session. Either fact on its own must not create cleanup guidance.

use disksage_lib::git_worktree::{
    public_summary, GitWorktreeActiveUseEvidence, GitWorktreeAuditEntry, GitWorktreeAuditReport,
    GitWorktreeDisposition, GitWorktreeReferenceBinding, GitWorktreeSizeEvidence,
    GIT_WORKTREE_AUDIT_SCHEMA_KIND, GIT_WORKTREE_AUDIT_VERSION,
};
use disksage_lib::reclaim_protection::REASON_ORCA_SESSION_SLEEPING;

const CLEANUP_NOTICE: &str =
    "sleep-session-requires-result-preserve-then-cleanup-then-reaudit";

fn report(completed_pull_request_commit: bool, sleeping: bool) -> GitWorktreeAuditReport {
    let blockers = if sleeping {
        vec![REASON_ORCA_SESSION_SLEEPING.to_string()]
    } else {
        Vec::new()
    };
    GitWorktreeAuditReport {
        schema_kind: GIT_WORKTREE_AUDIT_SCHEMA_KIND.into(),
        version: GIT_WORKTREE_AUDIT_VERSION,
        path_fingerprint_algorithm: "disksage.git-worktree-path/blake3-v2".into(),
        entry_fingerprint_algorithm: "disksage.git-worktree-entry/blake3-v3".into(),
        repository_root: "/private/repository".into(),
        common_dir: "/private/repository/.git".into(),
        generated_at_ms: 1,
        stale_open_pull_request_cutoff_ms: None,
        retention_references: vec![GitWorktreeReferenceBinding {
            reference_ref: "origin/develop".into(),
            reference_oid: "a".repeat(40),
        }],
        retention_reference_set_fingerprint: "r".repeat(64),
        removal_authority_fingerprint: "a".repeat(64),
        retention_reachable_commit_count: 1,
        worktree_count: 1,
        removal_candidate_count: 0,
        removal_candidate_allocated_bytes: 0,
        preserved_count: 1,
        evidence_gap_count: 0,
        evidence_complete: true,
        removal_plan_fingerprint: "f".repeat(64),
        exact_approval_phrase: None,
        entries: vec![GitWorktreeAuditEntry {
            path: "/private/repository/.git/worktrees/example".into(),
            path_fingerprint: "p".repeat(64),
            head: "b".repeat(40),
            branch: None,
            detached: true,
            bare: false,
            primary: false,
            audit_origin: false,
            locked: false,
            lock_reason: None,
            prunable: false,
            prunable_reason: None,
            status_clean: Some(true),
            status_entry_count: Some(0),
            contained_in_reference: Some(true),
            closed_pull_request_head: false,
            completed_pull_request_commit,
            open_pull_request_commit: false,
            stale_open_pull_request_head: false,
            head_is_retained_tip: false,
            actor_cwd_inside: Some(false),
            size: GitWorktreeSizeEvidence {
                method: "test".into(),
                evidence_complete: true,
                allocated_bytes: 0,
                logical_bytes: 0,
                visited_entries: 0,
                error: None,
            },
            active_use: GitWorktreeActiveUseEvidence {
                method: "test".into(),
                assessed: true,
                evidence_complete: true,
                active: false,
                observed_pids: Vec::new(),
                results_truncated: false,
                error: None,
            },
            disposition: GitWorktreeDisposition::Preserve,
            blockers,
            entry_fingerprint: "e".repeat(64),
        }],
        issues: Vec::new(),
        filesystem_mutation_executed: false,
    }
}

#[test]
fn completed_sleeping_session_emits_cleanup_notice() {
    let summary = public_summary(&report(true, true));
    assert!(summary.notices.iter().any(|notice| notice == CLEANUP_NOTICE));
}

#[test]
fn cleanup_notice_requires_completed_and_sleeping_on_the_same_entry() {
    for report in [report(true, false), report(false, true), report(false, false)] {
        let summary = public_summary(&report);
        assert!(
            summary.notices.iter().all(|notice| notice != CLEANUP_NOTICE),
            "cleanup guidance must be conditional: {summary:#?}"
        );
    }
}
