#![cfg(unix)]

use disksage_lib::container_orphan_reclaim::{
    execute_container_orphan_prune, probe_container_orphans_with_receipt_dir,
    ContainerRuntimeKind, ContainerRuntimeTarget, OrphanCategory,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

const CONTAINER_ID: &str =
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn write_fake_runtime(root: &std::path::Path) -> PathBuf {
    let script = root.join("fake-docker");
    let body = format!(
        r#"#!/bin/sh
set -eu
case "${{1:-}} ${{2:-}}" in
  "info ")
    printf '%s\n' '{{}}'
    ;;
  "container ps")
    printf '%s\n' '{{"ID":"{id}","State":"exited","Names":["fixture"]}}'
    ;;
  "container inspect")
    printf '%s\n' '[{{"Id":"{id}","Created":"2026-08-31T00:00:00Z","State":{{"Status":"exited"}},"Config":{{"Labels":{{"io.contextualwisdomlab.disksage.owner":"disksage","io.contextualwisdomlab.disksage.reclaimable":"true"}}}}}}]'
    ;;
  "container rm")
    dd if=/dev/zero bs=1024 count=1100 2>/dev/null | tr '\000' x
    ;;
  "images --filter"|"volume ls"|"network ls"|"buildx du")
    printf '%s\n' '[]'
    ;;
  *)
    printf '%s\n' '[]'
    ;;
esac
"#,
        id = CONTAINER_ID
    );
    fs::write(&script, body).unwrap();
    let mut permissions = fs::metadata(&script).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&script, permissions).unwrap();
    script
}

#[test]
fn oversized_mutation_output_marks_receipt_truncated() {
    let fixture = tempfile::tempdir().unwrap();
    let receipt_dir = fixture.path().join("receipts");
    fs::create_dir(&receipt_dir).unwrap();
    fs::set_permissions(&receipt_dir, fs::Permissions::from_mode(0o700)).unwrap();

    let target = ContainerRuntimeTarget::new(
        ContainerRuntimeKind::DockerNative,
        write_fake_runtime(fixture.path()),
        None,
    )
    .unwrap();

    let plan = probe_container_orphans_with_receipt_dir(&target, &receipt_dir);
    let container = plan
        .categories
        .iter()
        .find(|item| item.category == OrphanCategory::Container)
        .expect("container category");
    assert!(container.evidence_complete, "{:?}", container.issue);
    let phrase = container
        .approval_phrase
        .as_deref()
        .expect("container approval phrase");

    let receipt = execute_container_orphan_prune(
        &target,
        OrphanCategory::Container,
        phrase,
        "reviewed exact fixture container",
        1_788_134_400_000,
        &receipt_dir,
    )
    .unwrap();

    assert_eq!(receipt.status_code, -1);
    assert!(receipt.executed);
    assert!(
        receipt.output_truncated,
        "discarded mutation output must be explicit in the persisted execution receipt"
    );
}
