#![cfg(all(feature = "cloud-cli", unix))]

use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

/// Native private-evidence destinations are filesystem paths, not UTF-8 protocol fields.
///
/// Exercise the shipped feature-gated binary end to end so a future parser or receipt change
/// cannot silently reintroduce UTF-8 coercion after the CLI has accepted a valid native path.
#[test]
fn duplicate_audit_publishes_to_non_utf8_private_output_without_path_leakage() {
    let source = tempfile::tempdir().expect("audit source must be created");
    let private_parent = tempfile::tempdir().expect("private evidence parent must be created");
    let canonical_source =
        std::fs::canonicalize(source.path()).expect("audit source must remain canonicalizable");

    let mut filename_bytes = b"duplicate-audit-private-".to_vec();
    filename_bytes.push(0xff);
    filename_bytes.extend_from_slice(b".json");
    let filename = OsString::from_vec(filename_bytes.clone());
    let private_output = private_parent.path().join(&filename);

    let output = Command::new(env!("CARGO_BIN_EXE_disksage-duplicate-audit"))
        .arg("--root")
        .arg(source.path())
        .args(["--min-bytes", "1", "--max-entries", "10", "--private-output"])
        .arg(&private_output)
        .output()
        .expect("duplicate-audit CLI must launch with a native private-output path");

    #[cfg(target_os = "macos")]
    if output.status.code() == Some(2)
        && String::from_utf8_lossy(&output.stderr).contains("private-evidence-create-failed")
    {
        // APFS rejects this byte under the active locale; Linux CI exercises the
        // lossless native private-output branch while macOS keeps the unsupported case explicit.
        return;
    }

    assert_eq!(
        output.status.code(),
        Some(0),
        "valid native private-output paths must not be rejected as UTF-8 protocol input: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("successful duplicate audit must keep stdout machine-readable");
    assert_eq!(summary["schema_version"], 1);
    assert_eq!(summary["mutation_performed"], false);
    assert_eq!(summary["local_paths_included"], false);
    assert_eq!(summary["private_output"]["written"], true);
    assert_eq!(summary["private_output"]["create_new"], true);
    assert_eq!(summary["private_output"]["unix_mode"], "0600");
    assert_eq!(summary["private_output"]["contains_sensitive_local_paths"], true);
    assert_eq!(summary["private_output"]["is_approval"], false);
    assert!(summary["private_output"].get("path").is_none());

    assert!(private_output.is_file());
    assert_eq!(
        std::fs::metadata(&private_output)
            .expect("private evidence metadata must be readable")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert!(
        !output
            .stdout
            .windows(filename_bytes.len())
            .any(|window| window == filename_bytes),
        "public evidence must not reflect the native private-output filename"
    );
    let source_bytes = source.path().as_os_str().as_bytes();
    assert!(
        !output
            .stdout
            .windows(source_bytes.len())
            .any(|window| window == source_bytes),
        "public evidence must not reflect the caller-visible audited source path"
    );
    let canonical_source_bytes = canonical_source.as_os_str().as_bytes();
    assert!(
        !output
            .stdout
            .windows(canonical_source_bytes.len())
            .any(|window| window == canonical_source_bytes),
        "public evidence must not reflect the canonical audited source path"
    );

    let private_bytes = std::fs::read(&private_output).expect("private evidence must be readable");
    let private_json: serde_json::Value =
        serde_json::from_slice(&private_bytes).expect("private evidence must be valid JSON");
    assert_eq!(private_json["schema_version"], 1);
    assert_eq!(
        private_json["source_root"].as_str(),
        canonical_source.to_str(),
        "the private report must bind the canonical audited source path"
    );

    assert_eq!(private_output.file_name().unwrap().as_bytes(), filename_bytes);
}
