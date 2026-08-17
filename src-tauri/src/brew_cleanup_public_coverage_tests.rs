use std::path::Path;

use crate::brew_cleanup::{
    execute, judge, plan as build_plan, prompt, write_audit_record, BrewCleanupAuditRecord,
    BrewCleanupPlan,
};
use crate::llm::InferenceEngine;

fn sample_plan() -> BrewCleanupPlan {
    BrewCleanupPlan {
        schema_version: 1,
        platform: "macos".into(),
        brew_path: "/opt/homebrew/bin/brew".into(),
        brew_identity: "1:2".into(),
        brew_version: "Homebrew 6.0.12".into(),
        dry_run_output: "Would remove old downloads".into(),
        dry_run_output_truncated: false,
        observed_at_ms: 10,
        plan_fingerprint: "a".repeat(64),
        exact_approval_phrase: format!("DiskSage Homebrew cleanup 승인 {}", "a".repeat(64)),
    }
}

fn sample_record() -> BrewCleanupAuditRecord {
    BrewCleanupAuditRecord {
        schema_version: 1,
        plan: sample_plan(),
        judgment_id: "judgment-test".into(),
        verdict: crate::llm::Verdict::Safe,
        reason: "fixed maintenance command".into(),
        model_name: "test-model".into(),
        judged_at_ms: 20,
        executed_at_ms: 30,
        approved_by: "human:local:test".into(),
        command: vec!["brew".into(), "cleanup".into(), "--prune-prefix".into()],
        status_code: 0,
        stdout: String::new(),
        stderr: String::new(),
        output_truncated: false,
        rationale: "approved after dry run".into(),
    }
}

struct FakeEngine(Result<String, String>);

impl InferenceEngine for FakeEngine {
    fn infer(&self, _prompt: &str) -> Result<String, String> {
        self.0.clone()
    }
}

#[test]
fn approval_phrase_returns_the_exact_bound_phrase() {
    let plan = sample_plan();
    assert_eq!(plan.approval_phrase(), plan.exact_approval_phrase);
}

#[test]
fn prompt_binds_the_fixed_command_and_marks_dry_run_as_untrusted_evidence() {
    let mut plan = sample_plan();
    plan.dry_run_output = "diagnostic-only: would remove one stale cache".into();

    let rendered = prompt(&plan);

    assert!(rendered.contains("untrusted diagnostic text"));
    assert!(rendered.contains("Executable: /opt/homebrew/bin/brew"));
    assert!(rendered.contains("Version: Homebrew 6.0.12"));
    assert!(rendered.contains("Exact command: brew cleanup --prune-prefix"));
    assert!(rendered.contains(&plan.dry_run_output));
    assert!(rendered.contains("safe|caution|keep"));
}

#[test]
fn judgment_is_bound_to_the_exact_plan_and_inference_failures_remain_unrated() {
    let plan = sample_plan();
    let safe = FakeEngine(Ok(
        r#"{"verdict":"safe","reason":"bounded maintenance evidence"}"#.into(),
    ));
    let safe_judgment = judge(&safe, &plan, 123);

    assert_eq!(safe_judgment.verdict, crate::llm::Verdict::Safe);
    assert_eq!(safe_judgment.reason, "bounded maintenance evidence");
    assert_eq!(safe_judgment.plan_fingerprint, plan.plan_fingerprint);
    assert_eq!(safe_judgment.exact_approval_phrase, plan.exact_approval_phrase);
    assert_eq!(safe_judgment.judged_at_ms, 123);
    assert_eq!(safe_judgment.judgment_id.len(), 64);

    let unavailable = FakeEngine(Err("model unavailable".into()));
    let unrated = judge(&unavailable, &plan, 124);
    assert_eq!(unrated.verdict, crate::llm::Verdict::Unrated);
    assert!(unrated.reason.is_empty());
    assert_ne!(unrated.judgment_id, safe_judgment.judgment_id);
}

#[cfg(not(target_os = "macos"))]
#[test]
fn non_macos_planning_and_execution_fail_closed() {
    assert_eq!(
        build_plan(10).unwrap_err(),
        "brew-cleanup-unsupported-platform"
    );
    assert_eq!(
        execute(&sample_plan(), "judgment-test", 30).unwrap_err(),
        "brew-cleanup-unsupported-platform"
    );
}

#[test]
fn audit_record_rejects_relative_and_parent_traversal_directories() {
    let record = sample_record();
    assert_eq!(
        write_audit_record(Path::new("relative-app-data"), &record).unwrap_err(),
        "brew-cleanup-audit-directory-invalid"
    );

    let temp = tempfile::tempdir().unwrap();
    let traversing = temp.path().join("child").join("..").join("escape");
    assert_eq!(
        write_audit_record(&traversing, &record).unwrap_err(),
        "brew-cleanup-audit-directory-invalid"
    );
}

#[test]
fn audit_record_rejects_a_non_directory_parent() {
    let temp = tempfile::tempdir().unwrap();
    let file_path = temp.path().join("not-a-directory");
    std::fs::write(&file_path, b"file").unwrap();

    assert_eq!(
        write_audit_record(&file_path, &sample_record()).unwrap_err(),
        "brew-cleanup-audit-parent-create-failed"
    );
}

#[test]
fn audit_record_rejects_oversized_serialized_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let mut record = sample_record();
    record.stdout = "x".repeat(130 * 1024);

    assert_eq!(
        write_audit_record(temp.path(), &record).unwrap_err(),
        "brew-cleanup-audit-too-large"
    );
    let audit_dir = temp.path().join("brew-cleanup-records");
    assert!(audit_dir.is_dir());
    assert_eq!(std::fs::read_dir(audit_dir).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn audit_record_rejects_symlinked_parent_and_record_directories() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let real_parent = temp.path().join("real-parent");
    std::fs::create_dir(&real_parent).unwrap();
    let symlink_parent = temp.path().join("symlink-parent");
    symlink(&real_parent, &symlink_parent).unwrap();
    assert_eq!(
        write_audit_record(&symlink_parent, &sample_record()).unwrap_err(),
        "brew-cleanup-audit-parent-unsafe"
    );

    let app_data = temp.path().join("app-data");
    let outside_records = temp.path().join("outside-records");
    std::fs::create_dir(&app_data).unwrap();
    std::fs::create_dir(&outside_records).unwrap();
    symlink(&outside_records, app_data.join("brew-cleanup-records")).unwrap();
    assert_eq!(
        write_audit_record(&app_data, &sample_record()).unwrap_err(),
        "brew-cleanup-audit-directory-unsafe"
    );
}
