#![cfg(all(not(coverage), windows))]

use disksage_lib::dev_artifacts::find_artifacts;
use std::fs;
use std::process::Command;

#[test]
fn top_level_junction_cannot_become_development_cleanup_authority() {
    let workspace = tempfile::tempdir().expect("temporary workspace");
    let scan_root = workspace.path().join("project");
    let external = workspace.path().join("customer-owned-external");
    let junction = scan_root.join("node_modules");
    fs::create_dir_all(&scan_root).expect("create scan root");
    fs::create_dir_all(&external).expect("create external directory");
    fs::write(scan_root.join("package.json"), b"{}\n").expect("write project marker");
    fs::write(external.join("customer-data.bin"), b"customer-owned").expect("write external data");

    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&junction)
        .arg(&external)
        .status()
        .expect("create Windows junction");
    assert!(status.success(), "Windows junction fixture must be created");

    let found = find_artifacts(&scan_root, 0, u64::MAX);

    assert!(
        found.iter().all(|artifact| artifact.path != junction.to_string_lossy()),
        "a reparse-point directory must never become destructive cleanup authority",
    );
    fs::remove_dir(&junction).expect("remove junction without touching its target");
    assert!(external.join("customer-data.bin").exists());
}
