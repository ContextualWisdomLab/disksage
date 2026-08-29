//! Black-box help and invalid-argument contracts for feature-gated eviction CLIs.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

const BINARIES: [(&str, &str, &str, &str); 4] = [
    (
        "disksage-icloud-local-eviction",
        "usage: disksage-icloud-local-eviction --cloud-root ABSOLUTE_PATH --path ABSOLUTE_FILE [--execute --approved-plan-fingerprint HEX64 --confirm-plan-fingerprint HEX64 --approved-by human:IDENTITY --rationale TEXT --record-dir ABSOLUTE_LOCAL_DIRECTORY]",
        "icloud-local-eviction-unknown-argument",
        "icloud-local-eviction-invalid-utf8-argument",
    ),
    (
        "disksage-incomplete-download-destination-plan",
        "usage: disksage-incomplete-download-destination-plan --source-root ABSOLUTE_PATH --cloud-root ABSOLUTE_PATH --destination-subdirectory RELATIVE_PATH (--live-icloud-capacity | --capacity-snapshot ABSOLUTE.json) [--max-entries 1..=200000] [--stale-after-days 1..=3650] [--capacity-reserve-mib 0..=1048576] [--private-output ABSOLUTE_NEW_FILE.json]",
        "incomplete-download-destination-plan-unknown-argument",
        "incomplete-download-destination-plan-invalid-utf8-argument",
    ),
    (
        "disksage-cloud-local-eviction-batch",
        "usage: disksage-cloud-local-eviction-batch --cloud-root ABSOLUTE_PATH --manifest ABSOLUTE_JSON [--execute --approved-batch-fingerprint HEX64 --confirm-batch-fingerprint HEX64 --approved-by human:IDENTITY --rationale TEXT --record-dir ABSOLUTE_LOCAL_DIRECTORY]",
        "알 수 없는 인자",
        "icloud-local-eviction-batch-invalid-utf8-argument",
    ),
    (
        "disksage-icloud-local-eviction-batch",
        "usage: disksage-icloud-local-eviction-batch --cloud-root ABSOLUTE_PATH --manifest ABSOLUTE_JSON [--execute --approved-batch-fingerprint HEX64 --confirm-batch-fingerprint HEX64 --approved-by human:IDENTITY --rationale TEXT --record-dir ABSOLUTE_LOCAL_DIRECTORY]",
        "알 수 없는 인자",
        "icloud-local-eviction-batch-invalid-utf8-argument",
    ),
];

fn build_feature_gated_binaries() -> (tempfile::TempDir, Vec<PathBuf>) {
    let target_dir = tempfile::tempdir().expect("isolated Cargo target directory must be created");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let mut command = Command::new(cargo);
    command
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["build", "--locked", "--features", "cloud-cli"]);
    for (binary, _, _, _) in BINARIES {
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
        .map(|(binary, _, _, _)| {
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

fn assert_invalid_argument_is_bounded(binary: &Path, expected_diagnostic: &str) {
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
        stderr.contains(expected_diagnostic),
        "invalid invocation must emit its fixed bounded diagnostic"
    );
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
fn assert_non_utf8_argument_is_bounded(binary: &Path, expected_diagnostic: &str) {
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
        stderr.contains(expected_diagnostic),
        "invalid non-UTF-8 input must emit its fixed bounded diagnostic"
    );
    assert!(
        !stderr.contains("panicked") && !stderr.contains("thread 'main'"),
        "invalid host arguments must not escape through a Rust panic"
    );
}

#[cfg(unix)]
fn assert_native_path_values_are_not_forced_through_utf8(binaries: &[PathBuf]) {
    use std::os::unix::ffi::OsStringExt;

    let parent = tempfile::tempdir().expect("native path parent must be created");
    let mut name = b"native-cloud-root-".to_vec();
    name.push(0xff);
    let native_path = parent.path().join(OsString::from_vec(name));
    let manifest = parent.path().join("manifest.json");
    let capacity = parent.path().join("capacity.json");

    let local = command(&binaries[0])
        .arg("--cloud-root")
        .arg(&native_path)
        .arg("--path")
        .arg(&native_path)
        .output()
        .expect("local-eviction CLI must launch with native path values");
    let local_stderr = String::from_utf8(local.stderr).expect("diagnostic must remain UTF-8");
    assert_eq!(local.status.code(), Some(2));
    assert!(local_stderr.contains("HOME/USERPROFILE을 찾을 수 없음"));
    assert!(!local_stderr.contains("invalid-utf8-argument"));

    let destination = command(&binaries[1])
        .arg("--source-root")
        .arg(&native_path)
        .arg("--cloud-root")
        .arg(&native_path)
        .args(["--destination-subdirectory", "Recovered", "--capacity-snapshot"])
        .arg(&capacity)
        .output()
        .expect("destination-plan CLI must launch with native path values");
    let destination_stderr =
        String::from_utf8(destination.stderr).expect("diagnostic must remain UTF-8");
    assert_eq!(destination.status.code(), Some(2));
    assert!(destination_stderr.contains("home-directory-unavailable"));
    assert!(!destination_stderr.contains("invalid-utf8-argument"));

    let batch = command(&binaries[2])
        .arg("--cloud-root")
        .arg(&native_path)
        .arg("--manifest")
        .arg(&manifest)
        .output()
        .expect("batch-eviction CLI must launch with native path values");
    let batch_stderr = String::from_utf8(batch.stderr).expect("diagnostic must remain UTF-8");
    assert_eq!(batch.status.code(), Some(2));
    assert!(batch_stderr.contains("HOME을 확인할 수 없음"));
    assert!(!batch_stderr.contains("invalid-utf8-argument"));
}

#[test]
fn eviction_and_destination_help_are_successful_and_invalid_arguments_are_bounded() {
    let (_target_dir, binaries) = build_feature_gated_binaries();
    for ((_, expected_usage, expected_unknown, expected_invalid_utf8), binary) in
        BINARIES.iter().zip(&binaries)
    {
        assert_help_success(binary, expected_usage, "--help");
        assert_help_success(binary, expected_usage, "-h");
        assert_invalid_argument_is_bounded(binary, expected_unknown);
        assert_help_does_not_hide_invalid_argument(binary);
        #[cfg(unix)]
        assert_non_utf8_argument_is_bounded(binary, expected_invalid_utf8);
    }
    #[cfg(unix)]
    assert_native_path_values_are_not_forced_through_utf8(&binaries);
}
