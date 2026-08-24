use disksage_lib::cloud::{
    plan_cloud_archive, CloudAccountScope, CloudPlanOptions, CloudProvider, CloudRoot, ContentMetadata,
    FileFact,
};

#[cfg(unix)]
#[test]
fn cloud_plan_preview_uses_destination_ancestor_authority_at_runtime() {
    use std::os::unix::fs::symlink;

    let fixture = tempfile::tempdir().unwrap();
    let source_root = fixture.path().join("source");
    let cloud_root = fixture.path().join("cloud");
    let redirected_archive = fixture.path().join("redirected-archive");
    std::fs::create_dir(&source_root).unwrap();
    std::fs::create_dir(&cloud_root).unwrap();
    std::fs::create_dir(&redirected_archive).unwrap();

    let source_file = source_root.join("report.pdf");
    std::fs::write(&source_file, b"report").unwrap();

    // The final candidate itself does not exist, so ordinary destination-exists checks do not
    // block it. The nearest existing staging ancestor is nevertheless a symlink and must not
    // become capacity authority for a native-copy preview.
    symlink(
        &redirected_archive,
        cloud_root.join("DiskSage Archive"),
    )
    .unwrap();

    let file = FileFact {
        path: source_file,
        bytes: 6,
        created_ms: 1,
        modified_ms: 1,
        content_metadata: ContentMetadata::default(),
    };
    let root = CloudRoot {
        id: "google-drive:test".into(),
        provider: CloudProvider::GoogleDrive,
        account_scope: CloudAccountScope::Personal,
        label: "Google Drive".into(),
        path: cloud_root.to_string_lossy().into_owned(),
        readable: true,
        access_issue: None,
    };

    let report = plan_cloud_archive(
        &[file],
        &source_root,
        &root,
        86_400_001,
        CloudPlanOptions {
            min_size_bytes: 1,
            min_age_days: 0,
            limit: 10,
        },
    );

    assert_eq!(report.candidates.len(), 1);
    assert_eq!(report.candidates[0].blocked_reason, None);
    assert!(
        report
            .notices
            .iter()
            .any(|notice| notice == "local-volume-headroom-unverified"),
        "preview must reject an unsafe destination/staging capacity authority even when the source volume is healthy",
    );
    assert!(
        report.local_volume.is_some(),
        "source-volume pressure remains independent diagnostics rather than staging authority",
    );
    assert!(
        !redirected_archive.join("documents").join("report.pdf").exists(),
        "dry-run planning must not materialize the redirected destination",
    );
}
