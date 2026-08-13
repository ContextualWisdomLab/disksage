#![cfg(unix)]

use crate::brew_cleanup::{
    write_audit_record, write_audit_record_with_before_create_hook, BrewCleanupAuditRecord,
    SCHEMA_VERSION,
};
use serde_json::json;
use std::os::unix::fs::PermissionsExt;

fn valid_record() -> BrewCleanupAuditRecord {
    let plan_fingerprint = "a".repeat(64);
    serde_json::from_value(json!({
        "schema_version": SCHEMA_VERSION,
        "plan": {
            "schema_version": SCHEMA_VERSION,
            "platform": "macos",
            "brew_path": "/opt/homebrew/bin/brew",
            "brew_identity": "1:2",
            "brew_version": "Homebrew 6.0.12",
            "dry_run_output": "Would remove old downloads",
            "dry_run_output_truncated": false,
            "observed_at_ms": 10,
            "plan_fingerprint": plan_fingerprint,
            "exact_approval_phrase": format!(
                "DiskSage Homebrew cleanup 승인 {}",
                "a".repeat(64)
            )
        },
        "judgment_id": "b".repeat(64),
        "verdict": "safe",
        "reason": "fixed maintenance command",
        "model_name": "test-model",
        "judged_at_ms": 20,
        "executed_at_ms": 30,
        "approved_by": "human:local:test",
        "command": ["brew", "cleanup", "--prune-prefix"],
        "status_code": 0,
        "stdout": "",
        "stderr": "",
        "output_truncated": false,
        "rationale": "approved after bounded dry-run review"
    }))
    .expect("test audit record must satisfy the production schema")
}

fn brew_cleanup_source() -> String {
    std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/brew_cleanup.rs"),
    )
    .expect("brew cleanup source must be readable")
}

#[test]
fn shared_writable_app_data_parent_fails_closed_without_creating_audit_storage() {
    for unsafe_write_bit in [0o020, 0o002] {
        let app_data = tempfile::tempdir().expect("temporary app-data directory");
        std::fs::set_permissions(
            app_data.path(),
            std::fs::Permissions::from_mode(0o700 | unsafe_write_bit),
        )
        .expect("make app-data directory shared-writable for regression");

        let error = write_audit_record(app_data.path(), &valid_record())
            .expect_err("shared-writable audit parent must fail closed");

        assert_eq!(error, "brew-cleanup-audit-parent-writable-by-others");
        assert!(
            !app_data.path().join("brew-cleanup-records").exists(),
            "refusing an unsafe parent must not create durable authority storage"
        );
    }
}

#[test]
fn shared_writable_audit_directory_fails_closed_without_creating_a_record() {
    for unsafe_write_bit in [0o020, 0o002] {
        let app_data = tempfile::tempdir().expect("temporary app-data directory");
        let audit_directory = app_data.path().join("brew-cleanup-records");
        std::fs::create_dir(&audit_directory).expect("create audit directory fixture");
        std::fs::set_permissions(
            &audit_directory,
            std::fs::Permissions::from_mode(0o700 | unsafe_write_bit),
        )
        .expect("make audit directory shared-writable for regression");

        let error = write_audit_record(app_data.path(), &valid_record())
            .expect_err("shared-writable audit directory must fail closed");

        assert_eq!(error, "brew-cleanup-audit-directory-writable-by-others");
        assert_eq!(
            std::fs::read_dir(&audit_directory)
                .expect("refused audit directory remains readable")
                .count(),
            0,
            "refusing unsafe durable storage must not create an authority record"
        );
    }
}

#[test]
fn audit_storage_is_private_at_creation_and_object_bound_for_hardening() {
    let source = brew_cleanup_source();
    let writer = source
        .split_once("pub fn write_audit_record(")
        .map(|(_, writer)| writer)
        .expect("audit writer must remain present");

    assert!(
        source.contains("builder.mode(0o700);"),
        "the dedicated audit directory must be private from its creation boundary"
    );
    assert!(
        writer.contains("libc::openat(") && writer.contains("0o400,"),
        "audit records must be owner-read-only at descriptor-relative create_new"
    );
    assert!(
        source.contains("file.set_permissions(permissions)"),
        "post-write hardening must remain bound to the opened audit record"
    );
    assert!(
        !source.contains("std::fs::set_permissions(&path, permissions)"),
        "audit hardening must not re-resolve a replaceable pathname after create_new"
    );
}

#[test]
fn audit_publication_must_be_bound_to_an_opened_directory_identity() {
    let source = brew_cleanup_source();
    let writer = source
        .split_once("pub fn write_audit_record(")
        .map(|(_, writer)| writer)
        .expect("audit writer must remain present");

    assert!(
        writer.contains("openat(") || writer.contains("openat2("),
        "record creation must be relative to an already-opened audit directory identity"
    );
    assert!(
        writer.contains("O_NOFOLLOW"),
        "directory-relative record publication must reject symbolic-link substitution"
    );
    assert!(
        !writer.contains(".open(&path)"),
        "record publication must not re-resolve a replaceable directory pathname"
    );
    assert!(
        !writer.contains("std::fs::remove_file(&path)"),
        "failure cleanup must remain relative to the same opened audit directory identity"
    );
}

#[test]
fn directory_replacement_after_authorization_cannot_redirect_publication() {
    let app_data = tempfile::tempdir().expect("temporary app-data directory");
    let audit_directory = app_data.path().join("brew-cleanup-records");
    let moved_directory = app_data.path().join("authorized-audit-directory-moved");

    let error = write_audit_record_with_before_create_hook(
        app_data.path(),
        &valid_record(),
        || {
            std::fs::rename(&audit_directory, &moved_directory)
                .expect("move the already-authorized directory");
            std::fs::create_dir(&audit_directory).expect("install replacement directory");
            std::fs::set_permissions(
                &audit_directory,
                std::fs::Permissions::from_mode(0o700),
            )
            .expect("keep the replacement privately writable so identity is the only defect");
        },
    )
    .expect_err("directory identity drift must fail closed");

    assert_eq!(error, "brew-cleanup-audit-directory-identity-drift");
    assert_eq!(
        std::fs::read_dir(&audit_directory)
            .expect("replacement directory remains readable")
            .count(),
        0,
        "the replacement pathname must receive no authority record"
    );
    assert_eq!(
        std::fs::read_dir(&moved_directory)
            .expect("original authorized directory remains readable")
            .count(),
        0,
        "failed publication must clean the descriptor-relative partial record"
    );
}

#[test]
fn standard_intel_homebrew_repository_executable_is_admitted_without_following_alias_symlinks() {
    let source = brew_cleanup_source();

    assert!(
        source.contains("Path::new(\"/usr/local/Homebrew/bin/brew\")"),
        "fixed executable candidates must include the standard Intel Homebrew repository target"
    );
    assert!(
        source.contains("!metadata.file_type().is_symlink()"),
        "supporting Intel Homebrew must not weaken symbolic-link rejection"
    );
}
