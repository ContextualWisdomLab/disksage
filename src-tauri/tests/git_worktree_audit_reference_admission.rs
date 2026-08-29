//! Process-level admission contract for Git worktree retention references.
//!
//! Invalid host references must be rejected before any Git process is started. The subprocess is
//! given a real absolute directory but an empty PATH, so a parser that defers validation into the
//! audit library leaks into `git-command-spawn-failed` instead of the bounded reference error.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

fn binary_path() -> &'static Path {
    static BINARY_PATH: OnceLock<PathBuf> = OnceLock::new();
    BINARY_PATH
        .get_or_init(|| PathBuf::from(env!("CARGO_BIN_EXE_disksage-git-worktree-audit")))
        .as_path()
}

#[test]
fn malformed_retention_references_fail_before_git_domain_work() {
    let repository = tempfile::tempdir().expect("absolute temporary repository path");
    let oversized = "a".repeat(1025);
    let invalid = ["", "-dangerous-option", "control\nreference", oversized.as_str()];

    for reference in invalid {
        let output = Command::new(binary_path())
            .env("PATH", "")
            .arg("--repository-root")
            .arg(repository.path())
            .arg("--reference-ref")
            .arg(reference)
            .output()
            .expect("Git worktree audit binary should start");

        assert_eq!(output.status.code(), Some(2), "reference: {reference:?}");
        assert!(output.stdout.is_empty(), "reference: {reference:?}");
        assert_eq!(
            String::from_utf8(output.stderr).expect("stderr should remain UTF-8"),
            "DiskSage Git worktree audit: git-worktree-reference-invalid\n",
            "reference: {reference:?}"
        );
    }
}
