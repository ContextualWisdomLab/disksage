//! Process-level resource-bound contracts for the shipped Maven cache CLIs.
//!
//! The operational CLI default of two million scanned entries is also the maximum authority the
//! command line may grant. Oversized values must fail during argument admission, before DiskSage
//! attempts to inspect the supplied repository path. Candidate and issue output cardinality is
//! separately bounded so a caller cannot request an unbounded in-memory/public evidence document.

use std::process::Command;

const MAX_MAVEN_CACHE_ENTRIES: u64 = 2_000_000;
const MAX_MAVEN_CACHE_OUTPUT_ITEMS: usize = 10_000;

fn assert_oversized_max_entries_fails_before_repository_access(
    binary: &str,
    extra_args: &[String],
) {
    let parent = tempfile::tempdir().expect("resource-bound fixture parent must be created");
    let unavailable_repository = parent.path().join("repository-does-not-exist");
    let oversized = (MAX_MAVEN_CACHE_ENTRIES + 1).to_string();

    let output = Command::new(binary)
        .env_remove("HOME")
        .arg("--repository-root")
        .arg(&unavailable_repository)
        .args(extra_args)
        .args(["--max-entries", oversized.as_str()])
        .output()
        .expect("DiskSage Maven CLI must launch for resource-bound validation");

    assert_eq!(
        output.status.code(),
        Some(2),
        "oversized scan authority must use the ordinary bounded argument-error exit"
    );
    assert!(
        output.stdout.is_empty(),
        "oversized scan authority must not emit a successful Maven evidence document"
    );
    assert_eq!(
        String::from_utf8(output.stderr).expect("CLI diagnostics must remain valid UTF-8"),
        format!(
            "--max-entries는 1..={MAX_MAVEN_CACHE_ENTRIES} 범위여야 함\n"
        ),
        "the parser must reject the oversized bound before touching the unavailable repository"
    );
}

fn assert_oversized_audit_output_bound_fails_before_repository_access(flag: &str) {
    let parent = tempfile::tempdir().expect("resource-bound fixture parent must be created");
    let unavailable_repository = parent.path().join("repository-does-not-exist");
    let oversized = (MAX_MAVEN_CACHE_OUTPUT_ITEMS + 1).to_string();

    let output = Command::new(env!("CARGO_BIN_EXE_disksage-maven-cache-audit"))
        .env_remove("HOME")
        .arg("--repository-root")
        .arg(&unavailable_repository)
        .args([flag, oversized.as_str()])
        .output()
        .expect("DiskSage Maven audit CLI must launch for output-bound validation");

    assert_eq!(
        output.status.code(),
        Some(2),
        "oversized output authority must use the ordinary bounded argument-error exit"
    );
    assert!(
        output.stdout.is_empty(),
        "oversized output authority must not emit a successful Maven evidence document"
    );
    assert_eq!(
        String::from_utf8(output.stderr).expect("CLI diagnostics must remain valid UTF-8"),
        format!("{flag}는 0..={MAX_MAVEN_CACHE_OUTPUT_ITEMS} 범위여야 함\n"),
        "the parser must reject oversized output cardinality before touching the repository"
    );
}

#[test]
fn maven_audit_rejects_oversized_scan_authority_before_repository_access() {
    assert_oversized_max_entries_fails_before_repository_access(
        env!("CARGO_BIN_EXE_disksage-maven-cache-audit"),
        &[],
    );
}

#[test]
fn maven_prune_rejects_oversized_scan_authority_before_repository_access() {
    assert_oversized_max_entries_fails_before_repository_access(
        env!("CARGO_BIN_EXE_disksage-maven-cache-prune"),
        &[
            "--expected-candidate-set-fingerprint".to_string(),
            "0".repeat(64),
        ],
    );
}

#[test]
fn maven_audit_rejects_oversized_candidate_output_before_repository_access() {
    assert_oversized_audit_output_bound_fails_before_repository_access("--max-candidates");
}

#[test]
fn maven_audit_rejects_oversized_issue_output_before_repository_access() {
    assert_oversized_audit_output_bound_fails_before_repository_access("--max-issues");
}
