use std::process::Command;

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
