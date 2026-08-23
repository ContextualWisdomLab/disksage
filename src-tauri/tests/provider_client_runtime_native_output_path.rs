//! Native filesystem paths must remain native through the provider-runtime CLI boundary.
//!
//! The output destination is an OS path, not a UTF-8 protocol field. On Unix, a valid absolute
//! path containing non-UTF-8 bytes must therefore remain usable without reflecting those bytes in
//! diagnostics or changing the audit's path-free public evidence contract.

#![cfg(unix)]

use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

fn binary_path() -> &'static Path {
    static BINARY_PATH: OnceLock<PathBuf> = OnceLock::new();
    BINARY_PATH
        .get_or_init(|| {
            let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let target_dir = std::env::temp_dir().join(format!(
                "disksage-provider-runtime-native-output-{}",
                std::process::id()
            ));
            let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
            let output = Command::new(cargo)
                .current_dir(&manifest_dir)
                .args([
                    "build",
                    "--locked",
                    "--features",
                    "cloud-cli",
                    "--bin",
                    "disksage-provider-client-runtime",
                    "--target-dir",
                ])
                .arg(&target_dir)
                .output()
                .expect("provider client-runtime binary build must start");
            assert!(
                output.status.success(),
                "provider client-runtime binary build failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            target_dir.join("debug").join("disksage-provider-client-runtime")
        })
        .as_path()
}

#[test]
fn non_utf8_absolute_output_path_remains_a_valid_native_destination() {
    let directory = tempfile::tempdir().expect("native output parent must be created");
    let mut file_name = b"provider-runtime-".to_vec();
    file_name.push(0xff);
    file_name.extend_from_slice(b".json");
    let output_path = directory.path().join(OsStr::from_bytes(&file_name));
    assert!(output_path.is_absolute());

    let output = Command::new(binary_path())
        .env_remove("HOME")
        .arg("--output")
        .arg(&output_path)
        .output()
        .expect("provider client-runtime CLI must launch with a native output path");

    assert_eq!(
        output.status.code(),
        Some(0),
        "native absolute paths must not be rejected as UTF-8 protocol input: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("successful provider-runtime stdout must remain JSON");
    assert_eq!(stdout["schema_kind"], "disksage.provider-client-runtime-audit");
    assert_eq!(stdout["local_paths_included"], false);
    assert_eq!(stdout["cloud_write_executed"], false);

    let bytes = std::fs::read(&output_path).expect("native output path must receive the audit");
    let persisted: serde_json::Value =
        serde_json::from_slice(&bytes).expect("persisted provider-runtime audit must remain JSON");
    assert_eq!(persisted["schema_version"], 1);
    assert_eq!(persisted["local_paths_included"], false);

    assert!(
        !output.stdout.windows(file_name.len()).any(|window| window == file_name),
        "public evidence must not reflect the native output filename"
    );
}
