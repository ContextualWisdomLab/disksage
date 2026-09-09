use disksage_lib::container_orphan_reclaim::{
    execute_container_orphan_prune, probe_container_orphans_with_receipt_dir,
    ContainerRuntimeKind, ContainerRuntimeTarget, OrphanCategory,
};
use std::path::PathBuf;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
#[test]
fn nonzero_multi_target_remove_records_that_mutation_was_attempted() {
    const FIRST_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const SECOND_ID: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    let temp = tempfile::tempdir().expect("temporary runtime directory");
    let runtime = temp.path().join("docker");
    let first_removed = temp.path().join("first-removed");
    let script = format!(
        r#"#!/bin/sh
set -eu
case "${{1:-}}" in
  info) exit 0 ;;
  container)
    case "${{2:-}}" in
      ps)
        printf '%s\n' '{{"ID":"{FIRST_ID}","State":"exited","Names":[]}}'
        printf '%s\n' '{{"ID":"{SECOND_ID}","State":"exited","Names":[]}}'
        ;;
      inspect)
        case "${{3:-}}" in
          "{FIRST_ID}"|"{SECOND_ID}")
            printf '%s\n' '[{{"Id":"'"${{3}}"'","Created":"2026-08-30T00:00:00Z","State":{{"Status":"exited"}},"Config":{{"Labels":{{"io.contextualwisdomlab.disksage.owner":"disksage","io.contextualwisdomlab.disksage.reclaimable":"true"}}}}}}]'
            ;;
          *) exit 91 ;;
        esac
        ;;
      rm)
        [ "${{3:-}}" = "{FIRST_ID}" ] || exit 92
        [ "${{4:-}}" = "{SECOND_ID}" ] || exit 93
        [ "${{5:-}}" = "" ] || exit 94
        : > "{}"
        printf '%s\n' '{FIRST_ID}'
        echo 'second candidate refused' >&2
        exit 17
        ;;
      *) exit 95 ;;
    esac
    ;;
  images|volume|network) exit 0 ;;
  *) exit 96 ;;
esac
"#,
        first_removed.display()
    );
    std::fs::write(&runtime, script).expect("write fake runtime");
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700))
        .expect("make fake runtime executable");

    let receipt_dir = tempfile::tempdir().expect("temporary receipt directory");
    std::fs::set_permissions(receipt_dir.path(), std::fs::Permissions::from_mode(0o700))
        .expect("make receipt directory private");

    let target = ContainerRuntimeTarget::new(
        ContainerRuntimeKind::DockerNative,
        PathBuf::from(&runtime),
        None,
    )
    .expect("valid Docker target");
    let plan = probe_container_orphans_with_receipt_dir(&target, receipt_dir.path());
    let containers = plan
        .categories
        .iter()
        .find(|category| category.category == OrphanCategory::Container)
        .expect("container category");
    let phrase = containers
        .approval_phrase
        .as_deref()
        .expect("candidate-bound approval phrase");

    let execution = execute_container_orphan_prune(
        &target,
        OrphanCategory::Container,
        phrase,
        "Verify that a partial exact removal remains auditable.",
        1,
        receipt_dir.path(),
    )
    .expect("post-spawn nonzero removal must still return a receipt");

    assert!(first_removed.exists(), "the fake runtime proves mutation occurred");
    assert_eq!(execution.status_code, 17, "command success stays distinct");
    assert!(
        execution.executed,
        "executed records that the exact destructive command was attempted even when completion is indeterminate"
    );
    assert!(execution.receipt_recorded, "partial mutation evidence must be persisted");
}
