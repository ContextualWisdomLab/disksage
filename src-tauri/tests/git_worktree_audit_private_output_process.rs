#![cfg(unix)]

//! Black-box publication contract for the shipped Git-worktree audit CLI's private report.
//!
//! The public summary must carry only a bounded commitment to the private evidence. The private
//! file itself must be created once at owner-only mode and a second invocation must fail closed
//! without replacing the first report.

use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

fn binary_path() -> &'static Path {
    static BINARY_PATH: OnceLock<PathBuf> = OnceLock::new();
    BINARY_PATH
        .get_or_init(|| PathBuf::from(env!("CARGO_BIN_EXE_disksage-git-worktree-audit")))
        .as_path()
}

fn git(repository: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .current_dir(repository)
        .args(arguments)
        .output()
        .expect("Git should be available to the private-output process contract");
    assert!(
        output.status.success(),
        "git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn initialized_repository() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().expect("temporary directory should be created");
    let repository = temp.path().join("private-customer-repository");
    fs::create_dir(&repository).expect("repository directory should be created");
    git(&repository, &["init", "-q"]);
    git(&repository, &["config", "user.email", "coverage@example.invalid"]);
    git(&repository, &["config", "user.name", "DiskSage Test"]);
    fs::write(repository.join("tracked.txt"), b"tracked\n")
        .expect("tracked fixture should be written");
    git(&repository, &["add", "tracked.txt"]);
    git(&repository, &["commit", "-q", "-m", "fixture"]);
    (temp, repository)
}

fn run_private_output(repository: &Path, private_output: &Path) -> std::process::Output {
    Command::new(binary_path())
        .arg("--repository-root")
        .arg(repository)
        .args(["--reference-ref", "HEAD", "--private-output"])
        .arg(private_output)
        .output()
        .expect("Git worktree audit binary should start")
}

#[test]
fn private_report_is_owner_only_create_once_and_publicly_committed_by_digest() {
    let (temp, repository) = initialized_repository();
    let private_output = temp.path().join("private-report.json");

    let first = run_private_output(&repository, &private_output);
    assert_eq!(first.status.code(), Some(0));
    assert!(first.stderr.is_empty());

    let stdout = String::from_utf8(first.stdout).expect("public summary should remain UTF-8 JSON");
    let summary: serde_json::Value =
        serde_json::from_str(&stdout).expect("public summary should be valid JSON");
    let commitment = &summary["private_report"];
    assert_eq!(commitment["written"], true);
    assert_eq!(commitment["unix_mode"], "0600");
    assert_eq!(commitment["create_new"], true);
    assert_eq!(commitment["contains_sensitive_local_paths_and_branches"], true);
    assert_eq!(commitment["is_approval"], false);
    assert!(
        !stdout.contains(&repository.to_string_lossy().to_string()),
        "public summary must not expose the selected repository path"
    );
    assert!(
        !stdout.contains(&private_output.to_string_lossy().to_string()),
        "public summary must not expose the private-report path"
    );

    let private_bytes = fs::read(&private_output).expect("private report should be published");
    assert_eq!(
        commitment["bytes"].as_u64(),
        Some(private_bytes.len() as u64),
        "public byte commitment must bind the exact published object"
    );
    let expected_sha = Sha256::digest(&private_bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(commitment["sha256"], expected_sha);
    assert_eq!(
        fs::metadata(&private_output)
            .expect("private report metadata should remain available")
            .permissions()
            .mode()
            & 0o777,
        0o600,
        "private report must be owner-readable/writable only at publication"
    );
    let _: serde_json::Value =
        serde_json::from_slice(&private_bytes).expect("private report should be valid JSON evidence");

    let second = run_private_output(&repository, &private_output);
    assert_eq!(second.status.code(), Some(2));
    assert!(second.stdout.is_empty());
    assert_eq!(
        String::from_utf8(second.stderr).expect("failure stderr should remain UTF-8"),
        "DiskSage Git worktree audit: git-worktree-private-output-create-failed\n"
    );
    assert_eq!(
        fs::read(&private_output).expect("original private report must remain readable"),
        private_bytes,
        "create-once publication must never replace the first private report"
    );
}

#[test]
fn private_report_rejects_shared_writable_or_repository_owned_parent() {
    let (temp, repository) = initialized_repository();

    let shared_parent = temp.path().join("shared-private-parent");
    fs::create_dir(&shared_parent).expect("shared parent should be created");
    fs::set_permissions(&shared_parent, fs::Permissions::from_mode(0o770))
        .expect("shared parent mode should be configured");
    let shared_output = shared_parent.join("private-report.json");
    let shared = run_private_output(&repository, &shared_output);
    assert_eq!(shared.status.code(), Some(2));
    assert!(shared.stdout.is_empty());
    assert_eq!(
        String::from_utf8(shared.stderr).expect("failure stderr should remain UTF-8"),
        "DiskSage Git worktree audit: git-worktree-private-output-parent-writable-by-others\n"
    );
    assert!(
        !shared_output.exists(),
        "a shared-writable parent must never receive private evidence"
    );

    let inside_repository = repository.join("private-report.json");
    let inside = run_private_output(&repository, &inside_repository);
    assert_eq!(inside.status.code(), Some(2));
    assert!(inside.stdout.is_empty());
    assert_eq!(
        String::from_utf8(inside.stderr).expect("failure stderr should remain UTF-8"),
        "DiskSage Git worktree audit: git-worktree-private-output-inside-repository\n"
    );
    assert!(
        !inside_repository.exists(),
        "private evidence must stay outside the audited repository"
    );
}
