//! Black-box contracts for operational CLIs that already expose successful help.
//!
//! A help flag is terminal only when it is the sole request. Mixing help with an
//! invalid option must remain a bounded non-zero argument error rather than
//! silently accepting malformed automation input.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

fn build_operational_binaries() -> (tempfile::TempDir, Vec<(PathBuf, &'static str)>) {
    let target_dir = tempfile::tempdir().expect("isolated Cargo target directory must be created");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let status = Command::new(cargo)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "build",
            "--locked",
            "--features",
            "volume-cli",
            "--bin",
            "disksage-reclaim-plan",
            "--bin",
            "disksage-podman-reclaim-plan",
            "--bin",
            "disksage-volume-snapshot",
            "--target-dir",
        ])
        .arg(target_dir.path())
        .status()
        .expect("operational CLIs must be buildable for their process contracts");
    assert!(
        status.success(),
        "operational CLI build must succeed before process assertions"
    );

    let executable = |name: &str| {
        target_dir
            .path()
            .join("debug")
            .join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
    };
    let binaries = vec![
        (
            executable("disksage-reclaim-plan"),
            "Usage: disksage-reclaim-plan",
        ),
        (
            executable("disksage-podman-reclaim-plan"),
            "Usage: disksage-podman-reclaim-plan",
        ),
        (
            executable("disksage-volume-snapshot"),
            "Usage: disksage-volume-snapshot",
        ),
    ];
    for (binary, _) in &binaries {
        assert!(binary.is_file(), "expected built CLI at {}", binary.display());
    }
    (target_dir, binaries)
}

fn assert_help_success(binary: &Path, flag: &str, usage_marker: &str) {
    let output = Command::new(binary)
        .arg(flag)
        .output()
        .expect("DiskSage CLI must launch for its help contract");
    assert!(
        output.status.success(),
        "{flag} must succeed, got status {:?} and stderr {:?}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stderr.is_empty(),
        "successful help must not be projected through stderr"
    );
    let stdout = String::from_utf8(output.stdout).expect("help output must be valid UTF-8");
    assert!(
        stdout.contains(usage_marker),
        "help output must contain the stable usage synopsis"
    );
}

fn assert_unknown_argument_is_bounded(binary: &Path) {
    let output = Command::new(binary)
        .arg("--opaque-option=not-shown")
        .output()
        .expect("DiskSage CLI must launch for invalid argument validation");
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

fn assert_help_does_not_hide_invalid_argument(binary: &Path, help_flag: &str) {
    let output = Command::new(binary)
        .args([help_flag, "--opaque-option=not-shown"])
        .output()
        .expect("DiskSage CLI must launch for mixed help validation");
    assert!(
        !output.status.success(),
        "{help_flag} must not turn a mixed invalid invocation into success"
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
    let output = Command::new(binary)
        .arg(opaque)
        .output()
        .expect("DiskSage CLI must launch for non-UTF-8 argument validation");

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
        !stderr.contains("opaque"),
        "invalid diagnostics must not echo opaque argument payloads"
    );
    assert!(
        !stderr.contains("panicked") && !stderr.contains("thread 'main'"),
        "invalid host arguments must not escape through a Rust panic"
    );
}

#[test]
fn successful_help_is_strictly_terminal_and_invalid_input_stays_bounded() {
    let (_target_dir, binaries) = build_operational_binaries();
    for (binary, usage_marker) in &binaries {
        assert_help_success(binary, "--help", usage_marker);
        assert_help_success(binary, "-h", usage_marker);
        assert_unknown_argument_is_bounded(binary);
        assert_help_does_not_hide_invalid_argument(binary, "--help");
        assert_help_does_not_hide_invalid_argument(binary, "-h");
        #[cfg(unix)]
        assert_non_utf8_argument_is_bounded(binary);
    }
}
