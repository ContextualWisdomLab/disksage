//! Real-filesystem contracts for live Orca protection in the canonical v4 worktree audit.
//!
//! The linked worktree is otherwise safely removable because its HEAD is already contained in the
//! selected retention reference. Supplying live Orca terminal evidence must therefore be the only
//! reason it is preserved. This keeps live protection evidence inside the same audit/fingerprint
//! boundary that later execution re-audits and verifies the shipped CLIs can acquire that evidence.

use disksage_lib::git_worktree::{
    approve_stale_worktree_removal, audit_git_worktrees, execute_stale_worktree_removal,
    GitWorktreeAuditEntry, GitWorktreeAuditOptions, GitWorktreeAuditReport,
    GitWorktreeDisposition,
};
use disksage_lib::reclaim_protection::{ProtectionContext, REASON_ORCA_TERMINAL_LIVE};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

fn git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_AUTHOR_NAME", "DiskSage Test")
        .env("GIT_AUTHOR_EMAIL", "disksage@example.invalid")
        .env("GIT_COMMITTER_NAME", "DiskSage Test")
        .env("GIT_COMMITTER_EMAIL", "disksage@example.invalid")
        .output()
        .expect("git should spawn");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn entry_for_path<'a>(report: &'a GitWorktreeAuditReport, path: &Path) -> &'a GitWorktreeAuditEntry {
    let canonical = fs::canonicalize(path).expect("canonicalize linked worktree");
    report
        .entries
        .iter()
        .find(|entry| Path::new(&entry.path) == canonical.as_path())
        .expect("linked worktree must be present in audit")
}

fn real_linked_worktree() -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let temp = tempfile::tempdir().expect("temporary fixture root");
    let repository = temp.path().join("repository");
    let secondary = temp.path().join("secondary");
    fs::create_dir(&repository).expect("create repository root");

    git(&repository, &["init", "-q", "-b", "main"]);
    fs::write(repository.join("evidence.txt"), b"first\n").expect("write first revision");
    git(&repository, &["add", "evidence.txt"]);
    git(&repository, &["commit", "-q", "-m", "first"]);
    fs::write(repository.join("evidence.txt"), b"second\n").expect("write second revision");
    git(&repository, &["commit", "-q", "-am", "second"]);
    git(&repository, &["branch", "stale", "HEAD~1"]);
    git(
        &repository,
        &["worktree", "add", "-q", secondary.to_str().unwrap(), "stale"],
    );
    (temp, repository, secondary)
}

#[test]
fn live_orca_terminal_preserves_otherwise_removable_worktree() {
    let (_temp, repository, secondary) = real_linked_worktree();

    let baseline = audit_git_worktrees(
        &repository,
        &["main".into()],
        GitWorktreeAuditOptions::default(),
        now_ms(),
    )
    .expect("baseline audit real linked worktree without live Orca evidence");
    let baseline_entry = entry_for_path(&baseline, &secondary);
    assert_eq!(
        baseline_entry.disposition,
        GitWorktreeDisposition::RemovalCandidate,
        "{baseline_entry:#?}"
    );
    let baseline_fingerprint = baseline_entry.entry_fingerprint.clone();

    let live_path = fs::canonicalize(&secondary).expect("canonical live worktree path");
    let options = GitWorktreeAuditOptions {
        protection: ProtectionContext {
            orca_live_worktree_paths: vec![live_path],
            ..ProtectionContext::default()
        },
        ..GitWorktreeAuditOptions::default()
    };
    let report = audit_git_worktrees(&repository, &["main".into()], options, now_ms())
        .expect("audit real linked worktree with live Orca evidence");
    let entry = entry_for_path(&report, &secondary);

    assert_eq!(entry.status_clean, Some(true), "{entry:#?}");
    assert_eq!(entry.contained_in_reference, Some(true), "{entry:#?}");
    assert!(
        entry
            .blockers
            .iter()
            .any(|blocker| blocker == REASON_ORCA_TERMINAL_LIVE),
        "blockers={:?}",
        entry.blockers
    );
    assert_eq!(entry.disposition, GitWorktreeDisposition::Preserve);
    assert_ne!(
        entry.entry_fingerprint, baseline_fingerprint,
        "protection authority that changes the decision must change entry integrity"
    );
    assert_eq!(report.removal_candidate_count, 0, "{report:#?}");
    assert_eq!(report.exact_approval_phrase, None);
}

