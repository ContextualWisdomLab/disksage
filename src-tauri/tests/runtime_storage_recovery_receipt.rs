#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

const HELPER_ENV: &str = "DISKSAGE_RUNTIME_STORAGE_RECOVERY_HELPER";

#[test]
fn completed_restart_is_recorded_even_when_reachability_remains_unavailable() {
    if std::env::var_os(HELPER_ENV).is_some() {
        let plan = disksage_lib::runtime_storage::inspect()
            .into_iter()
            .find(|plan| plan.runtime == disksage_lib::runtime_storage::RuntimeStorageKind::Colima)
            .expect("Colima plan");
        let phrase = plan
            .recovery_approval_phrase
            .as_deref()
            .expect("unreachable running guest should offer recovery");
        let receipt = disksage_lib::runtime_storage::execute_recovery(
            disksage_lib::runtime_storage::RuntimeStorageKind::Colima,
            phrase,
            "Verify that a completed stop/start is not erased by the post-restart reachability probe.",
        )
        .expect("completed restart should return a receipt");

        assert_eq!(receipt.stop_status_code, 0);
        assert_eq!(receipt.start_status_code, 0);
        assert!(!receipt.guest_reachable_after_recovery);
        assert!(
            receipt.executed,
            "executed must report whether the approved stop/start mutation completed, not whether the later reachability observation succeeded"
        );
        return;
    }

    let temp = tempfile::tempdir().expect("temporary fake runtime directory");
    let colima = temp.path().join("colima");
    fs::write(
        &colima,
        r#"#!/bin/sh
set -eu
case "${1:-}" in
  --version) exit 0 ;;
  status)
    printf '%s\n' '{"display_name":"colima","runtime":"docker","driver":"mock"}'
    exit 0
    ;;
  ssh) exit 1 ;;
  stop) exit 0 ;;
  start) exit 0 ;;
  *) exit 98 ;;
esac
"#,
    )
    .expect("write fake colima");
    let mut permissions = fs::metadata(&colima)
        .expect("fake runtime metadata")
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&colima, permissions).expect("make fake runtime executable");

    let current_path = std::env::var_os("PATH").unwrap_or_default();
    let mut path_entries = vec![temp.path().to_path_buf()];
    path_entries.extend(std::env::split_paths(&current_path));
    let isolated_path = std::env::join_paths(path_entries).expect("isolated PATH");

    let status = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("--exact")
        .arg("completed_restart_is_recorded_even_when_reachability_remains_unavailable")
        .arg("--nocapture")
        .env(HELPER_ENV, "1")
        .env("PATH", isolated_path)
        .status()
        .expect("run isolated recovery regression");

    assert!(status.success(), "isolated production-boundary regression failed");
}
