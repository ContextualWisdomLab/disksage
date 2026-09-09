use disksage_lib::container_orphan_reclaim::{
    probe_container_orphans, ContainerRuntimeKind, ContainerRuntimeTarget, OrphanCategory,
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::path::PathBuf;

#[cfg(unix)]
#[test]
fn podman_image_audit_uses_authoritative_dangling_filter_without_container_count() {
    const DANGLING_ID: &str =
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
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
    case " $* " in
      *" --filter ancestor="*) case " $* " in *" --external "*) ;; *) exit 90 ;; esac ;;
    esac
    printf '[]\n'
    ;;
  images)
    [ "$#" -eq 6 ] || exit 93
    [ "${{2:-}}" = "--filter" ] || exit 94
    [ "${{3:-}}" = "dangling=true" ] || exit 95
    [ "${{4:-}}" = "--no-trunc" ] || exit 96
    [ "${{5:-}}" = "--format" ] || exit 97
    [ "${{6:-}}" = "json" ] || exit 98
    printf '%s\n' '[{{"id":"{DANGLING_ID}","names":["<none>"],"size":250665}}]'
    ;;
  volume|network) exit 0 ;;
  *) exit 99 ;;
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

    let plan = probe_container_orphans(&target);
    let image = plan
        .categories
        .iter()
        .find(|category| category.category == OrphanCategory::Image)
        .expect("image category");

    assert!(image.evidence_complete, "{:?}", image.issue);
    let evidence = image.evidence.as_ref().expect("image evidence");
    assert_eq!(evidence.total_records, 1);
    assert_eq!(evidence.candidate_records, 1);
    assert_eq!(evidence.candidate_size_sum_bytes, Some(250665));
    assert!(
        image.approval_phrase.is_none(),
        "read-only discovery without a receipt directory must not publish unusable authority"
    );
}
