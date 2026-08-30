#![cfg(unix)]

use disksage_lib::container_orphan_reclaim::{
    execute_container_orphan_prune, probe_container_orphans, ContainerRuntimeKind,
    ContainerRuntimeTarget, OrphanCategory,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;

#[test]
fn stopped_container_reclaim_never_forces_a_restarted_container() {
    let temp = tempfile::tempdir().expect("temporary runtime directory");
    let fake_podman = temp.path().join("podman");
    let container_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let script = format!(
        r#"#!/bin/sh
case " $* " in
  *" info "*)
    echo '{{}}'
    exit 0
    ;;
  *" container ps "*)
    printf '[{{"ID":"{container_id}","State":"exited","Names":["stale"]}}]\n'
    exit 0
    ;;
  *" container inspect {container_id} "*)
    printf '[{{"Id":"{container_id}","Created":"2026-08-30T00:00:00Z","State":{{"Status":"exited"}},"Config":{{"Labels":{{"io.contextualwisdomlab.disksage.owner":"disksage","io.contextualwisdomlab.disksage.reclaimable":"true"}}}}}}]\n'
    exit 0
    ;;
  *" images "*)
    echo '[]'
    exit 0
    ;;
  *" volume ls "*)
    echo '[]'
    exit 0
    ;;
  *" network ls "*)
    echo '[]'
    exit 0
    ;;
  *" container rm "*)
    case " $* " in
      *" --force "*)
        touch "$(dirname "$0")/force-used"
        exit 0
        ;;
      *)
        touch "$(dirname "$0")/safe-refusal"
        exit 1
        ;;
    esac
    ;;
esac
echo "unexpected fake Podman invocation: $*" >&2
exit 2
"#
    );
    fs::write(&fake_podman, script).expect("write fake Podman");
    let mut permissions = fs::metadata(&fake_podman).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&fake_podman, permissions).unwrap();

    let target = ContainerRuntimeTarget::new(
        ContainerRuntimeKind::PodmanMachine,
        fake_podman,
        Some("podman-machine-default".to_string()),
    )
    .expect("valid fake runtime target");

    let plan = probe_container_orphans(&target);
    assert!(plan.evidence_complete, "fixture must produce complete evidence: {plan:?}");
    let container_plan = plan
        .categories
        .iter()
        .find(|category| category.category == OrphanCategory::Container)
        .expect("container category");
    let approval = container_plan
        .approval_phrase
        .as_deref()
        .expect("stopped container approval phrase");

    let receipt = execute_container_orphan_prune(
        &target,
        OrphanCategory::Container,
        approval,
        "prove restarted-container refusal",
        1,
        &temp.path().join("receipts"),
    )
    .expect("non-zero exact removal is represented in the receipt");

    assert!(
        !temp.path().join("force-used").exists(),
        "a stale stopped-state observation must never authorize a force flag that can remove a container which restarted after the audit"
    );
    assert!(temp.path().join("safe-refusal").exists());
    assert!(!receipt.executed);
    assert_ne!(receipt.status_code, 0);
}
