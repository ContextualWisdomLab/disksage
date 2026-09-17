#![cfg(unix)]

use disksage_lib::container_orphan_reclaim::{
    probe_container_orphans, ContainerRuntimeKind, ContainerRuntimeTarget,
};
use std::os::unix::fs::PermissionsExt;

fn fake_runtime() -> (tempfile::TempDir, ContainerRuntimeTarget) {
    let temp = tempfile::tempdir().expect("temporary runtime directory");
    let runtime = temp.path().join("docker");
    std::fs::write(
        &runtime,
        r#"#!/bin/sh
set -eu
case "${1:-}" in
  info) exit 0 ;;
  container)
    [ "${2:-}" = "ps" ] || exit 91
    exit 0
    ;;
  images) exit 0 ;;
  buildx) exit 0 ;;
  volume)
    [ "${2:-}" = "ls" ] || exit 92
    exit 0
    ;;
  network)
    [ "${2:-}" = "ls" ] || exit 93
    exit 0
    ;;
  *) exit 94 ;;
esac
"#,
    )
    .expect("write fake runtime");
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
fn concurrent_short_lived_runtime_probes_keep_health_and_category_shape() {
    const WORKERS: usize = 8;
    const ITERATIONS: usize = 16;

    // Publish one stable executable before concurrency begins. Production probes execute an
    // already-installed runtime; mixing per-worker executable creation into this stress can turn
    // Linux write/exec exclusion (ETXTBSY) into a false lifecycle failure.
    let (runtime_dir, target) = fake_runtime();
    let workers: Vec<_> = (0..WORKERS)
        .map(|worker| {
            let target = target.clone();
            std::thread::spawn(move || {
                for iteration in 0..ITERATIONS {
                    let plan = probe_container_orphans(&target);
                    assert!(
                        plan.runtime.healthy,
                        "worker {worker} iteration {iteration}: runtime health failed: {:?}",
                        plan.runtime.detail_issue
                    );
                    assert_eq!(
                        plan.categories.len(),
                        5,
                        "worker {worker} iteration {iteration}: healthy Docker runtime lost category shape: {:?}",
                        plan.issues
                    );
                    assert!(
                        plan.categories.iter().all(|category| category.evidence_complete),
                        "worker {worker} iteration {iteration}: empty runtime category became incomplete: {:?}",
                        plan.issues
                    );
                }
            })
        })
        .collect();

    for worker in workers {
        worker.join().expect("runtime stress worker panicked");
    }
    drop(runtime_dir);
}
