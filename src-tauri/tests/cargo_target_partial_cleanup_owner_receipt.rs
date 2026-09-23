#![cfg(unix)]

#[path = "../src/unix_capability_cleanup.rs"]
mod unix_capability_cleanup;
#[path = "../src/unix_holder_authority.rs"]
mod unix_holder_authority;
#[path = "../src/cargo_target_reclaim.rs"]
mod cargo_target_reclaim;

use std::os::unix::fs::PermissionsExt;

#[test]
fn owner_partial_receipt_preserves_target_view_evidence_without_reclaim_credit() {
    assert_ne!(
        unsafe { libc::geteuid() },
        0,
        "partial-clean acceptance requires the unprivileged product execution boundary"
    );

    let temp = tempfile::tempdir().expect("temp root");
    let project = temp.path().join("project");
    let target = project.join("target");
    let child = target.join("blocked-child");
    let artifact = child.join("artifact.bin");
    std::fs::create_dir_all(&child).expect("child tree");
    std::fs::write(
        project.join("Cargo.toml"),
        "[package]\nname=\"partial-owner-receipt\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .expect("manifest");
    std::fs::write(&artifact, vec![7u8; 16 * 1024]).expect("artifact");

    let fake_cargo = project.join("fake-cargo");
    std::fs::write(&fake_cargo, "#!/bin/sh\nexit 99\n").expect("fake cargo");
    let mut cargo_permissions = std::fs::metadata(&fake_cargo)
        .expect("fake cargo metadata")
        .permissions();
    cargo_permissions.set_mode(0o755);
    std::fs::set_permissions(&fake_cargo, cargo_permissions).expect("fake cargo executable");

    let original_mode = std::fs::metadata(&target)
        .expect("target metadata")
        .permissions()
        .mode();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o555))
        .expect("make retained root non-writable");

    let outcome = cargo_target_reclaim::clean_cargo_target_with_active_use(
        &project,
        &target,
        &fake_cargo,
        |_| Ok(()),
    );

    std::fs::set_permissions(
        &target,
        std::fs::Permissions::from_mode(original_mode & 0o7777),
    )
    .expect("restore retained root permissions");

    let error = outcome.expect_err("owner must report the irreversible partial cleanup");
    assert!(
        !artifact.exists(),
        "fixture must prove irreversible mutation happened before the failure"
    );
    assert!(
        child.exists(),
        "failed root-relative directory removal must leave the child directory"
    );

    let receipt: serde_json::Value = serde_json::from_str(&error)
        .expect("owner partial-clean failure must remain a versioned JSON receipt");
    assert_eq!(receipt["schema_version"], 1);
    assert_eq!(receipt["code"], "cargo-target-partial-clean-failed");
    assert_eq!(receipt["completion"], "partial");
    assert_eq!(receipt["entries_removed"], 1);

    let before = receipt["target_view_allocated_bytes_before"]
        .as_u64()
        .expect("owner receipt must retain pre-clean target-view allocation evidence");
    let after = receipt["target_view_allocated_bytes_after"]
        .as_u64()
        .expect("owner receipt must retain best-effort post-failure target-view allocation evidence");
    let reduction = receipt["observed_target_view_reduction_bytes"]
        .as_u64()
        .expect("owner receipt must expose only observed target-view reduction");

    assert!(before > 0);
    assert_eq!(reduction, before.saturating_sub(after));
    assert_eq!(
        receipt["ledger_reclaim_bytes"], 0,
        "target-view reduction is not proof that physical blocks were released"
    );
    assert!(
        receipt["cause"].as_str().is_some_and(|cause| !cause.is_empty()),
        "partial-clean receipt must retain the causal filesystem failure"
    );
}
