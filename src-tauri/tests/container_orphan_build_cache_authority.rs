#![cfg(unix)]

use disksage_lib::container_orphan_reclaim::{
    execute_container_orphan_prune, ContainerRuntimeKind, ContainerRuntimeTarget, OrphanCategory,
};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

#[test]
fn build_cache_execution_fails_closed_before_any_runtime_mutation() {
    let receipt_dir = tempfile::tempdir().expect("private receipt tempdir");
    std::fs::set_permissions(
        receipt_dir.path(),
        std::fs::Permissions::from_mode(0o700),
    )
    .expect("private receipt permissions");

    let target = ContainerRuntimeTarget::new(
        ContainerRuntimeKind::DockerNative,
        PathBuf::from("/definitely/missing/disksage-docker"),
        None,
    )
    .expect("static docker target");

    let error = execute_container_orphan_prune(
        &target,
        OrphanCategory::BuildCache,
        "not-authorized",
        "reviewed exact candidates",
        1,
        receipt_dir.path(),
    )
    .expect_err("BuildKit cache has no exact identity-bound deletion primitive");

    assert_eq!(error, "orphan-prune-build-cache-exact-delete-unavailable");
}
