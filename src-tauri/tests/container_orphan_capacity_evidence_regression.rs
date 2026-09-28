#![cfg(unix)]

use disksage_lib::container_orphan_public::sanitize_execution;
use disksage_lib::container_orphan_reclaim::{
    execute_container_orphan_prune, probe_container_orphans_with_receipt_dir, ContainerRuntimeKind,
    ContainerRuntimeTarget, OrphanCategory,
};
use std::os::unix::fs::PermissionsExt;

#[test]
fn public_prune_receipt_does_not_claim_host_capacity_without_runtime_store_volume_authority() {
    const FULL_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let temp = tempfile::tempdir().expect("temporary runtime directory");
    let runtime = temp.path().join("docker");
    std::fs::write(
        &runtime,
        format!(
            r#"#!/bin/sh
set -eu
case "${{1:-}}" in
  info)
    printf '%s\n' '{{}}'
    exit 0
    ;;
  container)
    if [ "${{2:-}}" = "ps" ]; then
      printf '%s\n' '{{"ID":"{FULL_ID}","State":"exited","Names":[]}}'
      exit 0
    fi
    if [ "${{2:-}}" = "inspect" ] && [ "${{3:-}}" = "{FULL_ID}" ]; then
      printf '%s\n' '[{{"Id":"{FULL_ID}","Created":"2026-08-30T00:00:00Z","State":{{"Status":"exited"}},"Config":{{"Labels":{{"io.contextualwisdomlab.disksage.owner":"disksage","io.contextualwisdomlab.disksage.reclaimable":"true"}}}}}}]'
      exit 0
    fi
    if [ "${{2:-}}" = "rm" ] && [ "${{3:-}}" = "{FULL_ID}" ] && [ "${{4:-}}" = "" ]; then
      printf '%s\n' '{FULL_ID}'
      exit 0
    fi
    exit 98
    ;;
  *) exit 99 ;;
esac
"#
        ),
    )
    .expect("write fake runtime");
    let mut permissions = std::fs::metadata(&runtime)
        .expect("fake runtime metadata")
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&runtime, permissions).expect("make fake runtime executable");

    let target = ContainerRuntimeTarget::new(ContainerRuntimeKind::DockerNative, runtime, None)
        .expect("valid Docker target");
    let receipts = tempfile::tempdir().unwrap();
    std::fs::set_permissions(receipts.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let plan = probe_container_orphans_with_receipt_dir(&target, receipts.path());
    let container = plan
        .categories
        .iter()
        .find(|category| category.category == OrphanCategory::Container)
        .expect("container category");
    let phrase = container
        .approval_phrase
        .as_deref()
        .expect("candidate-bound approval phrase");

    let execution = execute_container_orphan_prune(
        &target,
        OrphanCategory::Container,
        phrase,
        "Remove the exact stopped-container candidate verified by DiskSage.",
        1,
        receipts.path(),
    )
    .expect("exact candidate removal must succeed");
    assert!(execution.executed);

    let public_receipt = sanitize_execution(execution);
    assert_eq!(
        public_receipt.before_available_bytes, None,
        "the process working-directory volume is not authoritative for runtime-store capacity"
    );
    assert_eq!(public_receipt.after_available_bytes, None);
    assert_eq!(public_receipt.observed_available_gain_bytes, None);
}
