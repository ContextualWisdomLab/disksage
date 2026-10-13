#![cfg(unix)]

use disksage_lib::cloud::{
    candidate_review_fingerprint, ArchiveKind, CloudAccountScope, CloudCandidate, CloudProvider,
    CloudRoot, MetadataEvidence,
};
use disksage_lib::cloud_transfer::{
    cloud_copy_approval_phrase, create_cloud_copy_approval, prepare_cloud_copy_with_approval,
    CloudCopyApprovalAction,
};
use std::os::unix::fs::PermissionsExt;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn candidate_and_root(
    temp: &tempfile::TempDir,
) -> (CloudCandidate, CloudRoot, std::path::PathBuf) {
    let source_dir = temp.path().join("source");
    let cloud_dir = temp.path().join("cloud");
    std::fs::create_dir_all(&source_dir).unwrap();
    std::fs::create_dir_all(&cloud_dir).unwrap();

    let source = source_dir.join("report.bin");
    let destination = cloud_dir.join("report.bin");
    std::fs::write(&source, b"verified source bytes").unwrap();
    let metadata = std::fs::metadata(&source).unwrap();
    let modified_ms = metadata
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    let mut candidate = CloudCandidate {
        metadata_fingerprint: "a".repeat(64),
        review_fingerprint: String::new(),
        src: source.to_string_lossy().into_owned(),
        dst: destination.to_string_lossy().into_owned(),
        provider: CloudProvider::Onedrive,
        destination_account_scope: CloudAccountScope::Personal,
        kind: ArchiveKind::Document,
        bytes: metadata.len(),
        age_days: 1,
        created_ms: modified_ms,
        modified_ms,
        production_time_ms: modified_ms,
        production_time_source: "embedded:test:CreateDate".into(),
        production_time_confidence: "high".into(),
        source_root: source_dir.to_string_lossy().into_owned(),
        relative_path: "report.bin".into(),
        source_context: ".".into(),
        requires_review: false,
        review_reasons: Vec::new(),
        content_title: Some("Report".into()),
        content_authors: Vec::new(),
        content_context: Vec::new(),
        duration_ms: None,
        dataset_profile: None,
        metadata_evidence: vec![MetadataEvidence {
            field: "production-date".into(),
            value: "2026-08-12".into(),
            source: "embedded:test:CreateDate".into(),
            confidence: "high".into(),
        }],
        blocked_reason: None,
    };
    candidate.review_fingerprint = candidate_review_fingerprint(&candidate);

    let root = CloudRoot {
        id: cloud_dir.to_string_lossy().into_owned(),
        provider: CloudProvider::Onedrive,
        account_scope: CloudAccountScope::Personal,
        label: "test".into(),
        path: cloud_dir.to_string_lossy().into_owned(),
        readable: true,
        access_issue: None,
    };
    (candidate, root, destination)
}

#[test]
fn shared_writable_receipt_directory_fails_closed_without_durable_authority() {
    for unsafe_write_bit in [0o020, 0o002] {
        let temp = tempfile::tempdir().unwrap();
        let (candidate, root, destination) = candidate_and_root(&temp);
        let receipt_dir = temp.path().join("receipts");
        std::fs::create_dir_all(&receipt_dir).unwrap();
        std::fs::set_permissions(
            &receipt_dir,
            std::fs::Permissions::from_mode(0o700 | unsafe_write_bit),
        )
        .unwrap();

        let action = CloudCopyApprovalAction::CopyOnly;
        let approval = create_cloud_copy_approval(
            &candidate,
            &root,
            action,
            now_ms(),
            "human:local:test",
            "authorize exact test cloud copy",
            &cloud_copy_approval_phrase(&candidate, action),
        )
        .unwrap();

        let error = prepare_cloud_copy_with_approval(
            &candidate,
            &root,
            &receipt_dir,
            None,
            &approval,
        )
        .expect_err("shared-writable receipt authority must fail closed");

        assert_eq!(error, "receipt-directory-writable-by-others");
        assert!(std::path::Path::new(&candidate.src).exists());
        assert!(
            !destination.exists(),
            "failed receipt publication must roll back the new copy"
        );
        assert_eq!(std::fs::read_dir(&receipt_dir).unwrap().count(), 0);
    }
}

#[test]
fn receipt_file_is_private_from_creation_and_object_bound_for_hardening() {
    let source = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/cloud_transfer.rs"),
    )
    .expect("cloud transfer source must be readable");

    assert!(
        source.contains("options.mode(0o400);"),
        "cloud copy receipts must be owner-read-only from create_new so a crash cannot leave broader authority"
    );
    assert!(
        source.contains("file.set_permissions(permissions)"),
        "post-write receipt hardening must remain bound to the opened file object"
    );
    assert!(
        !source.contains("std::fs::set_permissions(&path, permissions)"),
        "receipt hardening must not re-resolve a replaceable pathname after create_new"
    );
}
