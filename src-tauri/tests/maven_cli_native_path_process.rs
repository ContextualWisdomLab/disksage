#![cfg(unix)]

//! Native filesystem paths are OS path values, not UTF-8 protocol fields.
//!
//! Exercise both shipped Maven CLIs with an absolute repository root containing a non-UTF-8 byte.
//! The audit must succeed and provide the exact candidate-set fingerprint consumed by the prune
//! dry-run. Neither command may reject the valid native path during argument decoding.

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::process::Command;

#[test]
fn maven_audit_and_prune_accept_a_non_utf8_absolute_repository_root() {
    let parent = tempfile::tempdir().expect("native Maven repository parent must be created");
    let mut name = b"maven-repository-".to_vec();
    name.push(0xff);
    let repository = parent.path().join(OsString::from_vec(name));
    std::fs::create_dir(&repository).expect("native non-UTF-8 Maven repository must be created");

    let audit = Command::new(env!("CARGO_BIN_EXE_disksage-maven-cache-audit"))
        .arg("--repository-root")
        .arg(&repository)
        .output()
        .expect("Maven audit CLI must launch with a native repository path");
    assert_eq!(
        audit.status.code(),
        Some(0),
        "valid native repository paths must not be rejected as UTF-8 protocol input: {}",
        String::from_utf8_lossy(&audit.stderr)
    );
    assert!(audit.stderr.is_empty());
    let audit_json: serde_json::Value = serde_json::from_slice(&audit.stdout)
        .expect("successful Maven audit stdout must remain machine-readable JSON");
    let fingerprint = audit_json["candidate_set_fingerprint"]
        .as_str()
        .expect("Maven audit must expose its candidate-set fingerprint")
        .to_string();

    let prune = Command::new(env!("CARGO_BIN_EXE_disksage-maven-cache-prune"))
        .arg("--repository-root")
        .arg(&repository)
        .args(["--expected-candidate-set-fingerprint", fingerprint.as_str()])
        .output()
        .expect("Maven prune CLI must launch with a native repository path");
    assert_eq!(
        prune.status.code(),
        Some(0),
        "valid native repository paths must reach the bounded dry-run path: {}",
        String::from_utf8_lossy(&prune.stderr)
    );
    assert!(prune.stderr.is_empty());
    let prune_json: serde_json::Value = serde_json::from_slice(&prune.stdout)
        .expect("successful Maven prune stdout must remain machine-readable JSON");
    assert_eq!(prune_json["apply_requested"], false);
    assert_eq!(prune_json["filesystem_mutation_executed"], false);
}
