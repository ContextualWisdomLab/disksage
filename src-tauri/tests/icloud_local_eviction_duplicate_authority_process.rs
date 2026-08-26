//! Duplicate singleton authority options must fail closed before provider or filesystem work.
//!
//! This exercises the shipped feature-gated local-eviction binary rather than only its parser.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

fn build_binary() -> (tempfile::TempDir, PathBuf) {
    let target_dir = tempfile::tempdir().expect("isolated Cargo target directory must be created");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let status = Command::new(cargo)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "build",
            "--locked",
            "--features",
            "cloud-cli",
            "--bin",
            "disksage-icloud-local-eviction",
            "--target-dir",
        ])
        .arg(target_dir.path())
        .status()
        .expect("local-eviction CLI must be buildable for the process contract");
    assert!(status.success(), "local-eviction CLI build must succeed");
    let binary = target_dir
        .path()
        .join("debug")
        .join(format!(
            "disksage-icloud-local-eviction{}",
            std::env::consts::EXE_SUFFIX
        ));
    assert!(binary.is_file(), "local-eviction CLI must exist after build");
    (target_dir, binary)
}

fn assert_duplicate_rejected(binary: &Path, args: &[&OsStr], expected: &str) {
    let output = Command::new(binary)
        .args(args)
        .env_remove("HOME")
        .env_remove("USERPROFILE")
        .output()
        .expect("local-eviction CLI must launch for duplicate-option validation");
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty(), "invalid authority input must not emit success JSON");
    let stderr = String::from_utf8(output.stderr).expect("diagnostic must remain valid UTF-8");
    assert_eq!(stderr.trim_end(), expected);
}

#[test]
fn duplicate_eviction_authority_options_fail_closed_before_domain_work() {
    let (_target_dir, binary) = build_binary();
    let fixture = tempfile::tempdir().expect("fixture root must be created");
    let cloud_root = fixture.path().join("Cloud");
    let alternate_root = fixture.path().join("OtherCloud");
    let file = cloud_root.join("file.bin");
    let alternate_file = cloud_root.join("other.bin");

    assert_duplicate_rejected(
        &binary,
        &[
            OsStr::new("--cloud-root"),
            cloud_root.as_os_str(),
            OsStr::new("--cloud-root"),
            alternate_root.as_os_str(),
            OsStr::new("--path"),
            file.as_os_str(),
        ],
        "--cloud-root는 한 번만 지정할 수 있음",
    );
    assert_duplicate_rejected(
        &binary,
        &[
            OsStr::new("--cloud-root"),
            cloud_root.as_os_str(),
            OsStr::new("--path"),
            file.as_os_str(),
            OsStr::new("--path"),
            alternate_file.as_os_str(),
        ],
        "--path는 한 번만 지정할 수 있음",
    );
    assert_duplicate_rejected(
        &binary,
        &[
            OsStr::new("--cloud-root"),
            cloud_root.as_os_str(),
            OsStr::new("--path"),
            file.as_os_str(),
            OsStr::new("--execute"),
            OsStr::new("--execute"),
        ],
        "--execute는 한 번만 지정할 수 있음",
    );
}
