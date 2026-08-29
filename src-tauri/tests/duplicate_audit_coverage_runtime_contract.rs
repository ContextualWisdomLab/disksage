#![cfg(feature = "cloud-cli")]

//! Black-box regression for the shipped duplicate-audit runtime plus its coverage entrypoint.
//!
//! Repository-wide coverage is intentionally collected without defining `cfg(coverage)` because
//! that synthetic cfg historically changed production semantics. This owner must therefore prove
//! two things without manufacturing a non-production build mode: the shipped feature-gated binary
//! executes real help/audit behavior, and its entrypoint has no coverage-only no-op replacement.

use std::process::Command;

const EXPECTED_USAGE: &str = "usage: disksage-duplicate-audit --root ABSOLUTE_PATH [--min-bytes POSITIVE_INTEGER] [--max-entries 1..=1000000] [--private-output ABSOLUTE_NEW_FILE.json]";
const DUPLICATE_AUDIT_SOURCE: &str = include_str!("../src/bin/disksage-duplicate-audit.rs");

#[test]
fn duplicate_audit_coverage_contract_keeps_the_shipped_entrypoint_real() {
    assert!(
        !DUPLICATE_AUDIT_SOURCE.contains("#[cfg(coverage)]\nfn main()"),
        "coverage must never replace the shipped duplicate-audit entrypoint with a synthetic main"
    );
    assert!(
        !DUPLICATE_AUDIT_SOURCE.contains("#[cfg(not(coverage))]\nfn main()"),
        "the shipped duplicate-audit entrypoint must not disappear when instrumentation is enabled"
    );
}

#[test]
fn duplicate_audit_shipped_runtime_preserves_help_and_read_only_audit() {
    let binary = env!("CARGO_BIN_EXE_disksage-duplicate-audit");
    let audit_root = tempfile::tempdir().expect("empty audit root should be creatable");

    let help = Command::new(binary)
        .env_remove("HOME")
        .env_remove("USERPROFILE")
        .env_remove("APPDATA")
        .env_remove("XDG_DATA_HOME")
        .arg("--help")
        .output()
        .expect("shipped duplicate-audit binary should start for help");

    assert_eq!(help.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(help.stdout).expect("help stdout should remain UTF-8"),
        format!("{EXPECTED_USAGE}\n")
    );
    assert!(help.stderr.is_empty());

    let audit = Command::new(binary)
        .arg("--root")
        .arg(audit_root.path())
        .args(["--min-bytes", "1", "--max-entries", "10"])
        .output()
        .expect("shipped duplicate-audit binary should start for a real audit");
    assert!(
        audit.status.success(),
        "the shipped audit runtime must execute real read-only behavior: status {:?}, stderr {}",
        audit.status.code(),
        String::from_utf8_lossy(&audit.stderr)
    );
    assert!(
        audit.stderr.is_empty(),
        "a successful read-only audit must keep stderr empty"
    );

    let summary: serde_json::Value = serde_json::from_slice(&audit.stdout)
        .expect("shipped audit stdout should remain machine-readable JSON");
    assert_eq!(summary["schema_version"], 1);
    assert_eq!(summary["file_count"], 0);
    assert_eq!(summary["cluster_count"], 0);
    assert_eq!(summary["automatic_delete_allowed"], false);
    assert_eq!(summary["mutation_performed"], false);
    assert_eq!(summary["local_paths_included"], false);
    assert_eq!(summary["content_digests_included"], false);
}
