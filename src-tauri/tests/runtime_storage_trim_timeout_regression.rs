#[cfg(unix)]
#[test]
fn guest_trim_can_run_longer_than_the_probe_timeout() {
    use disksage_lib::runtime_storage::{self, RuntimeStorageKind};
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, Instant};

    let temp = tempfile::tempdir().expect("temporary runtime directory");
    let colima = temp.path().join("colima");
    fs::write(
        &colima,
        r#"#!/bin/sh
case "$*" in
  "--version")
    exit 0
    ;;
  "status --json")
    printf '%s\n' '{"status":"running"}'
    exit 0
    ;;
  "ssh -- true")
    exit 0
    ;;
  "ssh -- sudo fstrim -av")
    sleep 31
    printf '%s\n' '/: 1048576 bytes trimmed'
    exit 0
    ;;
  *)
    exit 2
    ;;
esac
"#,
    )
    .expect("write fake colima");
    let mut permissions = fs::metadata(&colima).expect("fake colima metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&colima, permissions).expect("make fake colima executable");

    let previous_path = std::env::var_os("PATH");
    let mut paths = vec![temp.path().to_path_buf()];
    if let Some(existing) = previous_path.as_deref() {
        paths.extend(std::env::split_paths(existing));
    }
    let test_path = std::env::join_paths(paths).expect("construct PATH");
    std::env::set_var("PATH", &test_path);

    let result = (|| {
        let plan = runtime_storage::inspect()
            .into_iter()
            .find(|plan| plan.runtime == RuntimeStorageKind::Colima)
            .expect("colima plan");
        assert_eq!(plan.guest_running, Some(true));
        assert_eq!(plan.guest_reachable, Some(true));
        let approval = plan
            .exact_approval_phrase
            .as_deref()
            .expect("running reachable guest has trim approval");

        let started = Instant::now();
        let execution = runtime_storage::execute_trim(
            RuntimeStorageKind::Colima,
            approval,
            "verify bounded long-running guest trim",
        )
        .expect("a normal fstrim may legitimately exceed the 30-second probe timeout");

        assert!(execution.executed);
        assert_eq!(execution.status_code, 0);
        assert!(started.elapsed() >= Duration::from_secs(30));
    })();

    match previous_path {
        Some(path) => std::env::set_var("PATH", path),
        None => std::env::remove_var("PATH"),
    }
    result
}
