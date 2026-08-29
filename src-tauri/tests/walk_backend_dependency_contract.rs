//! Package-boundary regression for the production filesystem walker.
//!
//! DiskSage must not resolve the unmaintained `jwalk` crate as a direct production dependency.
//! The replacement is required to use the already-resolved `walkdir` backend so locked builds do
//! not silently fall back to the deprecated walker while source migrations are incomplete.

use std::process::Command;

#[test]
fn production_manifest_resolves_walkdir_without_direct_jwalk() {
    let output = Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version=1",
            "--no-deps",
            "--locked",
            "--manifest-path",
            concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"),
        ])
        .output()
        .expect("execute cargo metadata at the real package boundary");
    assert!(
        output.status.success(),
        "locked Cargo metadata must resolve before dependency authority is evaluated: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("Cargo metadata is valid JSON");
    let package = metadata["packages"]
        .as_array()
        .and_then(|packages| {
            packages
                .iter()
                .find(|package| package["name"].as_str() == Some("disksage"))
        })
        .expect("DiskSage package exists in Cargo metadata");
    let dependencies = package["dependencies"]
        .as_array()
        .expect("DiskSage dependencies are an array");
    let names: Vec<&str> = dependencies
        .iter()
        .filter_map(|dependency| dependency["name"].as_str())
        .collect();

    assert!(
        !names.contains(&"jwalk"),
        "unmaintained jwalk must not remain a direct DiskSage production dependency"
    );
    assert!(
        names.contains(&"walkdir"),
        "DiskSage must resolve the maintained walkdir backend directly"
    );
}
