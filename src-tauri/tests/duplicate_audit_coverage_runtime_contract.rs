//! Black-box regression for coverage instrumentation preserving the shipped duplicate-audit runtime.
//!
//! Coverage builds must execute the same CLI behavior as ordinary builds. A no-op `cfg(coverage)`
//! main would make exact coverage falsely green while never measuring argument parsing or runtime
//! authority. Build the real feature-gated binary with `--cfg coverage` and require its terminal
//! help behavior to remain identical.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;

const EXPECTED_USAGE: &str = "usage: disksage-duplicate-audit --root ABSOLUTE_PATH [--min-bytes POSITIVE_INTEGER] [--max-entries 1..=1000000] [--private-output ABSOLUTE_NEW_FILE.json]";

#[test]
fn coverage_instrumentation_preserves_shipped_help_runtime() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target_dir = std::env::temp_dir().join(format!(
        "disksage-duplicate-audit-coverage-runtime-{}",
        std::process::id()
    ));
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));

    let build = Command::new(cargo)
        .current_dir(&manifest_dir)
        .args([
            "rustc",
            "--locked",
            "--features",
            "cloud-cli",
            "--bin",
            "disksage-duplicate-audit",
            "--target-dir",
        ])
        .arg(&target_dir)
        .args(["--", "--cfg", "coverage"])
        .output()
        .expect("Cargo should start for the coverage-instrumented duplicate-audit binary");
    assert!(
        build.status.success(),
        "coverage-instrumented duplicate-audit build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    let binary = target_dir.join("debug").join(format!(
        "disksage-duplicate-audit{}",
        std::env::consts::EXE_SUFFIX
    ));
    let output = Command::new(binary)
        .env_remove("HOME")
        .env_remove("USERPROFILE")
        .env_remove("APPDATA")
        .env_remove("XDG_DATA_HOME")
        .arg("--help")
        .output()
        .expect("coverage-instrumented duplicate-audit binary should start");

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(output.stdout).expect("help stdout should remain UTF-8"),
        format!("{EXPECTED_USAGE}\n")
    );
    assert!(output.stderr.is_empty());
}
