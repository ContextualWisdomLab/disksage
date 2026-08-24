//! Coverage instrumentation must preserve shipped runtime behavior for eviction/destination CLIs.
//!
//! These binaries are part of owned production coverage. Building with `--cfg coverage` must not
//! replace their real parser/runtime with an empty entry point, otherwise exact coverage can look
//! better while never measuring the shipped help and argument boundary.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

const BINARIES: [(&str, &str); 2] = [
    (
        "disksage-icloud-local-eviction",
        "usage: disksage-icloud-local-eviction --cloud-root ABSOLUTE_PATH --path ABSOLUTE_FILE [--execute --approved-plan-fingerprint HEX64 --confirm-plan-fingerprint HEX64 --approved-by human:IDENTITY --rationale TEXT --record-dir ABSOLUTE_LOCAL_DIRECTORY]",
    ),
    (
        "disksage-incomplete-download-destination-plan",
        "usage: disksage-incomplete-download-destination-plan --source-root ABSOLUTE_PATH --cloud-root ABSOLUTE_PATH --destination-subdirectory RELATIVE_PATH (--live-icloud-capacity | --capacity-snapshot ABSOLUTE.json) [--max-entries 1..=200000] [--stale-after-days 1..=3650] [--capacity-reserve-mib 0..=1048576] [--private-output ABSOLUTE_NEW_FILE.json]",
    ),
];

fn build_coverage_binary(binary: &str, target_dir: &Path) -> PathBuf {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let build = Command::new(cargo)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "rustc",
            "--locked",
            "--features",
            "cloud-cli",
            "--bin",
            binary,
            "--target-dir",
        ])
        .arg(target_dir)
        .args(["--", "--cfg", "coverage"])
        .output()
        .expect("Cargo should start for the coverage-instrumented operational CLI");
    assert!(
        build.status.success(),
        "coverage-instrumented {binary} build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    target_dir
        .join("debug")
        .join(format!("{binary}{}", std::env::consts::EXE_SUFFIX))
}

#[test]
fn coverage_instrumentation_preserves_terminal_help_runtime() {
    let target = tempfile::tempdir().expect("isolated coverage target must be created");

    for (binary, expected_usage) in BINARIES {
        let executable = build_coverage_binary(binary, target.path());
        let output = Command::new(&executable)
            .env_remove("HOME")
            .env_remove("USERPROFILE")
            .env_remove("APPDATA")
            .env_remove("XDG_DATA_HOME")
            .arg("--help")
            .output()
            .expect("coverage-instrumented operational CLI must launch");

        assert_eq!(
            output.status.code(),
            Some(0),
            "coverage instrumentation must preserve successful help for {binary}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.stderr.is_empty(),
            "coverage-instrumented successful help must keep stderr empty for {binary}"
        );
        assert_eq!(
            String::from_utf8(output.stdout).expect("help output must stay valid UTF-8"),
            format!("{expected_usage}\n"),
            "coverage instrumentation must execute the shipped help runtime for {binary}"
        );
    }
}
