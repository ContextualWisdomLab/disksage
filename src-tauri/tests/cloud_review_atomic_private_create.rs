#[cfg(unix)]
#[test]
fn cloud_review_decision_is_private_from_creation_not_only_after_path_chmod() {
    let source = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/cloud_review.rs"),
    )
    .expect("cloud review source must be readable");

    assert!(
        source.contains("options.mode(0o400);"),
        "cloud review decisions must be created with owner-read-only mode atomically so a crash before post-write chmod cannot leave a broader authority file"
    );
    assert!(
        source.contains("file.set_permissions(permissions)"),
        "post-write hardening must stay bound to the opened decision object rather than re-resolving its pathname"
    );
    assert!(
        !source.contains("std::fs::set_permissions(&path, permissions)"),
        "cloud review decision hardening must not chmod a pathname that can be replaced after create_new"
    );
}
