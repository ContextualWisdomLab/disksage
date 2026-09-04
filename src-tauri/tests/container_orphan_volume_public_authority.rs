use disksage_lib::container_orphan_public::sanitize_plan;
use disksage_lib::container_orphan_reclaim::{
    probe_container_orphans_with_receipt_dir, ContainerRuntimeKind, ContainerRuntimeTarget,
    OrphanCategory,
};

#[cfg(unix)]
#[test]
fn replaceable_volume_names_never_publish_destructive_authority() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().expect("temporary runtime directory");
    let runtime = temp.path().join("docker");
    std::fs::write(
        &runtime,
        r#"#!/bin/sh
set -eu
case "${1:-}" in
  info|container|images|network) exit 0 ;;
  volume)
    case "${2:-}" in
      ls)
        printf '%s\n' '[{"Name":"owned-cache"}]'
        ;;
      inspect)
        printf '%s\n' '[{"Name":"owned-cache","Driver":"local","CreatedAt":"2026-08-30T00:00:00Z","Labels":{"io.contextualwisdomlab.disksage.owner":"disksage","io.contextualwisdomlab.disksage.reclaimable":"true"}}]'
        ;;
      *) exit 97 ;;
    esac
    ;;
  *) exit 98 ;;
esac
"#,
    )
    .expect("write fake runtime");
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700))
        .expect("make fake runtime executable");

    let receipt_dir = temp.path().join("receipts");
    std::fs::create_dir(&receipt_dir).expect("create receipt directory");
    std::fs::set_permissions(&receipt_dir, std::fs::Permissions::from_mode(0o700))
        .expect("protect receipt directory");

    let target = ContainerRuntimeTarget::new(
        ContainerRuntimeKind::DockerNative,
        runtime,
        None,
    )
    .expect("valid Docker target");
    let raw_plan = probe_container_orphans_with_receipt_dir(&target, &receipt_dir);
    let raw_volume = raw_plan
        .categories
        .iter()
        .find(|entry| entry.category == OrphanCategory::Volume)
        .expect("volume category");
    assert!(raw_volume.approval_phrase.is_some(), "RED requires the backend to expose today's unsafe name-bound volume authority");

    let public_plan = sanitize_plan(raw_plan);
    let volume = public_plan
        .categories
        .iter()
        .find(|entry| entry.category == OrphanCategory::Volume)
        .expect("volume category");

    assert!(
        volume.approval_phrase.is_none(),
        "a reusable volume name cannot authorize deletion of the object that happens to own that name later"
    );
    assert!(
        volume.prune_command.is_none(),
        "read-only volume evidence must not expose a destructive command until deletion is bound to immutable object identity"
    );
}
