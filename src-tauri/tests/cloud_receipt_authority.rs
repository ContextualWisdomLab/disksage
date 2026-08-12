#[cfg(unix)]
use disksage_lib::cloud::{
    candidate_review_fingerprint, ArchiveKind, CloudAccountScope, CloudCandidate, CloudProvider,
    CloudRoot, MetadataEvidence,
};
#[cfg(unix)]
use disksage_lib::cloud_transfer::{
    cloud_copy_approval_phrase, create_cloud_copy_approval, prepare_cloud_copy_with_approval,
    CloudCopyApprovalAction,
};

#[cfg(unix)]
fn modified_ms(metadata: &std::fs::Metadata) -> u64 {
    metadata
        .modified()
        .expect("source modified time")
        .duration_since(std::time::UNIX_EPOCH)
        .expect("source modified time after epoch")
        .as_millis() as u64
}

#[cfg(unix)]
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("current time after epoch")
        .as_millis() as u64
}

#[cfg(unix)]
#[test]
fn shared_writable_receipt_directory_fails_closed_and_rolls_back_copy() {
    use std::os::unix::fs::PermissionsExt;

    for unsafe_write_bit in [0o020, 0o002] {
        let temp = tempfile::tempdir().expect("temporary cloud-copy fixture");
        let source = temp.path().join("source/report.pdf");
        let cloud = temp.path().join("cloud");
        let destination = cloud.join("DiskSage Archive/report.pdf");
        let receipt_dir = temp.path().join("receipts");
        std::fs::create_dir_all(source.parent().expect("source parent"))
            .expect("create source parent");
        std::fs::create_dir_all(&cloud).expect("create cloud root");
        std::fs::create_dir(&receipt_dir).expect("create receipt directory");
        std::fs::write(&source, b"receipt-authority-fixture").expect("write source fixture");

        let source_metadata = std::fs::metadata(&source).expect("source metadata");
        let mut candidate = CloudCandidate {
            metadata_fingerprint: "a".repeat(64),
            review_fingerprint: String::new(),
            src: source.to_string_lossy().into_owned(),
            dst: destination.to_string_lossy().into_owned(),
            provider: CloudProvider::Icloud,
            destination_account_scope: CloudAccountScope::Personal,
            kind: ArchiveKind::Document,
            bytes: source_metadata.len(),
            age_days: 90,
            created_ms: 1,
            modified_ms: modified_ms(&source_metadata),
            production_time_ms: 2,
            production_time_source: "embedded:test:CreateDate".into(),
            production_time_confidence: "high".into(),
            source_root: source.parent().expect("source root").to_string_lossy().into_owned(),
            relative_path: "report.pdf".into(),
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
            id: "icloud:test".into(),
            provider: CloudProvider::Icloud,
            account_scope: CloudAccountScope::Personal,
            label: "iCloud Drive".into(),
            path: cloud.to_string_lossy().into_owned(),
            readable: true,
            access_issue: None,
        };
        let approved_at_ms = now_ms();
        let action = CloudCopyApprovalAction::CopyOnly;
        let phrase = cloud_copy_approval_phrase(&candidate, action);
        let approval = create_cloud_copy_approval(
            &candidate,
            &root,
            action,
            approved_at_ms,
            "human:local:test",
            "Exact source and destination reviewed for copy-only archival.",
            &phrase,
        )
        .expect("valid exact cloud-copy approval");

        let mut permissions = std::fs::metadata(&receipt_dir)
            .expect("receipt directory metadata")
            .permissions();
        permissions.set_mode(0o700 | unsafe_write_bit);
        std::fs::set_permissions(&receipt_dir, permissions)
            .expect("make receipt directory shared-writable for regression");

        let error = prepare_cloud_copy_with_approval(
            &candidate,
            &root,
            &receipt_dir,
            None,
            &approval,
        )
        .expect_err("shared-writable receipt authority directory must fail closed");

        assert_eq!(error, "receipt-directory-writable-by-others");
        assert!(source.exists(), "source must remain after receipt refusal");
        assert!(
            !destination.exists(),
            "new cloud copy must roll back when its receipt cannot be published safely"
        );
        assert_eq!(
            std::fs::read_dir(&receipt_dir)
                .expect("receipt directory remains readable")
                .count(),
            0,
            "unsafe receipt authority directory must remain empty"
        );
    }
}

#[cfg(unix)]
#[test]
fn receipt_file_is_private_from_creation_and_object_bound_for_hardening() {
    let source = std::fs::read_to_string(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/cloud_transfer.rs"),
    )
    .expect("cloud transfer source must be readable");

    assert!(
        source.contains("options.mode(0o400);"),
        "receipt authority must be owner-read-only from create_new, not only after a later chmod"
    );
    assert!(
        source.contains("file.set_permissions(permissions)"),
        "receipt hardening must remain bound to the opened receipt file"
    );
    assert!(
        !source.contains("std::fs::set_permissions(&path, permissions)"),
        "receipt hardening must not re-resolve a replaceable pathname after create_new"
    );
}
