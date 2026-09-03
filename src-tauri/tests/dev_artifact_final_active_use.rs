#![cfg(unix)]

use disksage_lib::dev_artifacts::{clean_artifact_exact, inspect_artifact};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

#[test]
fn exact_cleanup_rechecks_active_use_at_the_mutation_boundary() {
    let root = tempfile::tempdir().expect("fixture root must be creatable");
    let project = root.path().join("project");
    let artifact = project.join("target");
    std::fs::create_dir_all(&artifact).expect("artifact directory must be creatable");
    std::fs::write(project.join("Cargo.toml"), b"[package]\nname='fixture'\nversion='0.1.0'\n")
        .expect("project marker must be writable");
    std::fs::write(artifact.join("payload.bin"), b"generated")
        .expect("artifact payload must be writable");

    let candidate = inspect_artifact(&artifact, 1).expect("fixture must be a valid artifact");
    let mut active_process = Command::new("sh")
        .arg("-c")
        .arg("sleep 30")
        .current_dir(&artifact)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("active-use fixture process must start");
    thread::sleep(Duration::from_millis(100));

    let result = clean_artifact_exact(&candidate, &root.path().join("journal.jsonl"), 2);

    let _ = active_process.kill();
    let _ = active_process.wait();
    assert!(!result.ok, "an artifact that became active must not be moved to Trash");
    assert_eq!(result.error, "development-artifact-active-use-detected");
    assert!(artifact.exists(), "the active artifact must remain at its original path");
}

#[test]
fn exact_cleanup_detects_active_use_for_relative_artifact_paths() {
    let cwd = std::env::current_dir().expect("test working directory must be available");
    let root = tempfile::tempdir_in(&cwd).expect("relative fixture root must be creatable");
    let project = root.path().join("relative-project");
    let artifact = project.join("target");
    std::fs::create_dir_all(&artifact).expect("relative artifact directory must be creatable");
    std::fs::write(project.join("Cargo.toml"), b"[package]\nname='relative-fixture'\nversion='0.1.0'\n")
        .expect("relative project marker must be writable");
    std::fs::write(artifact.join("payload.bin"), b"generated")
        .expect("relative artifact payload must be writable");

    let relative_artifact = artifact
        .strip_prefix(&cwd)
        .expect("fixture must remain below the test working directory")
        .to_path_buf();
    let candidate =
        inspect_artifact(&relative_artifact, 1).expect("relative fixture must be a valid artifact");
    let mut active_process = Command::new("sh")
        .arg("-c")
        .arg("sleep 30")
        .current_dir(&artifact)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("relative active-use fixture process must start");
    thread::sleep(Duration::from_millis(100));

    let result = clean_artifact_exact(&candidate, &root.path().join("journal.jsonl"), 2);

    let _ = active_process.kill();
    let _ = active_process.wait();
    assert!(!result.ok, "a relative artifact that became active must not be moved to Trash");
    assert_eq!(result.error, "development-artifact-active-use-detected");
    assert!(artifact.exists(), "the active relative artifact must remain at its original path");
}
