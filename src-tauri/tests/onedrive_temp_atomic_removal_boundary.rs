use disksage_lib::onedrive_temp_reclaim::execute;

#[test]
fn onedrive_temp_execute_refuses_non_atomic_path_deletion_before_provider_or_filesystem_work() {
    let home = tempfile::tempdir().expect("temporary home should be creatable");
    let error = execute(
        home.path(),
        &"0".repeat(64),
        "historical approval",
        1,
    )
    .expect_err("OneDrive temp deletion must remain unavailable until identity and removal are atomic");

    assert_eq!(error, "onedrive-temp-atomic-removal-unavailable");
}
