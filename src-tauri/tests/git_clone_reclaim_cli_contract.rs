use std::{fs, path::Path, process::Command};

fn git(repository: &Path, arguments: &[&str]) {
    let output = Command::new("git")
        .current_dir(repository)
        .args(arguments)
        .output()
        .expect("Git should be available for the stale-clone CLI fixture");
    assert!(
        output.status.success(),
        "git {arguments:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn stale_clone_reclaim_cli_uses_the_shared_pull_request_flag_contract() {
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-git-clone-reclaim"))
        .arg("--help")
        .output()
        .expect("run shipped git clone reclaim CLI");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).expect("UTF-8 CLI help");
    assert!(stdout.contains("--include-closed-pull-requests"));
    assert!(stdout.contains("--stale-open-pull-request-cutoff-ms"));
    assert!(!stdout.contains("--stale-open-cutoff-ms"));
}

#[test]
fn stale_clone_plan_stays_within_the_public_local_command_timeout() {
    let fixture = tempfile::tempdir().expect("temporary fixture directory");
    let repository = fixture.path().join("repository");
    fs::create_dir(&repository).expect("repository directory");
    git(&repository, &["init", "-q", "-b", "main"]);
    git(
        &repository,
        &["config", "user.email", "disksage@example.invalid"],
    );
    git(&repository, &["config", "user.name", "DiskSage Test"]);
    fs::write(repository.join("tracked.txt"), b"tracked\n").expect("tracked fixture");
    git(&repository, &["add", "tracked.txt"]);
    git(&repository, &["commit", "-q", "-m", "fixture"]);

    let output = Command::new(env!("CARGO_BIN_EXE_disksage-git-clone-reclaim"))
        .arg("--repository-root")
        .arg(&repository)
        .args(["--reference-ref", "HEAD"])
        .output()
        .expect("run shipped git clone reclaim plan");
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 stderr");

    assert!(
        output.status.success(),
        "stale-clone plan must reach its read-only product boundary: {stderr}"
    );
    assert!(!stderr.contains("git-worktree-command-timeout-out-of-bounds"));
    let payload: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("plan output should be JSON");
    assert_eq!(
        payload["schema_kind"],
        serde_json::Value::String("disksage.git-clone-reclaim-plan".into())
    );
    assert_eq!(payload["filesystem_mutation_executed"], false);
}
