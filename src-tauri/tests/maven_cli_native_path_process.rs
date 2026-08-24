#![cfg(unix)]

//! Native filesystem paths are OS path values, not UTF-8 protocol fields.
//!
//! The CLI parser must preserve native path bytes instead of rejecting them as argument text.
//! DiskSage's versioned Maven evidence schema deliberately requires a lossless Unicode repository
//! root, so a non-UTF-8 repository must reach that evidence boundary and fail there without
//! reflecting opaque bytes. Private evidence destinations are not serialized as paths in the
//! public receipt and therefore remain valid native filesystem paths.

use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;
use std::process::{Command, Output};

fn run(binary: &str, args: &[&std::ffi::OsStr]) -> Output {
    let executable = match binary {
        "audit" => env!("CARGO_BIN_EXE_disksage-maven-cache-audit"),
        "prune" => env!("CARGO_BIN_EXE_disksage-maven-cache-prune"),
        other => panic!("unexpected Maven CLI selector: {other}"),
    };
    Command::new(executable)
        .args(args)
        .output()
        .expect("Maven CLI must launch for native-path validation")
}

#[test]
fn maven_audit_preserves_non_utf8_repository_path_until_the_evidence_schema_boundary() {
    let parent = tempfile::tempdir().expect("native Maven repository parent must be created");
    let mut name = b"maven-repository-".to_vec();
    name.push(0xff);
    let repository = parent.path().join(OsString::from_vec(name));
    std::fs::create_dir(&repository).expect("native non-UTF-8 Maven repository must be created");

    let args = [std::ffi::OsStr::new("--repository-root"), repository.as_os_str()];
    let audit = run("audit", &args);

    assert_eq!(
        audit.status.code(),
        Some(2),
        "the native path must pass argument decoding and fail at the Unicode evidence boundary"
    );
    assert!(audit.stdout.is_empty());
    assert_eq!(
        String::from_utf8(audit.stderr).expect("bounded diagnostic must remain valid UTF-8"),
        "maven-cache-root-not-utf8\n"
    );
}

fn empty_repository_fingerprint(repository: &std::path::Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-maven-cache-audit"))
        .arg("--repository-root")
        .arg(repository)
        .output()
        .expect("Maven audit CLI must launch for prune fixture preparation");
    assert_eq!(
        output.status.code(),
        Some(0),
        "empty Unicode repository audit must succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let report: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("audit stdout must remain machine JSON");
    report["candidate_set_fingerprint"]
        .as_str()
        .expect("audit must expose the exact candidate-set fingerprint")
        .to_string()
}

fn native_private_output(parent: &std::path::Path, stem: &[u8]) -> std::path::PathBuf {
    let mut name = stem.to_vec();
    name.push(0xff);
    name.extend_from_slice(b".json");
    parent.join(OsString::from_vec(name))
}

#[test]
fn maven_audit_and_prune_accept_non_utf8_private_output_paths_without_public_path_leakage() {
    let repository = tempfile::tempdir().expect("Maven repository fixture must be created");
    let private = tempfile::tempdir().expect("private evidence parent must be created");
    let audit_output = native_private_output(private.path(), b"audit-");

    let audit = Command::new(env!("CARGO_BIN_EXE_disksage-maven-cache-audit"))
        .arg("--repository-root")
        .arg(repository.path())
        .arg("--output")
        .arg(&audit_output)
        .output()
        .expect("Maven audit CLI must launch with a native private-output path");
    assert_eq!(
        audit.status.code(),
        Some(0),
        "a native private-output path must reach secure publication: {}",
        String::from_utf8_lossy(&audit.stderr)
    );
    assert!(audit.stderr.is_empty());
    assert!(audit_output.is_file());
    let audit_public: serde_json::Value =
        serde_json::from_slice(&audit.stdout).expect("audit public receipt must remain machine JSON");
    assert_eq!(audit_public["private_output"]["written"], true);
    assert!(audit_public.get("output").is_none());

    let fingerprint = empty_repository_fingerprint(repository.path());
    let prune_output = native_private_output(private.path(), b"prune-");
    let prune = Command::new(env!("CARGO_BIN_EXE_disksage-maven-cache-prune"))
        .arg("--repository-root")
        .arg(repository.path())
        .args(["--expected-candidate-set-fingerprint", fingerprint.as_str()])
        .arg("--output")
        .arg(&prune_output)
        .output()
        .expect("Maven prune CLI must launch with a native private-output path");
    assert_eq!(
        prune.status.code(),
        Some(0),
        "a native private-output path must reach dry-run evidence publication: {}",
        String::from_utf8_lossy(&prune.stderr)
    );
    assert!(prune.stderr.is_empty());
    assert!(prune_output.is_file());
    let prune_public: serde_json::Value =
        serde_json::from_slice(&prune.stdout).expect("prune public receipt must remain machine JSON");
    assert_eq!(prune_public["private_output"]["written"], true);
    assert_eq!(prune_public["filesystem_mutation_executed"], false);
    assert!(prune_public.get("output").is_none());
}
