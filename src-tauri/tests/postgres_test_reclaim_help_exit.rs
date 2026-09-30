use std::process::Command;

const EXPECTED_USAGE: &str = "Usage: disksage-postgres-test-reclaim --data-directory ABSOLUTE_PATH --psql-path ABSOLUTE_PATH --pg-ctl-path ABSOLUTE_PATH --database-user USER --expected-database NAME [--expected-database NAME...] --record-directory PRIVATE_ABSOLUTE_PATH [--execute --approved-plan-fingerprint HEX64 --exact-approval-phrase TEXT]\nDefault mode only writes a private plan. Execution requires the exact fingerprint and approval phrase printed by that plan.";

/// Prove both help flags are successful terminal actions that perform no domain work.
#[test]
fn postgres_test_reclaim_help_exits_successfully_without_error_output() {
    for flag in ["--help", "-h"] {
        let output = Command::new(env!("CARGO_BIN_EXE_disksage-postgres-test-reclaim"))
            .arg(flag)
            .output()
            .expect("PostgreSQL reclaim CLI must launch for its help contract");

        assert!(
            output.status.success(),
            "{flag} must exit successfully, got status {:?} and stderr {:?}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stderr.is_empty(),
            "successful help must not be projected through stderr"
        );
        let stdout = String::from_utf8(output.stdout).expect("help output must be valid UTF-8");
        assert_eq!(stdout, format!("{EXPECTED_USAGE}\n"));
    }
}

/// Prove help cannot hide an otherwise invalid invocation in either argument order.
#[test]
fn postgres_test_reclaim_help_does_not_mask_other_arguments() {
    for arguments in [["--help", "--execute"], ["--execute", "--help"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_disksage-postgres-test-reclaim"))
            .args(arguments)
            .output()
            .expect("PostgreSQL reclaim CLI must launch for mixed-help validation");

        assert!(
            !output.status.success(),
            "mixed help must remain a bounded invalid invocation"
        );
        assert!(
            output.stdout.is_empty(),
            "invalid invocation must not emit successful help on stdout"
        );
        let stderr =
            String::from_utf8(output.stderr).expect("CLI diagnostics must remain valid UTF-8");
        assert!(
            stderr.starts_with("disksage-postgres-test-reclaim: unknown argument\nUsage:"),
            "mixed help must use the stable bounded invalid-argument path, got {stderr:?}"
        );
    }
}
