use std::process::Command;

const EXPECTED_USAGE: &str =
    "usage: disksage-photo-duplicate-audit PNG_PATH [PNG_PATH ...]";

fn assert_help_success(flag: &str) {
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-photo-duplicate-audit"))
        .env_remove("HOME")
        .arg(flag)
        .output()
        .expect("photo duplicate audit CLI must launch for its help contract");

    assert!(
        output.status.success(),
        "{flag} must be a successful terminal action, got status {:?} and stderr {:?}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "successful help must not be projected through stderr"
    );
    let stdout = String::from_utf8(output.stdout).expect("help output must be valid UTF-8");
    assert_eq!(
        stdout,
        format!("{EXPECTED_USAGE}\n"),
        "help output must equal the stable usage synopsis"
    );
}

#[test]
fn photo_duplicate_audit_help_is_terminal_and_read_only() {
    assert_help_success("--help");
    assert_help_success("-h");
}
