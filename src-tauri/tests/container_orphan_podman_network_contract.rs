use disksage_lib::container_orphan_reclaim::{
    probe_container_orphans, ContainerRuntimeKind, ContainerRuntimeTarget, OrphanCategory,
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::path::PathBuf;

#[cfg(unix)]
const NETWORK_ID: &str =
    "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

#[cfg(unix)]
fn podman_network_target(attached: bool) -> (tempfile::TempDir, ContainerRuntimeTarget) {
    let temp = tempfile::tempdir().expect("temporary Podman runtime directory");
    let runtime = temp.path().join("podman");
    let filtered_membership = if attached {
        r#"[{"Id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}]"#
    } else {
        "[]"
    };
    let script = format!(
        r#"#!/bin/sh
set -eu
[ "${{1:-}}" = "--connection" ] || exit 91
[ "${{2:-}}" = "machine-a" ] || exit 92
shift 2
case "${{1:-}}" in
  info) exit 0 ;;
  container)
    [ "${{2:-}}" = "ps" ] || exit 93
    case " $* " in
      *" --filter network={NETWORK_ID} "*) printf '%s\n' '{filtered_membership}' ;;
      *) printf '%s\n' '[]' ;;
    esac
    ;;
  images|volume) printf '%s\n' '[]' ;;
  network)
    if [ "${{2:-}}" = "ls" ]; then
      printf '%s\n' '[{{"driver":"bridge","id":"{NETWORK_ID}","name":"custom-net"}}]'
      exit 0
    fi
    if [ "${{2:-}}" = "inspect" ]; then
      # Current Podman documentation shows valid inspect JSON that can omit Containers
      # when no running containers are present. Membership must come from `ps --all`.
      printf '%s\n' '[{{"name":"custom-net","id":"{NETWORK_ID}","driver":"bridge","dns_enabled":true}}]'
      exit 0
    fi
    exit 94
    ;;
  *) exit 95 ;;
esac
"#,
    );
    std::fs::write(&runtime, script).expect("write fake Podman runtime");
    let mut permissions = std::fs::metadata(&runtime)
        .expect("fake Podman metadata")
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&runtime, permissions).expect("make fake Podman runtime executable");

    let target = ContainerRuntimeTarget::new(
        ContainerRuntimeKind::PodmanMachine,
        PathBuf::from(&runtime),
        Some("machine-a".to_string()),
    )
    .expect("valid Podman target");
    (temp, target)
}

#[cfg(unix)]
#[test]
fn podman_network_without_any_container_membership_is_a_bounded_candidate() {
    let (_temp, target) = podman_network_target(false);
    let plan = probe_container_orphans(&target);
    let network = plan
        .categories
        .iter()
        .find(|category| category.category == OrphanCategory::Network)
        .expect("network category");

    assert!(network.evidence_complete, "{:?}", network.issue);
    let evidence = network.evidence.as_ref().expect("network evidence");
    assert_eq!(evidence.total_records, 1);
    assert_eq!(evidence.candidate_records, 1);
    assert!(network.approval_phrase.is_some());
}

#[cfg(unix)]
#[test]
fn podman_network_with_stopped_container_membership_is_preserved() {
    let (_temp, target) = podman_network_target(true);
    let plan = probe_container_orphans(&target);
    let network = plan
        .categories
        .iter()
        .find(|category| category.category == OrphanCategory::Network)
        .expect("network category");

    assert!(network.evidence_complete, "{:?}", network.issue);
    let evidence = network.evidence.as_ref().expect("network evidence");
    assert_eq!(evidence.total_records, 1);
    assert_eq!(evidence.candidate_records, 0);
    assert!(network.approval_phrase.is_none());
}
