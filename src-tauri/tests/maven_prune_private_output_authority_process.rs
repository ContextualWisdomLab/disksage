#![cfg(unix)]

//! Maven prune private evidence must use the same fail-closed publication boundary as other private evidence.

use std::os::unix::fs::PermissionsExt;
use std::process::Command;

fn candidate_fingerprint(repository: &std::path::Path) -> String {
    let audit = Command::new(env!("CARGO_BIN_EXE_disksage-maven-cache-audit"))
        .arg("--repository-root")
        .arg(repository)
        .output()
        .expect("Maven audit CLI must launch to bind the prune candidate set");
    assert_eq!(audit.status.code(), Some(0));
    assert!(audit.stderr.is_empty());
    let audit_json: serde_json::Value =
        serde_json::from_slice(&audit.stdout).expect("audit output must remain machine JSON");
    audit_json["candidate_set_fingerprint"]
        .as_str()
        .expect("audit must expose a candidate-set fingerprint")
        .to_string()
}

#[test]
fn maven_prune_private_output_rejects_shared_writable_parent_and_redacts_path() {
    let repository = tempfile::tempdir().expect("Maven repository fixture must be created");
    let fingerprint = candidate_fingerprint(repository.path());
    let private = tempfile::tempdir().expect("private evidence parent must be created");
    let unsafe_output = private.path().join("unsafe-prune-report.json");

    let mut permissions = std::fs::metadata(private.path()).unwrap().permissions();
    permissions.set_mode(0o770);
    std::fs::set_permissions(private.path(), permissions).unwrap();

    let rejected = Command::new(env!("CARGO_BIN_EXE_disksage-maven-cache-prune"))
        .arg("--repository-root")
        .arg(repository.path())
        .args(["--expected-candidate-set-fingerprint", fingerprint.as_str()])
        .arg("--output")
        .arg(&unsafe_output)
        .output()
        .expect("Maven prune CLI must launch for private-output authority validation");
    assert_eq!(rejected.status.code(), Some(2));
    assert!(rejected.stdout.is_empty());
    let stderr = String::from_utf8(rejected.stderr).expect("diagnostic must remain valid UTF-8");
    assert_eq!(stderr.trim_end(), "private-evidence-parent-writable-by-others");
    assert!(!unsafe_output.exists());

    let mut permissions = std::fs::metadata(private.path()).unwrap().permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(private.path(), permissions).unwrap();
    let safe_output = private.path().join("customer-secret-prune-report.json");

    let accepted = Command::new(env!("CARGO_BIN_EXE_disksage-maven-cache-prune"))
        .arg("--repository-root")
        .arg(repository.path())
        .args(["--expected-candidate-set-fingerprint", fingerprint.as_str()])
        .arg("--output")
        .arg(&safe_output)
        .output()
        .expect("Maven prune CLI must launch for private-output publication validation");
    assert_eq!(
        accepted.status.code(),
        Some(0),
        "owner-only dry-run evidence publication must succeed: {}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    assert!(accepted.stderr.is_empty());
    assert!(safe_output.is_file());
    assert_eq!(std::fs::metadata(&safe_output).unwrap().permissions().mode() & 0o777, 0o600);

    let public: serde_json::Value =
        serde_json::from_slice(&accepted.stdout).expect("public prune summary must remain machine JSON");
    assert_eq!(public["private_output"]["written"], true);
    assert_eq!(public["private_output"]["unix_mode"], "0600");
    assert_eq!(public["filesystem_mutation_executed"], false);
    let public_text = String::from_utf8(accepted.stdout).expect("public JSON must be UTF-8");
    assert!(!public_text.contains("customer-secret-prune-report.json"));
    assert!(!public_text.contains(private.path().to_string_lossy().as_ref()));
    assert!(!public_text.contains(repository.path().to_string_lossy().as_ref()));
}
