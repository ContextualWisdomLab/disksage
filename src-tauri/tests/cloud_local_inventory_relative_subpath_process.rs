//! Black-box runtime coverage for cloud-local-inventory relative-subpath selection.
//!
//! These tests launch the shipped feature-gated CLI against an isolated synthetic OneDrive root.
//! A real descendant directory must remain a read-only inventory scope, while unavailable,
//! non-directory, or symlink descendants must fail closed before traversal can escape or invent
//! the selected provider-root scope.

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
                .expect("cloud local-inventory CLI must be buildable for relative-subpath tests");
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

fn bounded_inventory_args(cloud_root: &Path, relative: &str) -> Vec<OsString> {
    vec![
        OsString::from("--cloud-root"),
        cloud_root.as_os_str().to_os_string(),
        OsString::from("--relative-subpath"),
        OsString::from(relative),
        OsString::from("--min-allocated-mib"),
        OsString::from("0"),
        OsString::from("--max-entries"),
        OsString::from("16"),
        OsString::from("--max-results"),
        OsString::from("16"),
        OsString::from("--max-depth"),
        OsString::from("2"),
        OsString::from("--max-duration-ms"),
        OsString::from("2000"),
        OsString::from("--max-issues"),
        OsString::from("16"),
    ]
}

#[test]
fn real_descendant_directory_is_inventory_scope_without_provider_mutation() {
    let binary = binary_path();
    let home = tempfile::tempdir().expect("isolated synthetic provider home must be created");
    let onedrive = home.path().join("OneDrive");
    let archive = onedrive.join("Archive");
    std::fs::create_dir_all(&archive).expect("synthetic OneDrive descendant must be created");

    let output = Command::new(binary)
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .args(bounded_inventory_args(&onedrive, "Archive"))
        .output()
        .expect("cloud local-inventory CLI must launch for descendant inventory");

    assert!(
        output.status.success(),
        "real descendant inventory must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("descendant inventory must emit JSON");
    assert_eq!(report["version"], 2);
    assert_eq!(report["provider"], "onedrive");
    assert_eq!(report["account_scope"], "unknown");
    assert_eq!(report["cloud_root"], archive.to_string_lossy().as_ref());
    assert_eq!(
        report["cloud_root_id"],
        format!("{}#Archive", onedrive.to_string_lossy())
    );
    assert_eq!(report["evidence_complete"], true);
    assert_eq!(report["results_truncated"], false);
    assert_eq!(report["issues_truncated"], false);
    assert_eq!(report["candidates"].as_array().map(Vec::len), Some(0));
}

#[test]
fn unavailable_or_regular_file_descendant_fails_closed_before_inventory() {
    let binary = binary_path();
    let home = tempfile::tempdir().expect("isolated synthetic provider home must be created");
    let onedrive = home.path().join("OneDrive");
    std::fs::create_dir(&onedrive).expect("synthetic OneDrive root must be created");

    let unavailable = Command::new(binary)
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .args(bounded_inventory_args(&onedrive, "Missing"))
        .output()
        .expect("cloud local-inventory CLI must launch for unavailable descendant rejection");
    assert_eq!(unavailable.status.code(), Some(2));
    assert!(unavailable.stdout.is_empty());
    assert_eq!(
        unavailable.stderr,
        b"cloud-local-inventory-subpath-unavailable\n"
    );

    let marker = onedrive.join("Archive");
    std::fs::write(&marker, b"must-remain-a-file\n")
        .expect("regular-file descendant fixture must be written");
    let regular_file = Command::new(binary)
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .args(bounded_inventory_args(&onedrive, "Archive"))
        .output()
        .expect("cloud local-inventory CLI must launch for regular-file descendant rejection");
    assert_eq!(regular_file.status.code(), Some(2));
    assert!(regular_file.stdout.is_empty());
    assert_eq!(
        regular_file.stderr,
        b"cloud-local-inventory-subpath-not-real-directory\n"
    );
    assert_eq!(
        std::fs::read(&marker).expect("regular-file descendant must remain unchanged"),
        b"must-remain-a-file\n"
    );
}

#[cfg(unix)]
#[test]
fn symlink_descendant_fails_closed_before_inventory_escape() {
    use std::os::unix::fs::symlink;

    let binary = binary_path();
    let home = tempfile::tempdir().expect("isolated synthetic provider home must be created");
    let onedrive = home.path().join("OneDrive");
    let outside = tempfile::tempdir().expect("outside fixture must be created");
    std::fs::create_dir(&onedrive).expect("synthetic OneDrive root must be created");
    std::fs::write(outside.path().join("outside.txt"), b"must-not-be-inventoried\n")
        .expect("outside marker must be written");
    symlink(outside.path(), onedrive.join("Archive"))
        .expect("symlink descendant fixture must be created");

    let output = Command::new(binary)
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .args(bounded_inventory_args(&onedrive, "Archive"))
        .output()
        .expect("cloud local-inventory CLI must launch for symlink rejection");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        output.stderr,
        b"cloud-local-inventory-subpath-not-real-directory\n"
    );
    assert_eq!(
        std::fs::read(outside.path().join("outside.txt")).expect("outside marker must remain"),
        b"must-not-be-inventoried\n"
    );
}
