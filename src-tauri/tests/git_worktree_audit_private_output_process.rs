#![cfg(unix)]

//! Black-box publication contract for the shipped Git-worktree audit CLI's private report.
//!
//! The public summary must carry only a bounded commitment to the private evidence. The private
//! file itself must be created once at owner-only mode and a second invocation must fail closed
//! without replacing the first report.

use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

fn binary_path() -> &'static Path {
    static BINARY_PATH: OnceLock<PathBuf> = OnceLock::new();
    BINARY_PATH
        .get_or_init(|| {
            let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let target_dir = std::env::temp_dir().join(format!(
                "disksage-git-worktree-private-output-contract-{}",
                std::process::id()
            ));
            let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
            let output = Command::new(cargo)
                .current_dir(&manifest_dir)
                .args([
                    "build",
                    "--locked",
                    "--features",
                    "cloud-cli",
                    "--bin",
                    "disksage-git-worktree-audit",
                    "--target-dir",
                ])
                .arg(&target_dir)
                .output()
                .expect("Cargo should start for the shipped Git worktree audit binary");
            assert!(
                output.status.success(),
                "feature-gated Git worktree audit binary build failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            target_dir.join("debug").join(format!(
                "disksage-git-worktree-audit{}",
                std::env::consts::EXE_SUFFIX
            ))
        })
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
