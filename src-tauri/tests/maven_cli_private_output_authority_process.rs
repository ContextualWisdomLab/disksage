#![cfg(unix)]

//! Maven audit private evidence must use the shared fail-closed publication boundary.

use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[test]
fn maven_audit_private_output_rejects_shared_writable_parent_and_redacts_path() {
    let repository = tempfile::tempdir().expect("Maven repository fixture must be created");
    let private = tempfile::tempdir().expect("private evidence parent must be created");
    let unsafe_output = private.path().join("unsafe-customer-report.json");

    let mut permissions = std::fs::metadata(private.path())
        .expect("private parent metadata must be readable")
        .permissions();
    permissions.set_mode(0o770);
    std::fs::set_permissions(private.path(), permissions)
        .expect("shared-writable parent fixture must be configured");

    let rejected = Command::new(env!("CARGO_BIN_EXE_disksage-maven-cache-audit"))
        .arg("--repository-root")
        .arg(repository.path())
        .arg("--output")
        .arg(&unsafe_output)
        .output()
        .expect("Maven audit CLI must launch for private-output authority validation");
    assert_eq!(rejected.status.code(), Some(2));
    assert!(rejected.stdout.is_empty());
    let rejected_stderr =
        String::from_utf8(rejected.stderr).expect("diagnostic must remain valid UTF-8");
    assert_eq!(
        rejected_stderr.trim_end(),
        "private-evidence-parent-writable-by-others"
    );
    assert!(!unsafe_output.exists());

    let mut permissions = std::fs::metadata(private.path())
        .expect("private parent metadata must remain readable")
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(private.path(), permissions)
        .expect("private parent must be restored to owner-only authority");
    let safe_output = private.path().join("customer-secret-report.json");

    let accepted = Command::new(env!("CARGO_BIN_EXE_disksage-maven-cache-audit"))
        .arg("--repository-root")
        .arg(repository.path())
        .arg("--output")
        .arg(&safe_output)
        .output()
        .expect("Maven audit CLI must launch for private-output publication validation");
    assert_eq!(
        accepted.status.code(),
        Some(0),
        "owner-only private publication must succeed: {}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    assert!(accepted.stderr.is_empty());
    assert!(safe_output.is_file());
    assert_eq!(
        std::fs::metadata(&safe_output)
            .expect("private report metadata must be readable")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );

    let public: serde_json::Value = serde_json::from_slice(&accepted.stdout)
        .expect("public Maven summary must remain machine-readable JSON");
    assert_eq!(public["private_output"]["written"], true);
    assert_eq!(public["private_output"]["unix_mode"], "0600");
    assert_eq!(public["private_output"]["create_new"], true);
    let public_text = String::from_utf8(accepted.stdout).expect("public JSON must be UTF-8");
    assert!(!public_text.contains("customer-secret-report.json"));
    assert!(!public_text.contains(private.path().to_string_lossy().as_ref()));
    assert!(!public_text.contains(repository.path().to_string_lossy().as_ref()));
}
