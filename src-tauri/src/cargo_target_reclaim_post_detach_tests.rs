use crate::cargo_target_reclaim::clean_cargo_target_with_active_use;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

static ACTIVE_USE_PROBES: AtomicUsize = AtomicUsize::new(0);

fn holder_arrives_after_preflight(path: &Path) -> Result<(), String> {
    let call = ACTIVE_USE_PROBES.fetch_add(1, Ordering::SeqCst);
    if call == 0 {
        return Ok(());
    }
    if !path
        .to_string_lossy()
        .contains(".disksage-cargo-clean-")
    {
        return Err("cargo-target-post-detach-probe-not-detached".into());
    }
    Err("cargo-target-active-holders-present".into())
}

#[test]
fn post_detach_holder_blocks_cargo_and_rolls_back_target() {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().expect("temp root");
    let project = root.path().join("project");
    let target = project.join("target");
    let artifact = target.join("artifact");
    let cargo_ran = project.join("cargo-ran");
    let fake_cargo = project.join("fake-cargo");

    fs::create_dir_all(&target).expect("target dir");
    fs::write(
        project.join("Cargo.toml"),
        "[package]\nname=\"post-detach-holder\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
    )
    .expect("manifest");
    fs::write(&artifact, "must-survive").expect("artifact");
    fs::write(
        &fake_cargo,
        format!("#!/bin/sh\ntouch '{}'\nexit 42\n", cargo_ran.display()),
    )
    .expect("fake cargo");
    let mut permissions = fs::metadata(&fake_cargo).expect("fake cargo metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_cargo, permissions).expect("fake cargo executable");

    ACTIVE_USE_PROBES.store(0, Ordering::SeqCst);
    let result = clean_cargo_target_with_active_use(
        &project,
        &target,
        &fake_cargo,
        holder_arrives_after_preflight,
    );

    assert_eq!(
        ACTIVE_USE_PROBES.load(Ordering::SeqCst),
        2,
        "mutation authorization must repeat active-use evidence after identity-bound detach"
    );
    assert_eq!(
        result.unwrap_err(),
        "cargo-target-active-holders-present",
        "the detached-path holder must refuse mutation before Cargo starts"
    );
    assert!(!cargo_ran.exists(), "Cargo must not run after the second probe refuses");
    assert!(
        artifact.is_file(),
        "failed post-detach authorization must roll the detached target back to its original path"
    );
}
