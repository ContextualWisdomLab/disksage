//! Black-box regression for coverage instrumentation preserving the shipped duplicate-audit runtime.
//!
//! Coverage builds must execute the same CLI behavior as ordinary builds. A coverage-only shortcut
//! can make exact coverage falsely green while never measuring argument parsing or runtime authority.
//! Build the real feature-gated binary with `--cfg coverage`, prove terminal help is unchanged, and
//! then drive a real read-only audit of an empty filesystem root through the same instrumented binary.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const EXPECTED_USAGE: &str = "usage: disksage-duplicate-audit --root ABSOLUTE_PATH [--min-bytes POSITIVE_INTEGER] [--max-entries 1..=1000000] [--private-output ABSOLUTE_NEW_FILE.json]";

#[test]
fn coverage_instrumentation_preserves_shipped_help_and_audit_runtime() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after the Unix epoch")
        .as_nanos();
    let target_dir = std::env::temp_dir().join(format!(
        "disksage-duplicate-audit-coverage-runtime-target-{}-{nonce}",
        std::process::id()
    ));
    let audit_root = std::env::temp_dir().join(format!(
        "disksage-duplicate-audit-coverage-runtime-root-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir(&audit_root).expect("empty audit root should be creatable");

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
    let help = Command::new(&binary)
        .env_remove("HOME")
        .env_remove("USERPROFILE")
        .env_remove("APPDATA")
        .env_remove("XDG_DATA_HOME")
        .arg("--help")
        .output()
        .expect("coverage-instrumented duplicate-audit binary should start for help");

    assert_eq!(help.status.code(), Some(0));
    assert_eq!(
        String::from_utf8(help.stdout).expect("help stdout should remain UTF-8"),
        format!("{EXPECTED_USAGE}\n")
    );
    assert!(help.stderr.is_empty());

    let audit = Command::new(&binary)
        .args(["--root"])
        .arg(&audit_root)
        .args(["--min-bytes", "1", "--max-entries", "10"])
        .output()
        .expect("coverage-instrumented duplicate-audit binary should start for a real audit");
    assert!(
        audit.status.success(),
        "coverage instrumentation must not replace the shipped audit runtime: status {:?}, stderr {}",
        audit.status.code(),
        String::from_utf8_lossy(&audit.stderr)
    );
    assert!(
        audit.stderr.is_empty(),
        "a successful read-only audit must keep stderr empty"
    );

    let summary: serde_json::Value = serde_json::from_slice(&audit.stdout)
        .expect("coverage-instrumented audit stdout should remain machine-readable JSON");
    assert_eq!(summary["schema_version"], 1);
    assert_eq!(summary["file_count"], 0);
    assert_eq!(summary["cluster_count"], 0);
    assert_eq!(summary["automatic_delete_allowed"], false);
    assert_eq!(summary["mutation_performed"], false);
    assert_eq!(summary["local_paths_included"], false);
    assert_eq!(summary["content_digests_included"], false);

    std::fs::remove_dir(&audit_root).expect("empty audit root should remain removable");
    std::fs::remove_dir_all(&target_dir).expect("isolated coverage target should be removable");
}
