#[cfg(unix)]
use disksage_lib::container_orphan_reclaim::{
    probe_runtime_health, ContainerRuntimeKind, ContainerRuntimeTarget,
};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::time::{Duration, Instant};

#[cfg(unix)]
#[test]
fn successful_runtime_probe_does_not_wait_for_descendant_holding_capture_pipes() {
    let temp = tempfile::tempdir().expect("temporary runtime directory");
    let runtime = temp.path().join("docker");
    std::fs::write(
        &runtime,
        r#"#!/bin/sh
set -eu
if [ "${1:-}" = "info" ]; then
  sleep 2 &
  exit 0
fi
exit 0
"#,
    )
    .expect("write fake runtime");
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

    let started = Instant::now();
    let health = probe_runtime_health(&target);
    let elapsed = started.elapsed();

    assert!(
        health.healthy,
        "runtime probe should succeed: {:?}",
        health.detail_issue
    );
    assert!(
        elapsed < Duration::from_secs(1),
        "successful direct child exit must not wait for a descendant holding stdout/stderr; elapsed={elapsed:?}"
    );
}
