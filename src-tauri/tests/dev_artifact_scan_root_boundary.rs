#![cfg(not(coverage))]

use disksage_lib::dev_artifacts::find_artifacts;
use std::fs;

#[test]
fn scan_root_is_reported_when_it_is_a_marker_adjacent_generated_artifact() {
    let workspace = tempfile::tempdir().expect("temporary workspace");
    let project = workspace.path().join("app");
    let artifact_root = project.join("node_modules");
    fs::create_dir_all(&artifact_root).expect("create generated artifact root");
    fs::write(project.join("package.json"), b"{}\n").expect("write project marker");
    fs::write(artifact_root.join("payload.bin"), b"generated").expect("write generated payload");

    let found = find_artifacts(&artifact_root, 0, u64::MAX);

    assert_eq!(found.len(), 1, "the requested scan root must not disappear from inventory");
    assert_eq!(found[0].kind, "node_modules");
    assert_eq!(found[0].path, artifact_root.to_string_lossy());
}