#[test]
fn live_orca_terminal_arriving_after_approval_vetoes_removal_before_mutation() {
    let (_temp, repository, secondary) = real_linked_worktree();
    let preserved_file = secondary.join("evidence.txt");
    let preserved_bytes = fs::read(&preserved_file).expect("snapshot linked worktree file");

    let audited_at_ms = now_ms();
    let approved_report = audit_git_worktrees(
        &repository,
        &["main".into()],
        GitWorktreeAuditOptions::default(),
        audited_at_ms,
    )
    .expect("initial audit should observe an otherwise-removable linked worktree");
    assert_eq!(approved_report.removal_candidate_count, 1, "{approved_report:#?}");
    let approval_phrase = approved_report
        .exact_approval_phrase
        .as_deref()
        .expect("otherwise-removable worktree must require an exact approval phrase");
    let approved_at_ms = audited_at_ms.saturating_add(1);
    let approval = approve_stale_worktree_removal(
        &approved_report,
        approval_phrase,
        approved_at_ms,
        "human:disksage-test",
        "Verify that newly observed live protection vetoes deletion before mutation.",
    )
    .expect("bind approval to the initial removable plan");

    let live_path = fs::canonicalize(&secondary).expect("canonical live worktree path");
    let execution_options = GitWorktreeAuditOptions {
        protection: ProtectionContext {
            orca_live_worktree_paths: vec![live_path],
            ..ProtectionContext::default()
        },
        ..GitWorktreeAuditOptions::default()
    };
    let execution = execute_stale_worktree_removal(
        &approved_report,
        &approval,
        approval_phrase,
        execution_options,
        approved_at_ms.saturating_add(1),
    );

    assert!(
        execution.is_err(),
        "a worktree that becomes live after approval must fail closed before mutation"
    );
    assert!(secondary.exists(), "live worktree path must not be removed");
    assert_eq!(
        fs::read(&preserved_file).expect("read preserved worktree file"),
        preserved_bytes,
        "failed removal must not mutate the linked worktree contents"
    );
}

