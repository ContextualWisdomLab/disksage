//! Credential-free coverage for exact human cloud-copy approval admission.
//!
//! These tests exercise authorization binding only. They perform no provider call, copy, move,
//! eviction, credential access, or other data mutation.

use disksage_lib::cloud::{
    candidate_review_fingerprint, ArchiveKind, CloudAccountScope, CloudCandidate, CloudProvider,
    CloudRoot,
};
use disksage_lib::cloud_transfer::{
    cloud_copy_approval_phrase, create_cloud_copy_approval, CloudCopyApprovalAction,
    CLOUD_COPY_APPROVAL_VERSION,
};

fn candidate() -> CloudCandidate {
    let mut candidate = CloudCandidate {
        metadata_fingerprint: "a".repeat(64),
        review_fingerprint: String::new(),
        src: "/source/report.pdf".into(),
        dst: "/cloud/report.pdf".into(),
        provider: CloudProvider::Onedrive,
        destination_account_scope: CloudAccountScope::Personal,
        kind: ArchiveKind::Document,
        bytes: 1024,
        age_days: 100,
        created_ms: 1,
        modified_ms: 2,
        production_time_ms: 1,
        production_time_source: "embedded:test:CreateDate".into(),
        production_time_confidence: "high".into(),
        source_root: "/source".into(),
        relative_path: "report.pdf".into(),
        source_context: ".".into(),
        requires_review: false,
        review_reasons: Vec::new(),
        content_title: Some("Report".into()),
        content_authors: Vec::new(),
        content_context: Vec::new(),
        duration_ms: None,
        dataset_profile: None,
        metadata_evidence: Vec::new(),
        blocked_reason: None,
    };
    candidate.review_fingerprint = candidate_review_fingerprint(&candidate);
    candidate
}

fn cloud_root() -> CloudRoot {
    CloudRoot {
        id: "onedrive:personal:test".into(),
        provider: CloudProvider::Onedrive,
        account_scope: CloudAccountScope::Personal,
        label: "OneDrive test".into(),
        path: "/cloud".into(),
        readable: true,
        access_issue: None,
    }
}

#[test]
fn exact_copy_approval_binds_action_candidate_destination_actor_and_phrase() {
    let candidate = candidate();
    let root = cloud_root();

    for action in [
        CloudCopyApprovalAction::CopyOnly,
        CloudCopyApprovalAction::AdoptExistingCopy,
    ] {
        let phrase = cloud_copy_approval_phrase(&candidate, action);
        assert!(phrase.contains(action.as_str()));
        assert!(phrase.contains(&candidate.review_fingerprint));

        let approval = create_cloud_copy_approval(
            &candidate,
            &root,
            action,
            1_000,
            "human:coverage:operator",
            "approve only this exact candidate and destination",
            &phrase,
        )
        .unwrap();

        assert_eq!(approval.version, CLOUD_COPY_APPROVAL_VERSION);
        assert_eq!(approval.action, action);
        assert_eq!(approval.candidate_fingerprint, candidate.metadata_fingerprint);
        assert_eq!(approval.review_fingerprint, candidate.review_fingerprint);
        assert_eq!(approval.provider, candidate.provider);
        assert_eq!(approval.destination_account_scope, candidate.destination_account_scope);
        assert_eq!(approval.cloud_root_id, root.id);
        assert_eq!(approval.approved_at_ms, 1_000);
        assert_eq!(approval.approved_by, "human:coverage:operator");
        assert_eq!(approval.approval_id.len(), 64);
        assert!(approval.approval_id.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }
}

#[test]
fn exact_copy_approval_rejects_stale_candidate_destination_and_confirmation_context() {
    let baseline = candidate();
    let root = cloud_root();
    let action = CloudCopyApprovalAction::CopyOnly;
    let phrase = cloud_copy_approval_phrase(&baseline, action);

    let mut stale_review = baseline.clone();
    stale_review.review_fingerprint = "0".repeat(64);
    assert_eq!(
        create_cloud_copy_approval(
            &stale_review,
            &root,
            action,
            1_000,
            "human:coverage:operator",
            "approve this exact candidate",
            &cloud_copy_approval_phrase(&stale_review, action),
        )
        .unwrap_err(),
        "cloud-copy-approval-candidate-stale"
    );

    let mut invalid_metadata = baseline.clone();
    invalid_metadata.metadata_fingerprint = "z".repeat(64);
    invalid_metadata.review_fingerprint = candidate_review_fingerprint(&invalid_metadata);
    assert_eq!(
        create_cloud_copy_approval(
            &invalid_metadata,
            &root,
            action,
            1_000,
            "human:coverage:operator",
            "approve this exact candidate",
            &cloud_copy_approval_phrase(&invalid_metadata, action),
        )
        .unwrap_err(),
        "cloud-copy-approval-candidate-stale"
    );

    let mut wrong_provider = root.clone();
    wrong_provider.provider = CloudProvider::GoogleDrive;
    assert_eq!(
        create_cloud_copy_approval(
            &baseline,
            &wrong_provider,
            action,
            1_000,
            "human:coverage:operator",
            "approve this exact candidate",
            &phrase,
        )
        .unwrap_err(),
        "cloud-copy-approval-destination-mismatch"
    );

    let mut wrong_scope = root.clone();
    wrong_scope.account_scope = CloudAccountScope::Organization;
    assert_eq!(
        create_cloud_copy_approval(
            &baseline,
            &wrong_scope,
            action,
            1_000,
            "human:coverage:operator",
            "approve this exact candidate",
            &phrase,
        )
        .unwrap_err(),
        "cloud-copy-approval-destination-mismatch"
    );

    assert_eq!(
        create_cloud_copy_approval(
            &baseline,
            &root,
            action,
            1_000,
            "human:coverage:operator",
            "approve this exact candidate",
            "승인",
        )
        .unwrap_err(),
        "cloud-copy-exact-confirmation-phrase-mismatch"
    );
}

#[test]
fn exact_copy_approval_rejects_missing_human_attribution_time_and_root_identity() {
    let candidate = candidate();
    let root = cloud_root();
    let action = CloudCopyApprovalAction::CopyOnly;
    let phrase = cloud_copy_approval_phrase(&candidate, action);

    assert_eq!(
        create_cloud_copy_approval(
            &candidate,
            &root,
            action,
            1_000,
            "",
            "approve this exact candidate",
            &phrase,
        )
        .unwrap_err(),
        "cloud-copy-approval-attribution-invalid"
    );
    assert_eq!(
        create_cloud_copy_approval(
            &candidate,
            &root,
            action,
            1_000,
            "human:coverage:operator",
            "",
            &phrase,
        )
        .unwrap_err(),
        "cloud-copy-approval-attribution-invalid"
    );
    assert_eq!(
        create_cloud_copy_approval(
            &candidate,
            &root,
            action,
            0,
            "human:coverage:operator",
            "approve this exact candidate",
            &phrase,
        )
        .unwrap_err(),
        "cloud-copy-approval-time-invalid"
    );

    let mut missing_root_id = root;
    missing_root_id.id = "   ".into();
    assert_eq!(
        create_cloud_copy_approval(
            &candidate,
            &missing_root_id,
            action,
            1_000,
            "human:coverage:operator",
            "approve this exact candidate",
            &phrase,
        )
        .unwrap_err(),
        "cloud-copy-approval-root-id-missing"
    );
}
