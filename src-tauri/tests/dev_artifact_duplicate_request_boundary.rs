#![cfg(not(coverage))]

use disksage_lib::dev_artifacts::{clean_artifacts, find_artifacts};
use std::fs;

#[test]
fn duplicate_cleanup_requests_preserve_one_result_per_request() {
    let workspace = tempfile::tempdir().expect("temporary workspace");
    let project = workspace.path().join("app");
    let artifact = project.join("node_modules");
    fs::create_dir_all(&artifact).expect("create artifact");
    fs::write(project.join("package.json"), b"{}\n").expect("write project marker");
    fs::write(artifact.join("generated.bin"), b"generated").expect("write generated fixture");

    let selected = find_artifacts(workspace.path(), 0, u64::MAX);
    assert_eq!(selected.len(), 1);
    let duplicate_requests = vec![selected[0].clone(), selected[0].clone()];

    let original = workspace.path().join("original-node-modules");
    fs::rename(&artifact, &original).expect("move selected artifact aside");
    fs::create_dir(&artifact).expect("recreate path with a different object identity");
    fs::write(artifact.join("customer-data.bin"), b"replacement").expect("write replacement data");

    let journal = workspace.path().join("cleanup-journal.jsonl");
    let results = clean_artifacts(&duplicate_requests, workspace.path(), 0, &journal, 1);

    assert_eq!(results.len(), duplicate_requests.len());
    assert!(results.iter().all(|result| !result.ok));
    assert!(results.iter().all(|result| result.path == selected[0].path));
    assert!(artifact.join("customer-data.bin").exists());
    assert!(original.join("generated.bin").exists());
    assert!(!journal.exists());
}