#[test]
fn shipped_audit_cli_acquires_live_protection_inputs_and_requires_explicit_recent_window() {
    let (temp, repository, secondary) = real_linked_worktree();
    let live_path = fs::canonicalize(&secondary).expect("canonical live worktree path");
    let terminal_json = temp.path().join("orca-terminals.json");
    fs::write(
        &terminal_json,
        serde_json::to_vec(&serde_json::json!({
            "result": {"terminals": [{"worktreePath": live_path}]}
        }))
        .expect("serialize terminal fixture"),
    )
    .expect("write terminal fixture");
    let lead_queue = temp.path().join("LEAD_QUEUE.md");
    fs::write(&lead_queue, "# empty-but-real lead queue fixture\n").expect("write lead queue fixture");

    let binary = env!("CARGO_BIN_EXE_disksage-git-worktree-audit");
    let output = Command::new(binary)
        .arg("--repository-root")
        .arg(&repository)
        .arg("--reference-ref")
        .arg("main")
        .arg("--enable-orca-protections")
        .arg("--recent-write-window-secs")
        .arg("3600")
        .arg("--orca-terminal-json")
        .arg(&terminal_json)
        .arg("--open-pr-head-oid")
        .arg("0000000000000000000000000000000000000000")
        .arg("--open-pr-head-branch")
        .arg("stale")
        .arg("--lead-queue-file")
        .arg(&lead_queue)
        .arg("--assess-filesystem-protections")
        .arg("--assess-unpushed-commits")
        .arg("--assess-stash")
        .output()
        .expect("shipped worktree audit CLI should start");
    assert!(
        output.status.success(),
        "CLI must acquire protection evidence instead of rejecting the buyer-facing inputs: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("CLI output must remain public JSON");
    let reasons = summary["protection_reason_codes"]
        .as_array()
        .expect("public summary protection reason codes");
    assert!(
        reasons.iter().any(|reason| reason == REASON_ORCA_TERMINAL_LIVE),
        "summary={summary:#}"
    );
    assert_eq!(summary["removal_candidate_count"], 0);
    assert!(summary["exact_approval_phrase"].is_null());

    let no_window = Command::new(binary)
        .arg("--repository-root")
        .arg(&repository)
        .arg("--reference-ref")
        .arg("main")
        .arg("--enable-orca-protections")
        .arg("--orca-terminal-json")
        .arg(&terminal_json)
        .output()
        .expect("shipped worktree audit CLI should start for fail-closed validation");
    assert!(!no_window.status.success());
    assert!(
        String::from_utf8_lossy(&no_window.stderr).contains("--recent-write-window-secs"),
        "Orca protection pack must require an explicit recent-write window; stderr={}",
        String::from_utf8_lossy(&no_window.stderr)
    );
}

#[test]
fn shipped_remove_cli_reacquires_live_protection_before_mutation() {
    let (temp, repository, secondary) = real_linked_worktree();
    let preserved_file = secondary.join("evidence.txt");
    let preserved_bytes = fs::read(&preserved_file).expect("snapshot linked worktree file");

    let audit_binary = env!("CARGO_BIN_EXE_disksage-git-worktree-audit");
    let baseline = Command::new(audit_binary)
        .arg("--repository-root")
        .arg(&repository)
        .arg("--reference-ref")
        .arg("main")
        .output()
        .expect("shipped audit CLI should produce the reviewed removal plan");
    assert!(
        baseline.status.success(),
        "baseline audit must be executable: {}",
        String::from_utf8_lossy(&baseline.stderr)
    );
    let baseline_summary: serde_json::Value =
        serde_json::from_slice(&baseline.stdout).expect("baseline audit public JSON");
    assert_eq!(baseline_summary["removal_candidate_count"], 1, "{baseline_summary:#}");
    let plan_fingerprint = baseline_summary["removal_plan_fingerprint"]
        .as_str()
        .expect("reviewed plan fingerprint");
    let approval_phrase = baseline_summary["exact_approval_phrase"]
        .as_str()
        .expect("reviewed exact approval phrase");

    let live_path = fs::canonicalize(&secondary).expect("canonical live worktree path");
    let terminal_json = temp.path().join("orca-terminals-after-review.json");
    fs::write(
        &terminal_json,
        serde_json::to_vec(&serde_json::json!({
            "result": {"terminals": [{"worktreePath": live_path}]}
        }))
        .expect("serialize post-review terminal fixture"),
    )
    .expect("write post-review terminal fixture");
    let record_root = temp.path().join("records");

    let remove_binary = env!("CARGO_BIN_EXE_disksage-git-worktree-remove");
    let removal = Command::new(remove_binary)
        .arg("--repository-root")
        .arg(&repository)
        .arg("--reference-ref")
        .arg("main")
        .arg("--enable-orca-protections")
        .arg("--recent-write-window-secs")
        .arg("3600")
        .arg("--orca-terminal-json")
        .arg(&terminal_json)
        .arg("--approved-removal-plan-fingerprint")
        .arg(plan_fingerprint)
        .arg("--confirmation-exact-approval-phrase")
        .arg(approval_phrase)
        .arg("--reviewed-by")
        .arg("human:disksage-test")
        .arg("--rationale")
        .arg("Verify post-review live protection before mutation.")
        .arg("--record-root")
        .arg(&record_root)
        .output()
        .expect("shipped remove CLI should start");
    assert!(!removal.status.success(), "new live protection must veto removal");
    let removal_stderr = String::from_utf8_lossy(&removal.stderr);
    assert!(
        removal_stderr.contains("git-worktree-removal-plan-fingerprint-mismatch"),
        "remove CLI must reacquire live protection and reject the reviewed plan before approval/mutation; stderr={removal_stderr}"
    );
    assert!(
        !removal_stderr.contains("unknown option"),
        "protection inputs must be a supported mutation-time buyer contract; stderr={removal_stderr}"
    );
    assert!(secondary.exists(), "live worktree path must remain after the veto");
    assert_eq!(
        fs::read(&preserved_file).expect("read worktree after remove-CLI veto"),
        preserved_bytes,
        "remove-CLI veto must preserve the exact checked-out bytes"
    );
    assert!(
        !record_root.exists(),
        "plan mismatch caused by fresh protection must happen before approval/result records"
    );

    let no_window = Command::new(remove_binary)
        .arg("--repository-root")
        .arg(&repository)
        .arg("--reference-ref")
        .arg("main")
        .arg("--enable-orca-protections")
        .arg("--orca-terminal-json")
        .arg(&terminal_json)
        .arg("--approved-removal-plan-fingerprint")
        .arg(plan_fingerprint)
        .arg("--confirmation-exact-approval-phrase")
        .arg(approval_phrase)
        .arg("--reviewed-by")
        .arg("human:disksage-test")
        .arg("--rationale")
        .arg("No implicit recent-write window is permitted.")
        .arg("--record-root")
        .arg(&record_root)
        .output()
        .expect("shipped remove CLI should start for fail-closed validation");
    assert!(!no_window.status.success());
    assert!(
        String::from_utf8_lossy(&no_window.stderr).contains("--recent-write-window-secs"),
        "remove CLI Orca pack must require an explicit recent-write window; stderr={}",
        String::from_utf8_lossy(&no_window.stderr)
    );
}
