#![cfg(unix)]

use disksage_lib::container_orphan_reclaim::{
    probe_container_orphans, ContainerRuntimeKind, ContainerRuntimeTarget, OrphanCategory,
};
use std::os::unix::fs::PermissionsExt;

const FULL_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER_ID: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn mismatch_runtime() -> (tempfile::TempDir, ContainerRuntimeTarget) {
    let temp = tempfile::tempdir().expect("temporary runtime directory");
    let runtime = temp.path().join("docker");
    let script = format!(
        r#"#!/bin/sh
set -eu
case "${{1:-}}" in
  info|container|volume|network) exit 0 ;;
  images)
    printf '%s\n' '{{"Containers":"N/A","ID":"{FULL_ID}","Repository":"<none>","Size":"72.9MB","Tag":"<none>"}}'
    ;;
  image)
    [ "${{2:-}}" = "inspect" ] || exit 96
    printf '%s\n' '{{"Id":"sha256:{OTHER_ID}","Size":72900000}}'
    ;;
  buildx) exit 0 ;;
  *) exit 98 ;;
esac
"#
    );
    std::fs::write(&runtime, script).expect("write fake runtime");
    let mut permissions = std::fs::metadata(&runtime)
        .expect("fake runtime metadata")
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&runtime, permissions).expect("make fake runtime executable");
    let target = ContainerRuntimeTarget::new(ContainerRuntimeKind::DockerNative, runtime, None)
        .expect("valid Docker target");
    (temp, target)
}

#[test]
fn concurrent_identity_mismatch_keeps_runtime_health_and_image_fail_closed_shape() {
    const WORKERS: usize = 8;
    const ITERATIONS: usize = 8;

    let workers: Vec<_> = (0..WORKERS)
        .map(|worker| {
            std::thread::spawn(move || {
                let (_temp, target) = mismatch_runtime();
                for iteration in 0..ITERATIONS {
                    let plan = probe_container_orphans(&target);
                    assert!(
                        plan.runtime.healthy,
                        "worker {worker} iteration {iteration}: runtime health failed: {:?}; plan issues: {:?}",
                        plan.runtime.detail_issue,
                        plan.issues
                    );
                    let image = plan
                        .categories
                        .iter()
                        .find(|category| category.category == OrphanCategory::Image)
                        .unwrap_or_else(|| {
                            panic!(
                                "worker {worker} iteration {iteration}: healthy runtime lost image category; plan issues: {:?}",
                                plan.issues
                            )
                        });
                    assert!(
                        !image.evidence_complete,
                        "worker {worker} iteration {iteration}: mismatched image identity became complete evidence"
                    );
                    assert_eq!(
                        image.issue.as_deref(),
                        Some("docker-image-size-identity-mismatch"),
                        "worker {worker} iteration {iteration}: unexpected image issue"
                    );
                }
            })
        })
        .collect();

    for worker in workers {
        worker.join().expect("identity-mismatch stress worker panicked");
    }
}
