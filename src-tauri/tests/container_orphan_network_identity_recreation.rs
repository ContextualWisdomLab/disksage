use disksage_lib::container_orphan_reclaim::{
    execute_container_orphan_prune, probe_container_orphans_with_receipt_dir, ContainerRuntimeKind,
    ContainerRuntimeTarget, OrphanCategory,
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::path::PathBuf;

#[cfg(unix)]
const NETWORK_ID_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
#[cfg(unix)]
const NETWORK_ID_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

#[cfg(unix)]
#[test]
fn recreated_network_name_cannot_reuse_an_approval_for_a_different_identity() {
    let temp = tempfile::tempdir().expect("temporary Docker runtime directory");
    let runtime = temp.path().join("docker");
    let network_generation = temp.path().join("network-generation");
    let deletion_marker = temp.path().join("network-deleted");

    let script = r#"#!/bin/sh
set -eu
network_generation="__NETWORK_GENERATION__"
delete_marker="__DELETE_MARKER__"
id_a="__ID_A__"
id_b="__ID_B__"
case "${1:-}" in
  info) exit 0 ;;
  container)
    [ "${2:-}" = "ps" ] || exit 91
    exit 0
    ;;
  images|volume) exit 0 ;;
  network)
    case "${2:-}" in
      ls)
        generation=$(cat "$network_generation" 2>/dev/null || printf '0')
        if [ "$generation" = "0" ]; then
          network_id="$id_a"
          printf '1' > "$network_generation"
        else
          network_id="$id_b"
        fi
        printf '{"ID":"%s","Name":"custom-net","Driver":"bridge"}\n' "$network_id"
        ;;
      inspect)
        printf '[{"Id":"%s","Name":"custom-net","Driver":"bridge","Containers":{},"Labels":{"io.contextualwisdomlab.disksage.owner":"disksage","io.contextualwisdomlab.disksage.reclaimable":"true"}}]\n' "${3:-missing}"
        ;;
      rm)
        printf '%s\n' "${3:-missing}" > "$delete_marker"
        ;;
      *) exit 92 ;;
    esac
    ;;
  *) exit 93 ;;
esac
"#
    .replace(
        "__NETWORK_GENERATION__",
        &network_generation.display().to_string(),
    )
    .replace("__DELETE_MARKER__", &deletion_marker.display().to_string())
    .replace("__ID_A__", NETWORK_ID_A)
    .replace("__ID_B__", NETWORK_ID_B);

    std::fs::write(&runtime, script).expect("write fake Docker runtime");
    let mut permissions = std::fs::metadata(&runtime)
        .expect("fake Docker metadata")
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&runtime, permissions).expect("make fake Docker runtime executable");

    let target = ContainerRuntimeTarget::new(
        ContainerRuntimeKind::DockerNative,
        PathBuf::from(&runtime),
        None,
    )
    .expect("valid Docker target");

    let receipts = tempfile::tempdir().unwrap();
    std::fs::set_permissions(receipts.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let plan = probe_container_orphans_with_receipt_dir(&target, receipts.path());
    let network = plan
        .categories
        .iter()
        .find(|category| category.category == OrphanCategory::Network)
        .expect("network category");
    assert!(network.evidence_complete, "{:?}", network.issue);
    assert_eq!(
        network
            .evidence
            .as_ref()
            .expect("network evidence")
            .candidate_records,
        1
    );
    let approval = network
        .approval_phrase
        .as_deref()
        .expect("network approval phrase")
        .to_string();

    let error = execute_container_orphan_prune(
        &target,
        OrphanCategory::Network,
        &approval,
        "operator requested exact network cleanup",
        1,
        receipts.path(),
    )
    .expect_err("a recreated network with the same name must invalidate the approval");

    assert_eq!(error, "orphan-prune-confirmation-mismatch");
    assert!(
        !deletion_marker.exists(),
        "the recreated network must not receive deletion authority"
    );
}
