#![cfg(target_os = "linux")]

use disksage_lib::container_orphan_reclaim::{
    probe_container_orphans, ContainerRuntimeKind, ContainerRuntimeTarget,
};
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

fn runtime_with_escaped_info_writer() -> (tempfile::TempDir, ContainerRuntimeTarget) {
    let temp = tempfile::tempdir().expect("temporary runtime directory");
    let runtime = temp.path().join("docker");
    std::fs::write(
        &runtime,
        r#"#!/bin/sh
set -eu
case "${1:-}" in
  info)
    # Model a helper that escapes the runtime CLI's private process group but inherits its pipes.
    # The reviewed CLI has already exited; this descendant must not extend the public probe latency.
    /usr/bin/setsid /bin/sh -c 'sleep 3' &
    exit 0
    ;;
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
fn escaped_descendant_pipe_writer_does_not_extend_completed_probe() {
    let (_runtime_dir, target) = runtime_with_escaped_info_writer();
    let started = Instant::now();

    let plan = probe_container_orphans(&target);
    let elapsed = started.elapsed();

    assert!(
        plan.runtime.healthy,
        "runtime CLI itself completed successfully: {:?}",
        plan.runtime.detail_issue
    );
    assert_eq!(
        plan.categories.len(),
        5,
        "escaped helper must not alter the healthy Docker category shape: {:?}",
        plan.issues
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "a detached descendant inherited stdout/stderr and extended a completed runtime probe to {elapsed:?}"
    );
}
