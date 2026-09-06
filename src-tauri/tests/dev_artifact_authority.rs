use disksage_lib::dev_artifacts::find_artifacts;

#[test]
fn unrelated_target_layout_is_not_cleanup_authority() {
    let tmp = tempfile::tempdir().expect("create fixture root");
    let target = tmp.path().join("target");
    for child in ["deps", "build", "incremental"] {
        std::fs::create_dir_all(target.join("debug").join(child))
            .expect("create cargo-like directory name");
    }
    std::fs::write(target.join("customer-owned.sqlite"), b"business data")
        .expect("write customer-owned fixture");

    let artifacts = find_artifacts(tmp.path(), 0, u64::MAX);

    assert!(
        artifacts.is_empty(),
        "directory names alone must not authorize permanent cleanup of an unrelated target tree: {artifacts:?}"
    );
}
