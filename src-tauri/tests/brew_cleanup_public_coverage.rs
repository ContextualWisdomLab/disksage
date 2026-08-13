//! Credential-free production coverage for the Homebrew cleanup public boundary.
//!
//! The Linux CI path must prove that macOS-only execution fails closed, while the portable
//! judgment and immutable-audit contracts remain deterministic and bounded. These tests never
//! invoke Homebrew, mutate user files, contact a model provider, or depend on OS credential state.

use disksage_lib::brew_cleanup::{
    execute, judge, plan, prompt, write_audit_record, BrewCleanupAuditRecord, BrewCleanupJudgment,
    BrewCleanupPlan, MAX_JUDGMENT_AGE_MS, SCHEMA_VERSION,
};
use disksage_lib::llm::{InferenceEngine, Verdict};

struct FakeEngine(Result<String, String>);

impl InferenceEngine for FakeEngine {
    fn infer(&self, _prompt: &str) -> Result<String, String> {
        self.0.clone()
    }
}

fn fixture_plan() -> BrewCleanupPlan {
    let fingerprint = "a".repeat(64);
    BrewCleanupPlan {
        schema_version: SCHEMA_VERSION,
        platform: "macos".into(),
        brew_path: "/opt/homebrew/bin/brew".into(),
        brew_identity: "1:2".into(),
        brew_version: "Homebrew 6.0.12".into(),
        dry_run_output: "Would remove old downloads".into(),
        dry_run_output_truncated: false,
        observed_at_ms: 10,
        plan_fingerprint: fingerprint.clone(),
        exact_approval_phrase: format!("DiskSage Homebrew cleanup 승인 {fingerprint}"),
    }
}

fn fixture_judgment(raw: Result<String, String>, judged_at_ms: u64) -> BrewCleanupJudgment {
    judge(&FakeEngine(raw), &fixture_plan(), judged_at_ms)
}

fn fixture_record() -> BrewCleanupAuditRecord {
    let judgment = fixture_judgment(
        Ok(r#"{"verdict":"safe","reason":"bounded maintenance evidence"}"#.into()),
        20,
    );
    BrewCleanupAuditRecord {
        schema_version: SCHEMA_VERSION,
        plan: judgment.plan.clone(),
        judgment_id: judgment.judgment_id.clone(),
        verdict: judgment.verdict,
        reason: judgment.reason.clone(),
        model_name: judgment.model_name.clone(),
        judged_at_ms: judgment.judged_at_ms,
        executed_at_ms: 30,
        approved_by: "human:local:test".into(),
        command: vec!["brew".into(), "cleanup".into(), "--prune-prefix".into()],
        status_code: 0,
        stdout: String::new(),
        stderr: String::new(),
        output_truncated: false,
        rationale: "approved after bounded dry run".into(),
    }
}

#[test]
fn public_plan_metadata_and_prompt_keep_fixed_authority() {
    let plan = fixture_plan();
    assert_eq!(plan.approval_phrase(), plan.exact_approval_phrase);
    assert_eq!(MAX_JUDGMENT_AGE_MS, 5 * 60 * 1_000);

    let rendered = prompt(&plan);
    assert!(rendered.contains("brew cleanup --prune-prefix"));
    assert!(rendered.contains("Would remove old downloads"));
    assert!(!rendered.contains("rm -rf"));
}

#[cfg(not(target_os = "macos"))]
#[test]
fn unsupported_platform_fails_before_homebrew_execution() {
    assert_eq!(
        plan(123).unwrap_err(),
        "brew-cleanup-unsupported-platform"
    );
    assert_eq!(
        execute(&fixture_plan(), "judgment-id", 456).unwrap_err(),
        "brew-cleanup-unsupported-platform"
    );
}

#[test]
fn judgment_maps_each_verdict_and_bounds_untrusted_reason_text() {
    for (raw, expected) in [
        (
            r#"{"verdict":"safe","reason":"safe reason"}"#,
            Verdict::Safe,
        ),
        (
            r#"{"verdict":"caution","reason":"caution reason"}"#,
            Verdict::Caution,
        ),
        (
            r#"{"verdict":"keep","reason":"keep reason"}"#,
            Verdict::Keep,
        ),
        ("not-json", Verdict::Unrated),
    ] {
        assert_eq!(fixture_judgment(Ok(raw.into()), 20).verdict, expected);
    }

    assert_eq!(
        fixture_judgment(Err("model unavailable".into()), 20).verdict,
        Verdict::Unrated
    );

    let long_reason = "r".repeat(1_500);
    let raw = format!(r#"{{"verdict":"safe","reason":"{long_reason}"}}"#);
    let bounded = fixture_judgment(Ok(raw), 20);
    assert_eq!(bounded.verdict, Verdict::Safe);
    assert_eq!(bounded.reason.chars().count(), 1_000);
    assert_eq!(bounded.plan_fingerprint, bounded.plan.plan_fingerprint);
    assert_eq!(bounded.exact_approval_phrase, bounded.plan.exact_approval_phrase);
    assert_eq!(bounded.schema_version, SCHEMA_VERSION);
    assert!(!bounded.judgment_id.is_empty());
}

#[test]
fn audit_publication_rejects_ambiguous_or_oversized_storage_before_authority_is_published() {
    let record = fixture_record();

    assert_eq!(
        write_audit_record(std::path::Path::new("relative/app-data"), &record).unwrap_err(),
        "brew-cleanup-audit-directory-invalid"
    );

    let temp = tempfile::tempdir().unwrap();
    let lexical_parent = temp.path().join("nested").join("..");
    assert_eq!(
        write_audit_record(&lexical_parent, &record).unwrap_err(),
        "brew-cleanup-audit-directory-invalid"
    );

    let mut oversized = record.clone();
    oversized.stdout = "x".repeat(140 * 1024);
    assert_eq!(
        write_audit_record(temp.path(), &oversized).unwrap_err(),
        "brew-cleanup-audit-too-large"
    );
    let records_dir = temp.path().join("brew-cleanup-records");
    assert!(records_dir.is_dir());
    assert_eq!(std::fs::read_dir(records_dir).unwrap().count(), 0);
}

#[test]
fn immutable_audit_publication_is_create_once_and_round_trips_bounded_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let record = fixture_record();
    let path = write_audit_record(temp.path(), &record).unwrap();
    let parsed: BrewCleanupAuditRecord =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(parsed, record);

    assert_eq!(
        write_audit_record(temp.path(), &record).unwrap_err(),
        "brew-cleanup-audit-create-failed"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o400
        );
    }
}