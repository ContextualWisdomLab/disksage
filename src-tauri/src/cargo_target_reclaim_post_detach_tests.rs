use crate::cargo_target_reclaim::{
    clean_cargo_target_with_active_use, ensure_target_safe_to_reclaim,
};
use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

static ACTIVE_USE_PROBES: AtomicUsize = AtomicUsize::new(0);
static POST_PREFLIGHT_HOLDER: Mutex<Option<File>> = Mutex::new(None);

fn holder_arrives_after_preflight(path: &Path) -> Result<(), String> {
    let call = ACTIVE_USE_PROBES.fetch_add(1, Ordering::SeqCst);
    if call == 0 {
        ensure_target_safe_to_reclaim(path)?;
        let holder = File::open(path.join("artifact"))
            .map_err(|error| format!("post-open-holder-open-failed:{error}"))?;
        *POST_PREFLIGHT_HOLDER
            .lock()
            .map_err(|_| "post-open-holder-lock-poisoned".to_string())? = Some(holder);
        return Ok(());
    }
    Err("cargo-target-unexpected-second-pathname-probe".into())
}

#[test]
fn post_open_holder_blocks_before_detach_and_cargo() {
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
        "[package]\nname=\"post-open-holder\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
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
    *POST_PREFLIGHT_HOLDER.lock().expect("reset holder") = None;
    let result = clean_cargo_target_with_active_use(
        &project,
        &target,
        &fake_cargo,
        holder_arrives_after_preflight,
    );
    let _ = POST_PREFLIGHT_HOLDER.lock().expect("drop holder").take();

    assert_eq!(
        ACTIVE_USE_PROBES.load(Ordering::SeqCst),
        1,
        "pathname active-use evidence is preflight only; post-open authorization must use the reviewed object"
    );
    assert_eq!(
        result.unwrap_err(),
        "cargo-target-active-holders-present",
        "a real descriptor acquired after preflight must be rejected by exact-object holder authorization before mutation"
    );
    assert!(!cargo_ran.exists(), "Cargo must not run after post-open holder refusal");
    assert!(
        artifact.is_file(),
        "post-open holder refusal must leave the reviewed target at its original path"
    );
    let detached_exists = fs::read_dir(&project)
        .expect("project listing")
        .filter_map(Result::ok)
        .any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".disksage-cargo-clean-")
        });
    assert!(
        !detached_exists,
        "holder refusal must occur before any quarantine detach is created"
    );
}