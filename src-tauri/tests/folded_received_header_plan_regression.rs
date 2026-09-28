use disksage_lib::cloud::{
    plan_cloud_archive, CloudAccountScope, CloudPlanOptions, CloudProvider, CloudRoot, ContentMetadata,
    FileFact,
};

#[test]
fn folded_received_header_at_end_of_block_is_safe_through_public_plan() {
    let fixture = tempfile::tempdir().unwrap();
    let source_root = fixture.path().join("source");
    let cloud_root = fixture.path().join("cloud");
    std::fs::create_dir(&source_root).unwrap();
    std::fs::create_dir(&cloud_root).unwrap();

    let message = concat!(
        "Date: Mon, 17 Aug 2026 12:00:00 +0000\r\n",
        "Subject: Folded Received regression\r\n",
        "Received: from relay.example\r\n",
        "\tby mx.example with ESMTP\r\n",
        "\r\n",
        "body is deliberately outside the bounded metadata parser\r\n",
    );
    let message_path = source_root.join("folded-received.eml");
    std::fs::write(&message_path, message.as_bytes()).unwrap();

    let report = plan_cloud_archive(
        &[FileFact {
            path: message_path,
            bytes: message.len() as u64,
            created_ms: 1,
            modified_ms: 1,
            content_metadata: ContentMetadata::default(),
        }],
        &source_root,
        &CloudRoot {
            id: "google-drive:test".into(),
            provider: CloudProvider::GoogleDrive,
            account_scope: CloudAccountScope::Personal,
            label: "Google Drive".into(),
            path: cloud_root.to_string_lossy().into_owned(),
            readable: true,
            access_issue: None,
        },
        86_400_001,
        CloudPlanOptions {
            min_size_bytes: 1,
            min_age_days: 0,
            limit: 10,
        },
    );

    assert_eq!(report.candidates.len(), 1);
    let candidate = &report.candidates[0];
    assert_eq!(candidate.content_title.as_deref(), Some("Folded Received regression"));
    assert!(candidate.metadata_evidence.iter().any(|evidence| {
        evidence.field == "email-header-bytes-inspected"
            && evidence.source == "local:metadata-probe:bounded-rfc5322-header"
    }));
    assert!(candidate.metadata_evidence.iter().any(|evidence| {
        evidence.field == "email-body-inspected"
            && evidence.value == "false"
            && evidence.source == "local:metadata-probe:bounded-rfc5322-header"
    }));
}
