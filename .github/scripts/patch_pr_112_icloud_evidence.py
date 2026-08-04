"""Patch PR 112 so incomplete iCloud queue evidence always blocks readiness."""

from pathlib import Path
import sys


PATH = Path("src-tauri/src/naruon_cloud_copy_readiness.rs")


def replace_once(text: str, old: str, new: str, label: str) -> str:
    """Replace exactly one audited source block and fail closed on drift."""

    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected one block, found {count}")
    return text.replace(old, new, 1)


def add_tests(text: str) -> str:
    """Add regressions for incomplete and complete WAL-consistent iCloud evidence."""

    marker = '''    #[test]
    fn export_rejects_provider_switches_and_fingerprint_mutation() {'''
    tests = '''    #[test]
    fn incomplete_icloud_snapshot_never_authorizes_new_copy() {
        let report = report(CloudProvider::Icloud);
        let runtime = assess_provider_client_runtime(CloudProvider::Icloud, None, 25);
        let health = icloud_health(false);

        let envelope =
            export_naruon_cloud_copy_readiness(&report, &runtime, Some(&health)).unwrap();
        let admission = envelope.icloud_new_copy_admission.as_ref().unwrap();

        assert_eq!(envelope.icloud_new_copy_admission_met, Some(false));
        assert_eq!(admission.state, "blocked");
        assert_eq!(
            admission.blockers,
            vec!["icloud-new-copy-admission-evidence-unavailable"]
        );
        assert!(!admission.evidence_complete);
        assert!(!admission.database_snapshot_includes_wal);
        assert!(envelope
            .candidate_blocker_counts
            .contains_key("icloud-new-copy-admission-evidence-unavailable"));
    }

    #[test]
    fn complete_wal_consistent_icloud_snapshot_can_clear_only_the_admission_gate() {
        let report = report(CloudProvider::Icloud);
        let runtime = assess_provider_client_runtime(CloudProvider::Icloud, None, 25);
        let mut health = icloud_health(false);
        health.evidence_complete = true;
        health.database_snapshot_includes_wal = true;

        let envelope =
            export_naruon_cloud_copy_readiness(&report, &runtime, Some(&health)).unwrap();
        let admission = envelope.icloud_new_copy_admission.as_ref().unwrap();

        assert_eq!(envelope.icloud_new_copy_admission_met, Some(true));
        assert_eq!(admission.state, "clear");
        assert!(admission.blockers.is_empty());
        assert!(admission.evidence_complete);
        assert!(admission.database_snapshot_includes_wal);
        assert!(!envelope
            .candidate_blocker_counts
            .contains_key("icloud-new-copy-admission-evidence-unavailable"));
    }

    #[test]
    fn export_rejects_provider_switches_and_fingerprint_mutation() {'''
    return replace_once(text, marker, tests, "iCloud evidence regression insertion")


def apply_fix(text: str) -> str:
    """Make report and exported-summary evidence completeness fail closed."""

    text = replace_once(
        text,
        '''        || report.database_sidecar_write_permitted
        || report.evidence_complete
        || report.database_snapshot_includes_wal
    {''',
        '''        || report.database_sidecar_write_permitted
        || report.evidence_complete != report.database_snapshot_includes_wal
    {''',
        "report evidence claim validation",
    )
    text = replace_once(
        text,
        '''    let expected = expected_icloud_admission_blockers(report);
    let expected_state = if expected.is_empty() {
        "clear"
    } else {
        "blocked"
    };
    if report.new_copy_admission_state != expected_state
        || report.new_copy_admission_blockers != expected''',
        '''    let reported_blockers = expected_icloud_admission_blockers(report);
    let reported_state = if reported_blockers.is_empty() {
        "clear"
    } else {
        "blocked"
    };
    if report.new_copy_admission_state != reported_state
        || report.new_copy_admission_blockers != reported_blockers''',
        "reported queue evidence validation",
    )
    text = replace_once(
        text,
        '''    Ok((
        Some(IcloudNewCopyAdmissionSummary {''',
        '''    let mut exported_blockers = reported_blockers;
    if !report.evidence_complete || !report.database_snapshot_includes_wal {
        exported_blockers.push("icloud-new-copy-admission-evidence-unavailable".into());
    }
    let exported_state = if exported_blockers.is_empty() {
        "clear"
    } else {
        "blocked"
    };
    let admission_met = exported_blockers.is_empty();
    Ok((
        Some(IcloudNewCopyAdmissionSummary {''',
        "exported incomplete-evidence blocker",
    )
    text = replace_once(
        text,
        '''            state: report.new_copy_admission_state.clone(),''',
        '''            state: exported_state.into(),''',
        "exported admission state",
    )
    text = replace_once(
        text,
        '''            blockers: report.new_copy_admission_blockers.clone(),''',
        '''            blockers: exported_blockers,''',
        "exported admission blockers",
    )
    text = replace_once(
        text,
        '''        Some(expected.is_empty()),
    ))''',
        '''        Some(admission_met),
    ))''',
        "exported admission verdict",
    )
    text = replace_once(
        text,
        '''    let expected_state = if expected.is_empty() {
        "clear"
    } else {
        "blocked"
    };''',
        '''    if !summary.evidence_complete || !summary.database_snapshot_includes_wal {
        expected.push("icloud-new-copy-admission-evidence-unavailable".to_string());
    }
    let expected_state = if expected.is_empty() {
        "clear"
    } else {
        "blocked"
    };''',
        "summary incomplete-evidence blocker",
    )
    text = replace_once(
        text,
        '''    if summary.scheduled_count != scheduled_count
        || summary.scheduled_bytes != scheduled_bytes
        || summary.evidence_complete
        || summary.database_snapshot_includes_wal''',
        '''    if summary.scheduled_count != scheduled_count
        || summary.scheduled_bytes != scheduled_bytes
        || summary.evidence_complete != summary.database_snapshot_includes_wal''',
        "summary completeness validation",
    )
    return text


def main() -> None:
    """Apply the selected TDD phase to the exact reviewed source file."""

    if len(sys.argv) != 2 or sys.argv[1] not in {"tests", "fix"}:
        raise SystemExit("usage: patch_pr_112_icloud_evidence.py tests|fix")
    text = PATH.read_text(encoding="utf-8")
    if sys.argv[1] == "tests":
        text = add_tests(text)
    else:
        text = apply_fix(text)
    PATH.write_text(text, encoding="utf-8")


if __name__ == "__main__":
    main()
