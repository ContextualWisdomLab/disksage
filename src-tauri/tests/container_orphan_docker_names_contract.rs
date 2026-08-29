#![cfg(unix)]

use disksage_lib::container_orphan_reclaim::{
    probe_container_orphans, ContainerRuntimeKind, ContainerRuntimeTarget, OrphanCategory,
};
use std::os::unix::fs::PermissionsExt;

fn assert_docker_names_shape_is_accepted(names: &str) {
    const FULL_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let temp = tempfile::tempdir().expect("temporary runtime directory");
    let runtime = temp.path().join("docker");
    let script = format!(
        r#"#!/bin/sh
set -eu
case "${{1:-}}" in
  info) exit 0 ;;
  container)
    case "${{2:-}}" in
      ps)
        case " $* " in *" --no-trunc "*) ;; *) echo "missing --no-trunc" >&2; exit 92 ;; esac
        printf '%s\n' '{{"ID":"{FULL_ID}","State":"exited","Names":"{names}"}}'
        ;;
      inspect)
        printf '%s\n' '{{"Id":"{FULL_ID}","Created":"2026-01-01T00:00:00Z","State":{{"Status":"exited"}},"Config":{{"Labels":{{"io.contextualwisdomlab.disksage.owner":"disksage","io.contextualwisdomlab.disksage.reclaimable":"true"}}}}}}'
        ;;
      *) exit 91 ;;
    esac
    ;;
  images|volume|network) exit 0 ;;
  *) exit 93 ;;
esac
"#
    );
    std::fs::write(&runtime, script).expect("write fake Docker runtime");
    let mut permissions = std::fs::metadata(&runtime)
        .expect("fake runtime metadata")
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&runtime, permissions).expect("make fake runtime executable");

    let target = ContainerRuntimeTarget::new(
        ContainerRuntimeKind::DockerNative,
        runtime,
        None,
    )
    .expect("valid Docker target");
    let plan = probe_container_orphans(&target);
    let container = plan
        .categories
        .iter()
        .find(|category| category.category == OrphanCategory::Container)
        .expect("container category");

    assert!(container.evidence_complete, "{:?}", container.issue);
    let evidence = container.evidence.as_ref().expect("container evidence");
    assert_eq!(evidence.total_records, 1);
    assert_eq!(evidence.candidate_records, 1);
    assert!(container.approval_phrase.is_none());
}

#[test]
fn docker_plain_comma_joined_names_do_not_break_stopped_container_audit() {
    assert_docker_names_shape_is_accepted("web,worker");
}

#[test]
fn docker_plain_single_name_does_not_break_stopped_container_audit() {
    assert_docker_names_shape_is_accepted("web");
}
