use std::time::Duration;

use disksage_lib::dev_artifacts::inspect_artifact_with_budget;

fn large_target(root: &std::path::Path, project_name: &str) -> std::path::PathBuf {
    let project = root.join(project_name);
    let target = project.join("target");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(project.join("Cargo.toml"), b"[package]\nname='fixture'\n").unwrap();
    for index in 0..2_048u32 {
        std::fs::write(target.join(format!("entry-{index:04}.bin")), b"generated").unwrap();
    }
    target
}

#[test]
fn multiple_large_manifests_respect_the_callers_remaining_budget() {
    let temp = tempfile::tempdir().unwrap();
    let first = large_target(temp.path(), "first-project");
    let second = large_target(temp.path(), "second-project");

    for target in [&first, &second] {
        let artifact = inspect_artifact_with_budget(target, 0, Duration::ZERO)
            .expect("marker-bound target remains a recognized artifact");
        assert!(
            !artifact.scan_complete,
            "an exhausted caller budget must fail every candidate manifest closed instead of restarting a fresh per-candidate budget"
        );
    }
}
