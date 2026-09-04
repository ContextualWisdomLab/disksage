use disksage_lib::cloud::{
    plan_cloud_archive, CloudAccountScope, CloudPlanOptions, CloudProvider, CloudRoot, ContentMetadata,
    FileFact, production_year_month, system_now_ms,
};
use disksage_lib::cloud_plan_view::normalize_native_copy_headroom_notices;

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
    let source_metadata = std::fs::metadata(&source_file).unwrap();
    let modified_ms = source_metadata
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let observed_at_ms = system_now_ms();

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
        bytes: source_metadata.len(),
        created_ms: observed_at_ms,
        modified_ms,
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

    let mut report = plan_cloud_archive(
        &[file],
        &source_root,
        &root,
        observed_at_ms,
        CloudPlanOptions {
            min_size_bytes: 1,
            min_age_days: 0,
            limit: 10,
        },
    );
    normalize_native_copy_headroom_notices(&mut report);

    assert_eq!(report.candidates.len(), 1);
    assert_eq!(
        report.candidates[0].blocked_reason.as_deref(),
        Some("local-volume-headroom-destination-parent-unsafe")
    );
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

#[cfg(unix)]
#[test]
fn one_unverified_candidate_does_not_blanket_block_candidates_with_verified_headroom() {
    use std::os::unix::fs::symlink;

    let fixture = tempfile::tempdir().unwrap();
    let source_root = fixture.path().join("source");
    let cloud_root = fixture.path().join("cloud");
    let archive_root = cloud_root.join("DiskSage Archive");
    let redirected_documents = fixture.path().join("redirected-documents");
    std::fs::create_dir(&source_root).unwrap();
    std::fs::create_dir(&cloud_root).unwrap();
    std::fs::create_dir(&archive_root).unwrap();
    std::fs::create_dir(&redirected_documents).unwrap();

    let observed_at_ms = system_now_ms();
    let (year, month) = production_year_month(observed_at_ms);
    let archive_month = archive_root
        .join(format!("{year:04}"))
        .join(format!("{month:02}"));
    std::fs::create_dir_all(&archive_month).unwrap();
    symlink(&redirected_documents, archive_month.join("documents")).unwrap();
    let mut facts = Vec::new();
    for (name, bytes) in [("report.pdf", b"report".as_slice()), ("clip.mp4", b"clip".as_slice())] {
        let path = source_root.join(name);
        std::fs::write(&path, bytes).unwrap();
        let metadata = std::fs::metadata(&path).unwrap();
        let modified_ms = metadata
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        facts.push(FileFact {
            path,
            bytes: metadata.len(),
            created_ms: observed_at_ms,
            modified_ms,
            content_metadata: ContentMetadata::default(),
        });
    }

    let root = CloudRoot {
        id: "google-drive:test".into(),
        provider: CloudProvider::GoogleDrive,
        account_scope: CloudAccountScope::Personal,
        label: "Google Drive".into(),
        path: cloud_root.to_string_lossy().into_owned(),
        readable: true,
        access_issue: None,
    };
    let mut report = plan_cloud_archive(
        &facts,
        &source_root,
        &root,
        observed_at_ms,
        CloudPlanOptions {
            min_size_bytes: 1,
            min_age_days: 0,
            limit: 10,
        },
    );
    normalize_native_copy_headroom_notices(&mut report);

    assert_eq!(report.candidates.len(), 2);
    assert!(report.candidates.iter().all(|candidate| candidate.blocked_reason.is_none()));
    assert!(
        report
            .notices
            .iter()
            .any(|notice| notice == "local-volume-headroom-partial"),
        "mixed per-candidate headroom results need a non-blocking plan diagnostic",
    );
    assert!(
        !report
            .notices
            .iter()
            .any(|notice| notice == "local-volume-headroom-unverified"),
        "one unsafe destination ancestor must not disable candidates whose own staging headroom is verified",
    );
    assert!(
        !report
            .notices
            .iter()
            .any(|notice| notice == "local-volume-headroom-insufficient"),
        "plan-wide native-copy blockers are reserved for plans where no candidate has verified headroom",
    );
    assert!(
        !redirected_documents.join("report.pdf").exists(),
        "dry-run planning must not materialize the redirected candidate",
    );
}
