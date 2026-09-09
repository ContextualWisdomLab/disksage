#![cfg(unix)]

use disksage_lib::container_orphan_reclaim::{
    execute_container_orphan_prune, probe_container_orphans_with_receipt_dir,
    ContainerRuntimeKind, ContainerRuntimeTarget, OrphanCategory,
};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn fake_runtime(script_body: &str) -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().expect("temporary runtime directory");
    let runtime = temp.path().join("docker");
    std::fs::write(&runtime, format!("#!/bin/sh\nset -eu\n{script_body}\n"))
        .expect("write fake runtime");
    let mut permissions = std::fs::metadata(&runtime)
        .expect("fake runtime metadata")
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&runtime, permissions).expect("make fake runtime executable");
    (temp, runtime)
}

fn docker_target(runtime: &Path) -> ContainerRuntimeTarget {
    ContainerRuntimeTarget::new(
        ContainerRuntimeKind::DockerNative,
        runtime.to_path_buf(),
        None,
    )
    .expect("valid Docker target")
}

fn private_receipt_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temporary receipt directory");
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))
        .expect("private receipt permissions");
    dir
}

#[test]
fn oversized_delete_output_is_reported_as_truncated_indeterminate_evidence() {
    const FULL_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let (_temp, runtime) = fake_runtime(&format!(
        r#"
case "${{1:-}}" in
  info) exit 0 ;;
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
      dd if=/dev/zero bs=1048576 count=2 2>/dev/null
      exit 0
    fi
    exit 98
    ;;
  images|volume|network) exit 0 ;;
  *) exit 99 ;;
esac
"#
    ));
    let target = docker_target(&runtime);
    let receipts = private_receipt_dir();
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
        "Verify that oversized mutation output remains explicit in the execution receipt.",
        1,
        receipts.path(),
    )
    .expect("mutation outcome must be returned as conservative receipt evidence");

    assert_eq!(execution.status_code, -1, "oversized output makes the mutation outcome indeterminate");
    assert!(
        execution.output_truncated,
        "discarded mutation output must be represented explicitly in the execution receipt"
    );
    assert!(
        !execution.stderr.is_empty(),
        "the conservative receipt must retain an indeterminate-outcome diagnostic"
    );
}
