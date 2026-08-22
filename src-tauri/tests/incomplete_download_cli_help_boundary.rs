//! Black-box help-contract regressions for incomplete-download operator CLIs.
//!
//! Help is a terminal, side-effect-free operator boundary: it must not require HOME,
//! cloud discovery, provider capacity, private evidence, or execution authority.

use std::process::Command;

fn assert_terminal_help(binary: &str, expected_usage: &str) {
    for help_flag in ["--help", "-h"] {
        let output = Command::new(binary)
            .arg(help_flag)
            .env_remove("HOME")
            .env_remove("USERPROFILE")
            .output()
            .expect("the shipped incomplete-download binary should start");

        assert_eq!(
            output.status.code(),
            Some(0),
            "help must be terminal success for {help_flag}: stderr={}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stderr.is_empty(),
            "help must not be reported as an execution failure for {help_flag}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).expect("help output should be UTF-8");
        assert!(
            stdout.contains(expected_usage),
            "help output should contain the shipped usage contract: {stdout}"
        );
    }
}

#[test]
fn destination_plan_help_is_terminal_without_home_or_provider_io() {
    assert_terminal_help(
        env!("CARGO_BIN_EXE_disksage-incomplete-download-destination-plan"),
        "usage: disksage-incomplete-download-destination-plan",
    );
}

#[test]
fn materialize_help_is_terminal_without_home_or_mutation_authority() {
    assert_terminal_help(
        env!("CARGO_BIN_EXE_disksage-incomplete-download-materialize"),
        "usage: disksage-incomplete-download-materialize",
    );
}
