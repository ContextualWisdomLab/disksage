#![cfg(unix)]

use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

fn run_git(root: &Path, args: &[&str]) {
    assert!(
        Command::new("git")
            .args(args)
            .current_dir(root)
            .status()
            .expect("git command must start")
            .success(),
        "git command failed: {args:?}"
    );
}

#[test]
fn stale_clone_planner_flattens_multiple_pull_request_pages() {
    let root = std::env::temp_dir().join(format!(
        "disksage-stale-clone-pagination-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let repository = root.join("repo");
    let fake_bin = root.join("bin");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&repository).unwrap();
    fs::create_dir_all(&fake_bin).unwrap();

    run_git(&repository, &["init", "-q"]);
    run_git(&repository, &["config", "user.email", "test@example.invalid"]);
    run_git(&repository, &["config", "user.name", "DiskSage Test"]);
    run_git(&repository, &["checkout", "-qb", "feature"]);
    fs::write(repository.join("tracked.txt"), "tracked\n").unwrap();
    run_git(&repository, &["add", "tracked.txt"]);
    run_git(&repository, &["commit", "-qm", "fixture"]);
    run_git(
        &repository,
        &["remote", "add", "origin", "https://github.com/example/repo.git"],
    );

    let head = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repository)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_string();

    let first_page = r#"[{"number":1,"state":"CLOSED","headRefName":"other","headRefOid":"1111111111111111111111111111111111111111","createdAtMs":1,"url":"https://github.com/example/repo/pull/1","association_method":"exact-head"}]"#;
    let second_page = format!(
        r#"[{{"number":2,"state":"CLOSED","headRefName":"feature","headRefOid":"{head}","createdAtMs":1,"url":"https://github.com/example/repo/pull/2","association_method":"exact-head"}}]"#
    );
    let flattened = format!(
        r#"[{{"number":1,"state":"CLOSED","headRefName":"other","headRefOid":"1111111111111111111111111111111111111111","createdAtMs":1,"url":"https://github.com/example/repo/pull/1","association_method":"exact-head"}},{{"number":2,"state":"CLOSED","headRefName":"feature","headRefOid":"{head}","createdAtMs":1,"url":"https://github.com/example/repo/pull/2","association_method":"exact-head"}}]"#
    );
    let script = format!(
        "#!/bin/sh\nslurp=0\nfor arg in \"$@\"; do\n  if [ \"$arg\" = \".default_branch\" ]; then\n    printf '%s\\n' main\n    exit 0\n  fi\n  if [ \"$arg\" = \"--slurp\" ]; then slurp=1; fi\ndone\nif [ \"$slurp\" -eq 1 ]; then\n  printf '%s\\n' '{flattened}'\nelse\n  printf '%s\\n%s\\n' '{first_page}' '{second_page}'\nfi\n"
    );
    let gh = fake_bin.join("gh");
    fs::write(&gh, script).unwrap();
    let mut permissions = fs::metadata(&gh).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&gh, permissions).unwrap();

    let original_path = std::env::var_os("PATH").unwrap_or_default();
    let path = std::env::join_paths(
        std::iter::once(fake_bin.as_path()).chain(std::env::split_paths(&original_path).as_ref()),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-stale-git-clone"))
        .args([
            "--repository-root",
            repository.to_str().unwrap(),
            "--open-age-days",
            "90",
        ])
        .env("PATH", path)
        .output()
        .expect("planner CLI must start");

    assert!(
        output.status.success(),
        "planner rejected paginated PR evidence: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(plan["pull_request"]["number"], 2);
    assert_eq!(plan["pull_request"]["headRefOid"], head);

    fs::remove_dir_all(root).unwrap();
}
