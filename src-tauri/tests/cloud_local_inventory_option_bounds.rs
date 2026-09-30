//! Black-box range-admission contract for the shipped cloud local-inventory CLI.
//!
//! Invalid resource bounds must fail before home/provider discovery. In particular, an empty
//! home must never turn an invalid `--all-roots` invocation into a successful empty JSON report.
//! The exact maximum MiB value is also exercised through a real synthetic provider root so the
//! accepted value must survive the MiB-to-byte conversion used by production inventory.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

fn binary_path() -> &'static Path {
    static BINARY_PATH: OnceLock<PathBuf> = OnceLock::new();
    BINARY_PATH
        .get_or_init(|| {
            let target_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("cloud-cli-contracts");
            std::fs::create_dir_all(&target_dir)
                .expect("shared Cargo contract target directory must be created");
            let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
            let status = Command::new(cargo)
                .current_dir(env!("CARGO_MANIFEST_DIR"))
                .args([
                    "build",
                    "--locked",
                    "--features",
                    "cloud-cli",
                    "--bin",
                    "disksage-cloud-local-inventory",
                    "--target-dir",
                ])
                .arg(&target_dir)
                .status()
                .expect("cloud local-inventory CLI must be buildable for range-admission tests");
            assert!(status.success(), "feature-gated CLI build must succeed");

            let binary = target_dir.join("debug").join(format!(
                "disksage-cloud-local-inventory{}",
                std::env::consts::EXE_SUFFIX
            ));
            assert!(binary.is_file(), "feature-gated CLI binary must exist");
            binary
        })
        .as_path()
}

fn assert_out_of_range(binary: &Path, flag: &str, value: &str, expected: &str) {
    let home = tempfile::tempdir().expect("isolated empty home must be created");
    let output = Command::new(binary)
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .args(["--all-roots", flag, value])
        .output()
        .expect("cloud local-inventory CLI must launch for range validation");

    assert_eq!(
        output.status.code(),
        Some(2),
        "invalid {flag}={value} must fail before empty-home discovery"
    );
    assert!(
        output.stdout.is_empty(),
        "invalid range input must not emit a successful batch report"
    );
    assert_eq!(
        output.stderr,
        format!("{expected}\n").as_bytes(),
        "range diagnostics must stay exact and bounded"
    );
}

#[test]
fn invalid_inventory_limits_fail_before_empty_home_discovery_can_return_success() {
    let binary = binary_path();

    for (flag, value, expected) in [
        (
            "--min-allocated-mib",
            "17592186044416",
            "cloud-local-inventory-min-allocated-mib-invalid",
        ),
        ("--max-entries", "0", "cloud-local-inventory-max-entries-invalid"),
        (
            "--max-entries",
            "1000001",
            "cloud-local-inventory-max-entries-invalid",
        ),
        ("--max-results", "0", "cloud-local-inventory-max-results-invalid"),
        (
            "--max-results",
            "10001",
            "cloud-local-inventory-max-results-invalid",
        ),
        ("--max-depth", "65", "cloud-local-inventory-max-depth-invalid"),
        (
            "--max-duration-ms",
            "0",
            "cloud-local-inventory-max-duration-invalid",
        ),
        (
            "--max-duration-ms",
            "300001",
            "cloud-local-inventory-max-duration-invalid",
        ),
        ("--max-issues", "0", "cloud-local-inventory-max-issues-invalid"),
        (
            "--max-issues",
            "1001",
            "cloud-local-inventory-max-issues-invalid",
        ),
    ] {
        assert_out_of_range(binary, flag, value, expected);
    }
}

#[test]
fn exact_inventory_limit_ceilings_remain_admitted_through_real_provider_inventory() {
    let binary = binary_path();
    let home = tempfile::tempdir().expect("isolated synthetic provider home must be created");
    std::fs::create_dir(home.path().join("OneDrive"))
        .expect("synthetic OneDrive root must be created");
    let output = Command::new(binary)
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .args([
            "--all-roots",
            "--min-allocated-mib",
            "17592186044415",
            "--max-entries",
            "1000000",
            "--max-results",
            "10000",
            "--max-depth",
            "64",
            "--max-duration-ms",
            "300000",
            "--max-issues",
            "1000",
        ])
        .output()
        .expect("cloud local-inventory CLI must launch at exact supported ceilings");

    assert!(
        output.status.success(),
        "exact supported ceilings must remain valid through a real worker inventory: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid ceiling invocation emits JSON");
    assert_eq!(report["version"], 1);
    assert_eq!(report["discovered_roots"], 1);
    assert_eq!(report["reported_roots"], 1);
    assert_eq!(report["failed_roots"], 0);
    assert_eq!(report["evidence_complete"], true);
}
