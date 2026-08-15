//! Black-box help and invalid-argument contracts for feature-gated eviction CLIs.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

const BINARIES: [(&str, &str); 3] = [
    (
        "disksage-icloud-local-eviction",
        "usage: disksage-icloud-local-eviction --cloud-root ABSOLUTE_PATH --path ABSOLUTE_FILE [--execute --approved-plan-fingerprint HEX64 --confirm-plan-fingerprint HEX64 --approved-by human:IDENTITY --rationale TEXT --record-dir ABSOLUTE_LOCAL_DIRECTORY]",
    ),
    (
        "disksage-incomplete-download-destination-plan",
        "usage: disksage-incomplete-download-destination-plan --source-root ABSOLUTE_PATH --cloud-root ABSOLUTE_PATH --destination-subdirectory RELATIVE_PATH (--live-icloud-capacity | --capacity-snapshot ABSOLUTE.json) [--max-entries 1..=200000] [--stale-after-days 1..=3650] [--capacity-reserve-mib 0..=1048576] [--private-output ABSOLUTE_NEW_FILE.json]",
    ),
    (
        "disksage-icloud-local-eviction-batch",
        "usage: disksage-icloud-local-eviction-batch --cloud-root ABSOLUTE_PATH --manifest ABSOLUTE_JSON [--execute --approved-batch-fingerprint HEX64 --confirm-batch-fingerprint HEX64 --approved-by human:IDENTITY --rationale TEXT --record-dir ABSOLUTE_LOCAL_DIRECTORY]",
    ),
];

fn build_feature_gated_binaries() -> (tempfile::TempDir, Vec<PathBuf>) {
    let target_dir = tempfile::tempdir().expect("isolated Cargo target directory must be created");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let mut command = Command::new(cargo);
    command
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["build", "--locked", "--features", "cloud-cli"]);
    for (binary, _) in BINARIES {
        command.args(["--bin", binary]);
    }
    let status = command
        .arg("--target-dir")
        .arg(target_dir.path())
        .status()
        .expect("feature-gated operational CLIs must be buildable for process contracts");
    assert!(
        status.success(),
        "feature-gated operational CLI build must succeed before process assertions"
    );

    let binaries = BINARIES
        .iter()
        .map(|(binary, _)| {
            let path = target_dir
                .path()
                .join("debug")
                .join(format!("{binary}{}", std::env::consts::EXE_SUFFIX));
            assert!(
                path.is_file(),
                "{binary} must exist after the explicit cloud-cli build"
            );
            path
        })
        .collect();
    (target_dir, binaries)
}

fn command(binary: &Path) -> Command {
    let mut command = Command::new(binary);
    command.env_remove("HOME").env_remove("USERPROFILE");
    command
}

fn assert_help_success(binary: &Path, expected_usage: &str, flag: &str) {
    let output = command(binary)
        .arg(flag)
        .output()
        .expect("operational CLI must launch for its help contract");

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
        format!("{expected_usage}\n"),
        "help output must equal the complete stable usage contract"
    );
}

fn assert_invalid_argument_is_bounded(binary: &Path) {
    let output = command(binary)
        .arg("--opaque-option=not-shown")
        .output()
        .expect("operational CLI must launch for invalid argument validation");

    assert!(
        !output.status.success(),
        "an unknown argument must remain a non-zero failure"
    );
    assert!(
        output.stdout.is_empty(),
        "invalid invocation must not emit successful output on stdout"
    );
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must be valid UTF-8");
    assert!(!stderr.is_empty(), "invalid invocation must remain visible");
    assert!(
        !stderr.contains("not-shown"),
        "invalid diagnostics must not echo arbitrary argument payloads"
    );
}

fn assert_help_does_not_hide_invalid_argument(binary: &Path) {
    let output = command(binary)
        .args(["--help", "--opaque-option=not-shown"])
        .output()
        .expect("operational CLI must launch for mixed help validation");

    assert!(
        !output.status.success(),
        "help must not turn an otherwise invalid invocation into success"
    );
    assert!(
        output.stdout.is_empty(),
        "mixed invalid invocation must not emit successful help on stdout"
    );
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must be valid UTF-8");
    assert!(!stderr.is_empty(), "mixed invalid invocation must remain visible");
    assert!(
        !stderr.contains("not-shown"),
        "mixed invalid diagnostics must not echo arbitrary argument payloads"
    );
}

#[cfg(unix)]
fn assert_non_utf8_argument_is_bounded(binary: &Path) {
    use std::os::unix::ffi::OsStringExt;

    let opaque = OsString::from_vec(vec![b'-', b'-', b'o', b'p', b'a', b'q', b'u', b'e', 0xff]);
    let output = command(binary)
        .arg(opaque)
        .output()
        .expect("operational CLI must launch for non-UTF-8 argument validation");

    assert_eq!(
        output.status.code(),
        Some(2),
        "invalid non-UTF-8 input must use the ordinary bounded argument-error exit"
    );
    assert!(
        output.stdout.is_empty(),
        "invalid non-UTF-8 input must not emit successful output"
    );
    let stderr = String::from_utf8(output.stderr).expect("CLI diagnostics must remain valid UTF-8");
    assert!(!stderr.is_empty(), "invalid non-UTF-8 input must remain visible");
    assert!(
        !stderr.contains("panicked") && !stderr.contains("thread 'main'"),
        "invalid host arguments must not escape through a Rust panic"
    );
}

#[test]
fn eviction_and_destination_help_are_successful_and_invalid_arguments_are_bounded() {
    let (_target_dir, binaries) = build_feature_gated_binaries();
    for ((_, expected_usage), binary) in BINARIES.iter().zip(&binaries) {
        assert_help_success(binary, expected_usage, "--help");
        assert_help_success(binary, expected_usage, "-h");
        assert_invalid_argument_is_bounded(binary);
        assert_help_does_not_hide_invalid_argument(binary);
        #[cfg(unix)]
        assert_non_utf8_argument_is_bounded(binary);
    }
}
