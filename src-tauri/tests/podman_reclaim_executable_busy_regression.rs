#![cfg(target_os = "linux")]

//! Regression coverage for a transient Linux `ETXTBSY` at the Podman process boundary.
//!
//! A legitimate executable can be momentarily busy while a package/runtime updater still owns a
//! writable file descriptor. DiskSage must tolerate that narrow transient without retrying other
//! spawn failures or weakening the bounded probe contract.

use disksage_lib::podman_reclaim::{probe_podman_reclaim, DEFAULT_PODMAN_MACHINE};
use serde_json::json;
use std::fs::{self, OpenOptions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

fn install_probe_fixture(directory: &Path) -> PathBuf {
    let raw_path = directory.join("machine.raw");
    fs::write(&raw_path, b"raw").unwrap();
    fs::write(
        directory.join(format!("{DEFAULT_PODMAN_MACHINE}.json")),
        json!({ "ImagePath": { "Path": raw_path.to_string_lossy().into_owned() } }).to_string(),
    )
    .unwrap();

    let inspect = json!([{
        "ConfigDir": { "Path": directory.to_string_lossy().into_owned() },
        "Name": DEFAULT_PODMAN_MACHINE,
        "State": "running",
        "Resources": { "DiskSize": 1 }
    }])
    .to_string();
    let info = json!({
        "store": {
            "graphRoot": "/var/lib/containers/storage",
            "graphRootAllocated": 10,
            "graphRootUsed": 1,
            "imageStore": { "number": 0 },
            "containerStore": { "number": 0, "running": 0, "stopped": 0 }
        }
    })
    .to_string();
    let system_df = json!([
        { "Type": "Images", "Total": 0, "Active": 0, "RawSize": 0, "RawReclaimable": 0 },
        { "Type": "Containers", "Total": 0, "Active": 0, "RawSize": 0, "RawReclaimable": 0 },
        { "Type": "Local Volumes", "Total": 0, "Active": 0, "RawSize": 0, "RawReclaimable": 0 }
    ])
    .to_string();

    let executable = directory.join("podman");
    fs::write(
        &executable,
        format!(
            r#"#!/bin/sh
case "$*" in
  "machine inspect podman-machine-default")
    cat <<'DISKSAGE_INSPECT'
{inspect}
DISKSAGE_INSPECT
    ;;
  "machine ssh podman-machine-default -- df -B1 --output=size,used,avail /")
    printf '%s\n' '10 1 9'
    ;;
  "--connection podman-machine-default info --format json")
    cat <<'DISKSAGE_INFO'
{info}
DISKSAGE_INFO
    ;;
  "--connection podman-machine-default system df --format json")
    cat <<'DISKSAGE_SYSTEM_DF'
{system_df}
DISKSAGE_SYSTEM_DF
    ;;
  "--connection podman-machine-default images --all --format json")
    printf '%s\n' '[]'
    ;;
  *)
    exit 91
    ;;
esac
"#
        ),
    )
    .unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    executable
}

#[test]
fn transient_executable_busy_is_retried_before_probe_failure() {
    let temp = tempfile::tempdir().unwrap();
    let executable = install_probe_fixture(temp.path());

    // Linux returns ETXTBSY when exec races a writable descriptor on the executable itself.
    let busy_handle = OpenOptions::new().write(true).open(&executable).unwrap();
    let release_handle = thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        drop(busy_handle);
    });

    let plan = probe_podman_reclaim(
        &executable,
        DEFAULT_PODMAN_MACHINE,
        Duration::from_secs(2),
    );
    release_handle.join().unwrap();

    assert!(
        plan.machine.is_some(),
        "transient ETXTBSY must not erase machine evidence: {:?}",
        plan.issues
    );
    assert!(
        !plan
            .issues
            .iter()
            .any(|issue| issue.starts_with("podman-machine-inspect-spawn:")),
        "transient ETXTBSY must not become a terminal spawn issue: {:?}",
        plan.issues
    );
}
