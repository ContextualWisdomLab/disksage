#![cfg(unix)]

use disksage_lib::generated_cache_reclaim::{audit, stage_and_remove_regenerable_root};
use std::path::{Path, PathBuf};
use std::process::Command;

static TEST_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn git_workspace(prefix: &str) -> tempfile::TempDir {
    let workspace = tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in(std::env::temp_dir())
        .unwrap();
    let status = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(workspace.path())
        .status()
        .unwrap();
    assert!(status.success());
    std::fs::write(workspace.path().join("source-sentinel.txt"), b"retain source").unwrap();
    workspace
}

fn javascript_dependency(workspace: &Path) -> PathBuf {
    std::fs::write(workspace.join("package.json"), b"{}").unwrap();
    std::fs::write(workspace.join("package-lock.json"), b"{}").unwrap();
    let dependency = workspace.join("node_modules");
    std::fs::create_dir_all(dependency.join("package-a")).unwrap();
    std::fs::write(dependency.join("package-a/index.js"), b"module.exports = 1;").unwrap();
    dependency
}

fn uv_dependency(workspace: &Path) -> PathBuf {
    let project = workspace.join("services/api");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("pyproject.toml"), b"[project]\nname='api'\n").unwrap();
    std::fs::write(project.join("uv.lock"), b"version = 1\n").unwrap();
    let dependency = project.join(".venv");
    std::fs::create_dir_all(dependency.join("lib/python/site-packages/package_a")).unwrap();
    std::fs::write(
        dependency.join("lib/python/site-packages/package_a/__init__.py"),
        b"VALUE = 1\n",
    )
    .unwrap();
    dependency
}

fn assert_source_evidence_retained(workspace: &Path) {
    assert_eq!(
        std::fs::read(workspace.join("source-sentinel.txt")).unwrap(),
        b"retain source"
    );
    assert!(workspace.join(".git").is_dir());
}

fn remove_only_dependency_subtree(dependency: PathBuf, workspace: &Path) {
    let home = Path::new("/Users/test");
    let plan = audit(&dependency, home, 1).unwrap();
    assert!(plan.blockers.is_empty(), "{plan:?}");

    stage_and_remove_regenerable_root(&plan, &dependency, home, 2, u64::MAX).unwrap();

    assert!(!dependency.exists());
    assert_source_evidence_retained(workspace);
}

#[test]
fn javascript_reclaim_removes_only_the_regenerable_dependency_subtree() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let workspace = git_workspace("disksage-js-reclaim-execution-");
    let dependency = javascript_dependency(workspace.path());

    remove_only_dependency_subtree(dependency, workspace.path());

    assert!(workspace.path().join("package.json").is_file());
    assert!(workspace.path().join("package-lock.json").is_file());
}

#[test]
fn uv_reclaim_removes_only_the_regenerable_dependency_subtree() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let workspace = git_workspace("disksage-uv-reclaim-execution-");
    let dependency = uv_dependency(workspace.path());

    remove_only_dependency_subtree(dependency, workspace.path());

    assert!(workspace.path().join("services/api/pyproject.toml").is_file());
    assert!(workspace.path().join("services/api/uv.lock").is_file());
}

#[test]
fn expired_authority_after_staging_restores_the_original_dependency_path() {
    let _serial = TEST_SERIAL.lock().unwrap();
    let workspace = git_workspace("disksage-reclaim-restore-");
    let dependency = javascript_dependency(workspace.path());
    let home = Path::new("/Users/test");
    let plan = audit(&dependency, home, 1).unwrap();
    assert!(plan.blockers.is_empty(), "{plan:?}");

    assert_eq!(
        stage_and_remove_regenerable_root(&plan, &dependency, home, 2, 0).unwrap_err(),
        "generated-cache-approval-expired-before-removal"
    );

    assert!(dependency.join("package-a/index.js").is_file());
    assert_source_evidence_retained(workspace.path());
    let staging = workspace.path().join(format!(
        ".disksage-generated-cache-staging-{}",
        &plan.plan_fingerprint[..16]
    ));
    assert!(!staging.exists());
}
