use disksage_lib::container_orphan_reclaim::{
    probe_container_orphans, ContainerRuntimeKind, ContainerRuntimeTarget, OrphanCategory,
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::path::PathBuf;

#[cfg(unix)]
fn podman_target_with_container_json(container_json: &str) -> (tempfile::TempDir, ContainerRuntimeTarget) {
    let temp = tempfile::tempdir().expect("temporary Podman runtime directory");
    let runtime = temp.path().join("podman");
    let script = format!(
        r#"#!/bin/sh
set -eu
[ "${{1:-}}" = "--connection" ] || exit 91
[ "${{2:-}}" = "machine-a" ] || exit 92
shift 2
case "${{1:-}}" in
  info) exit 0 ;;
  container)
    if [ "${{2:-}}" = "ps" ]; then
      printf '%s\n' '{container_json}'
      exit 0
    fi
    if [ "${{2:-}}" = "inspect" ]; then
      printf '[{{"Id":"%s","Mounts":[]}}]\n' "${{3:-}}"
      exit 0
    fi
    exit 93
    ;;
  images|volume|network) exit 0 ;;
  *) exit 94 ;;
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
fn podman_stopped_is_removable_while_known_prestart_and_transitional_states_are_preserved() {
    const STOPPED_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const INITIALIZED_ID: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const STOPPING_ID: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    const CONFIGURED_ID: &str = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";
    let container_json = format!(
        r#"[{{"Id":"{STOPPED_ID}","State":"stopped","Names":[]}},{{"Id":"{INITIALIZED_ID}","State":"initialized","Names":[]}},{{"Id":"{STOPPING_ID}","State":"stopping","Names":[]}},{{"Id":"{CONFIGURED_ID}","State":"configured","Names":[]}}]"#,
    );
    let (_temp, target) = podman_target_with_container_json(&container_json);

    let plan = probe_container_orphans(&target);
    let container = plan
        .categories
        .iter()
        .find(|category| category.category == OrphanCategory::Container)
        .expect("container category");

    assert!(container.evidence_complete, "{:?}", container.issue);
    let evidence = container.evidence.as_ref().expect("container evidence");
    assert_eq!(evidence.total_records, 4);
    assert_eq!(evidence.candidate_records, 1);
    assert!(container.approval_phrase.is_some());
}

#[cfg(unix)]
#[test]
fn podman_unknown_state_remains_fail_closed() {
    const UNKNOWN_ID: &str = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
    let container_json = format!(r#"[{{"Id":"{UNKNOWN_ID}","State":"unknown","Names":[]}}]"#);
    let (_temp, target) = podman_target_with_container_json(&container_json);

    let plan = probe_container_orphans(&target);
    let container = plan
        .categories
        .iter()
        .find(|category| category.category == OrphanCategory::Container)
        .expect("container category");

    assert!(!container.evidence_complete);
    assert_eq!(container.issue.as_deref(), Some("unknown-container-state:unknown"));
}
