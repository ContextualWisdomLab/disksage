#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

const HELPER_ENV: &str = "DISKSAGE_RUNTIME_STORAGE_RECOVERY_HELPER";
const START_FAIL_ENV: &str = "DISKSAGE_RUNTIME_STORAGE_START_FAIL";
const FIXED_COLIMA_CANDIDATES: &[&str] = &[
    "/opt/homebrew/bin/colima",
    "/usr/local/bin/colima",
    "/usr/bin/colima",
];

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

    if installed_colima_would_override_test_path() {
        return;
    }
    let temp = tempfile::tempdir().expect("temporary fake runtime directory");
    write_fake_colima(temp.path());
    let isolated_path = isolated_path_with(temp.path());

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

#[test]
fn successful_stop_with_failed_start_returns_partial_recovery_receipt() {
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
            "Preserve evidence that shutdown completed even when the approved restart command fails.",
        )
        .expect("a completed shutdown is a mutation and must return a structured receipt");

        assert_eq!(receipt.stop_status_code, 0);
        assert_eq!(receipt.start_status_code, 42);
        assert!(!receipt.guest_reachable_after_recovery);
        assert!(
            !receipt.executed,
            "executed remains the full stop/start completion flag while the receipt preserves the partial shutdown mutation"
        );
        return;
    }

    if installed_colima_would_override_test_path() {
        return;
    }
    let temp = tempfile::tempdir().expect("temporary fake runtime directory");
    write_fake_colima(temp.path());
    let isolated_path = isolated_path_with(temp.path());

    let status = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("--exact")
        .arg("successful_stop_with_failed_start_returns_partial_recovery_receipt")
        .arg("--nocapture")
        .env(HELPER_ENV, "1")
        .env(START_FAIL_ENV, "1")
        .env("PATH", isolated_path)
        .status()
        .expect("run isolated partial-recovery regression");

    assert!(status.success(), "partial-recovery receipt regression failed");
}

fn installed_colima_would_override_test_path() -> bool {
    FIXED_COLIMA_CANDIDATES.iter().any(|candidate| {
        fs::metadata(candidate).is_ok_and(|metadata| metadata.is_file())
    })
}

fn isolated_path_with(directory: &std::path::Path) -> std::ffi::OsString {
    let current_path = std::env::var_os("PATH").unwrap_or_default();
    let mut path_entries = vec![directory.to_path_buf()];
    path_entries.extend(std::env::split_paths(&current_path));
    std::env::join_paths(path_entries).expect("isolated PATH")
}

fn write_fake_colima(directory: &std::path::Path) {
    let colima = directory.join("colima");
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
  start)
    if [ "${DISKSAGE_RUNTIME_STORAGE_START_FAIL:-}" = "1" ]; then
      exit 42
    fi
    exit 0
    ;;
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
}
