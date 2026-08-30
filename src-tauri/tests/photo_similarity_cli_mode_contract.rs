#![cfg(not(windows))]

use std::process::Command;

const MODE_ERROR: &str = "photo-audit-audit-options-require-audit";

fn photo_cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_disksage-photo-similarity-audit"))
}

#[test]
fn execute_rejects_private_output_before_domain_work() {
    let output = photo_cli()
        .args([
            "--execute",
            "--root",
            "/tmp/disksage-photo-mode-contract",
            "--private-output",
            "/tmp/disksage-photo-mode-contract.json",
        ])
        .output()
        .expect("photo similarity CLI should start");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(MODE_ERROR),
        "execution must reject audit-only --private-output before reading reports: {stderr}"
    );
}

#[test]
fn execute_rejects_max_entries_before_domain_work() {
    let output = photo_cli()
        .args([
            "--execute",
            "--root",
            "/tmp/disksage-photo-mode-contract",
            "--max-entries",
            "17",
        ])
        .output()
        .expect("photo similarity CLI should start");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(MODE_ERROR),
        "execution must reject audit-only --max-entries before reading reports: {stderr}"
    );
}
