use disksage_lib::container_orphan_reclaim::{
    probe_container_orphans, ContainerRuntimeKind, ContainerRuntimeTarget, OrphanCategory,
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
#[test]
fn docker_image_used_by_a_container_is_not_a_prune_candidate() {
    const IMAGE_ID: &str =
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const CONTAINER_ID: &str =
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    let temp = tempfile::tempdir().expect("temporary Docker runtime directory");
    let runtime = temp.path().join("docker");
    let script = format!(
        r#"#!/bin/sh
set -eu
case "${{1:-}}" in
  info) exit 0 ;;
  container)
    [ "${{2:-}}" = "ps" ] || exit 91
    case " $* " in
      *" --filter ancestor={IMAGE_ID} "*)
        printf '%s\n' '{{"ID":"{CONTAINER_ID}","State":"running","Names":["consumer"]}}'
        ;;
      *) exit 0 ;;
    esac
    ;;
  images)
    case " $* " in *" --all "*) ;; *) exit 92 ;; esac
    case " $* " in *" --no-trunc "*) ;; *) exit 93 ;; esac
    printf '%s\n' '{{"Containers":"N/A","ID":"{IMAGE_ID}","Repository":"<none>","Size":"72.9MB","Tag":"<none>"}}'
    ;;
  volume|network) exit 0 ;;
  *) exit 94 ;;
esac
"#,
    );
    std::fs::write(&runtime, script).expect("write fake Docker runtime");
    let mut permissions = std::fs::metadata(&runtime)
        .expect("fake Docker metadata")
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&runtime, permissions).expect("make fake Docker runtime executable");

    let target = ContainerRuntimeTarget::new(
        ContainerRuntimeKind::DockerNative,
        runtime,
        None,
    )
    .expect("valid Docker target");

    let plan = probe_container_orphans(&target);
    let image = plan
        .categories
        .iter()
        .find(|category| category.category == OrphanCategory::Image)
        .expect("image category");

    assert!(image.evidence_complete, "{:?}", image.issue);
    let evidence = image.evidence.as_ref().expect("image evidence");
    assert_eq!(evidence.total_records, 1);
    assert_eq!(
        evidence.candidate_records, 0,
        "an image referenced by a container must not receive deletion authority"
    );
    assert!(image.approval_phrase.is_none());
}
