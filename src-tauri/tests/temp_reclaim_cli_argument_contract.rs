use std::process::Command;

fn invoke(extra: &[&str]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_disksage-temp-reclaim"));
    command.args([
        "--execute-fingerprint",
        "not-a-real-candidate",
        "--approved-by",
        "local-user",
        "--approval-phrase",
        "not-a-real-approval",
        "--journal-path",
        "/tmp/disksage-temp-reclaim-argument-contract.jsonl",
    ]);
    command.args(extra);
    command
        .output()
        .expect("temp reclaim CLI should start for argument validation")
}

#[test]
fn unknown_execution_flag_is_rejected_before_planning() {
    let output = invoke(&["--unexpected-option", "value"]);

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(
        stderr.contains("지원하지 않는 인자"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn duplicate_execution_flag_is_rejected_before_planning() {
    let output = invoke(&["--approved-by", "different-user"]);

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("stderr should be UTF-8");
    assert!(stderr.contains("중복된 인자"), "unexpected stderr: {stderr}");
}
