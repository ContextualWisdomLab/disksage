#![cfg(unix)]

use disksage_lib::generated_cache_reclaim::{regeneration_contract, RegenerationContract};
use std::os::unix::fs::symlink;
use std::path::Path;

fn unique_workspace(prefix: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn create_git_workspace(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    std::fs::write(path.join(".git"), b"gitdir: /tmp/not-consulted").unwrap();
}

#[test]
fn lexical_parent_components_never_authorize_temporary_workspace_deletion() {
    let workspace = unique_workspace("disksage-parent-boundary");
    create_git_workspace(&workspace);
    std::fs::create_dir_all(workspace.join("nested")).unwrap();
    std::fs::write(workspace.join("package.json"), b"{}").unwrap();
    std::fs::write(workspace.join("package-lock.json"), b"{}").unwrap();

    let escaped_lexical_path = workspace.join("nested/../node_modules");
    assert_eq!(
        regeneration_contract(&escaped_lexical_path, Path::new("/Users/test")),
        None,
        "a path containing ParentDir must fail closed instead of inheriting workspace authority"
    );

    std::fs::remove_dir_all(workspace).unwrap();
}

#[test]
fn symlinked_workspace_ancestors_never_authorize_external_dependency_deletion() {
    let workspace = unique_workspace("disksage-symlink-boundary");
    create_git_workspace(&workspace);
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(outside.path().join("node_modules")).unwrap();
    std::fs::write(outside.path().join("package.json"), b"{}").unwrap();
    std::fs::write(outside.path().join("package-lock.json"), b"{}").unwrap();
    symlink(outside.path(), workspace.join("external-project")).unwrap();

    let candidate = workspace.join("external-project/node_modules");
    assert_eq!(
        regeneration_contract(&candidate, Path::new("/Users/test")),
        None,
        "a symlinked ancestor must not extend the validated temporary-workspace deletion boundary"
    );

    std::fs::remove_dir_all(workspace).unwrap();
}

#[test]
fn unrelated_workspace_root_javascript_lockfiles_do_not_govern_nested_projects() {
    for root_lock in ["package-lock.json", "pnpm-lock.yaml", "yarn.lock"] {
        let workspace = unique_workspace("disksage-nested-js-lock");
        create_git_workspace(&workspace);
        std::fs::write(workspace.join(root_lock), b"root-only-lock").unwrap();
        let nested = workspace.join("services/independent");
        std::fs::create_dir_all(nested.join("node_modules")).unwrap();
        std::fs::write(nested.join("package.json"), b"{}").unwrap();

        assert_eq!(
            regeneration_contract(&nested.join("node_modules"), Path::new("/Users/test")),
            None,
            "an unrelated root {root_lock} must not make a nested project reproducible"
        );

        std::fs::remove_dir_all(workspace).unwrap();
    }
}

#[test]
fn unrelated_workspace_root_uv_lock_does_not_govern_nested_project() {
    let workspace = unique_workspace("disksage-nested-uv-lock");
    create_git_workspace(&workspace);
    std::fs::write(workspace.join("uv.lock"), b"version = 1").unwrap();
    let nested = workspace.join("services/independent");
    std::fs::create_dir_all(nested.join(".venv")).unwrap();
    std::fs::write(nested.join("pyproject.toml"), b"[project]").unwrap();

    assert_eq!(
        regeneration_contract(&nested.join(".venv"), Path::new("/Users/test")),
        None,
        "an unrelated root uv.lock must not make a nested project reproducible"
    );

    std::fs::remove_dir_all(workspace).unwrap();
}

#[test]
fn project_local_lockfiles_still_authorize_only_the_dependency_subtree() {
    let workspace = unique_workspace("disksage-local-lock-positive");
    create_git_workspace(&workspace);
    let javascript = workspace.join("services/web");
    std::fs::create_dir_all(javascript.join("node_modules")).unwrap();
    std::fs::write(javascript.join("package.json"), b"{}").unwrap();
    std::fs::write(javascript.join("package-lock.json"), b"{}").unwrap();
    assert_eq!(
        regeneration_contract(&javascript.join("node_modules"), Path::new("/Users/test")),
        Some(RegenerationContract::TemporaryWorkspaceNodeModules)
    );

    let python = workspace.join("services/api");
    std::fs::create_dir_all(python.join(".venv")).unwrap();
    std::fs::write(python.join("pyproject.toml"), b"[project]").unwrap();
    std::fs::write(python.join("uv.lock"), b"version = 1").unwrap();
    assert_eq!(
        regeneration_contract(&python.join(".venv"), Path::new("/Users/test")),
        Some(RegenerationContract::TemporaryWorkspaceUvEnvironment)
    );

    assert_eq!(
        regeneration_contract(&workspace, Path::new("/Users/test")),
        None,
        "source workspace itself remains outside generated-cache deletion authority"
    );

    std::fs::remove_dir_all(workspace).unwrap();
}
