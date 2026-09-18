use disksage_lib::dev_artifact_approval::{clean_artifacts_with_confirmation, review_selection};
use disksage_lib::dev_artifacts::find_artifacts;

#[test]
fn bound_cleanup_requires_the_separately_typed_backend_phrase() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("fixture");
    let target = project.join("target");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(
        project.join("Cargo.toml"),
        b"[package]\nname='fixture'\nversion='0.0.0'\n",
    )
    .unwrap();
    std::fs::write(project.join("Cargo.lock"), b"version = 4\n").unwrap();
    std::fs::write(target.join("payload.bin"), vec![7_u8; 4096]).unwrap();

    let now_ms = 1_000_u64;
    let candidates = find_artifacts(temp.path(), 0, now_ms);
    assert_eq!(candidates.len(), 1);
    let approval = review_selection(temp.path(), &candidates, now_ms).unwrap();

    let results = clean_artifacts_with_confirmation(
        &candidates,
        temp.path(),
        0,
        &temp.path().join("journal.jsonl"),
        now_ms,
        &approval,
        "not the typed phrase",
    );

    assert_eq!(results.len(), 1);
    assert!(!results[0].ok);
    assert_eq!(results[0].error, "development-artifact-confirmation-required");
    assert!(target.join("payload.bin").is_file());
}
