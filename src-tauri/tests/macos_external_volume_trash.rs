#![cfg(target_os = "macos")]

use disksage_lib::dev_artifacts::{clean_artifacts, find_artifacts};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

struct HomeGuard {
    original: Option<OsString>,
}

impl HomeGuard {
    fn replace(home: &Path) -> Self {
        let original = std::env::var_os("HOME");
        std::env::set_var("HOME", home);
        Self { original }
    }
}

impl Drop for HomeGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
    }
}

struct MountedImage {
    mount_point: PathBuf,
}

impl MountedImage {
    fn create(root: &Path) -> Self {
        let image = root.join("external-volume.dmg");
        let mount_point = root.join("external-volume");
        fs::create_dir_all(&mount_point).unwrap();

        let created = Command::new("/usr/bin/hdiutil")
            .args(["create", "-quiet", "-size", "32m", "-fs", "HFS+", "-volname"])
            .arg("DiskSageExternalTrashTest")
            .arg(&image)
            .status()
            .expect("hdiutil must be available on supported macOS test hosts");
        assert!(created.success(), "temporary external-volume image creation failed");

        let attached = Command::new("/usr/bin/hdiutil")
            .args(["attach", "-quiet", "-nobrowse", "-mountpoint"])
            .arg(&mount_point)
            .arg(&image)
            .status()
            .expect("temporary external-volume image attach must execute");
        assert!(attached.success(), "temporary external-volume image attach failed");

        Self { mount_point }
    }
}

impl Drop for MountedImage {
    fn drop(&mut self) {
        let _ = Command::new("/usr/bin/hdiutil")
            .args(["detach", "-quiet", "-force"])
            .arg(&self.mount_point)
            .status();
    }
}

fn create_node_project(root: &Path) -> PathBuf {
    let project = root.join("webapp");
    let artifact = project.join("node_modules");
    fs::create_dir_all(&artifact).unwrap();
    fs::write(project.join("package.json"), b"{}").unwrap();
    fs::write(artifact.join("payload.bin"), b"reviewed-external-volume-artifact").unwrap();
    artifact
}

#[test]
fn macos_cleanup_moves_reviewed_external_volume_artifact_to_that_volumes_trash() {
    use std::os::unix::fs::MetadataExt;

    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    fs::create_dir_all(&home).unwrap();
    assert!(
        !home.join(".Trash").exists(),
        "the regression must prove external-volume cleanup does not depend on the home Trash"
    );
    let _home_guard = HomeGuard::replace(&home);

    let mounted = MountedImage::create(tmp.path());
    let artifact = create_node_project(&mounted.mount_point);
    let home_device = fs::metadata(&home).unwrap().dev();
    let external_device = fs::metadata(&artifact).unwrap().dev();
    assert_ne!(
        external_device, home_device,
        "the fixture must exercise a real filesystem device distinct from HOME"
    );

    let now_ms = 1_888_888_888_u64;
    let planned = find_artifacts(&mounted.mount_point, 0, now_ms);
    assert_eq!(planned.len(), 1, "the external node_modules fixture must be reviewable");
    assert_eq!(Path::new(&planned[0].path), artifact.as_path());

    let journal = tmp.path().join("journal.jsonl");
    let results = clean_artifacts(&planned, &mounted.mount_point, 0, &journal, now_ms);
    assert_eq!(results.len(), 1);
    assert!(
        results[0].ok,
        "a reviewed artifact on another mounted volume must use that volume's native Trash independently of HOME/.Trash: {}",
        results[0].error.as_deref().unwrap_or("unknown cleanup failure")
    );
    assert!(
        !artifact.exists(),
        "successful cleanup must remove the reviewed pathname from the mounted volume"
    );
}
