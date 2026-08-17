//! Focused edge coverage for provider-authoritative capacity admission and assessment.
//!
//! These fixtures are local synthetic provider responses only. They exercise fail-closed scalar
//! bounds and conservative capacity decisions without opening credentials or contacting a provider.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::provider_capacity::{
    assess_capacity, parse_google_drive_capacity, parse_icloud_brctl_quota,
    parse_onedrive_capacity, root_with_verified_capacity_scope, CapacityEvidenceKind,
    CloudCapacitySnapshot, CloudCapacityState, CAPACITY_SCHEMA_VERSION,
};

fn root(scope: CloudAccountScope) -> CloudRoot {
    CloudRoot {
        id: "capacity-edge-root".into(),
        provider: CloudProvider::Onedrive,
        account_scope: scope,
        label: "Capacity edge root".into(),
        path: "/Cloud/CapacityEdge".into(),
        readable: true,
        access_issue: None,
    }
}

fn snapshot(
    provider: CloudProvider,
    state: CloudCapacityState,
    remaining_bytes: Option<u64>,
) -> CloudCapacitySnapshot {
    CloudCapacitySnapshot {
        schema_version: CAPACITY_SCHEMA_VERSION,
        provider,
        account_scope: None,
        evidence_kind: CapacityEvidenceKind::ProviderApi,
        observed_at_ms: 17,
        total_bytes: Some(100),
        used_bytes: Some(50),
        remaining_bytes,
        trashed_bytes: None,
        max_upload_size_bytes: None,
        state,
        evidence_fingerprint: None,
        unavailable_reason: None,
    }
}

#[test]
fn icloud_capacity_rejects_oversized_wire_evidence_before_parsing() {
    let oversized = "1".repeat(4 * 1024 + 1);
    assert_eq!(
        parse_icloud_brctl_quota(&oversized, 1).unwrap_err(),
        "icloud-native-quota-output-invalid"
    );
}

#[test]
fn onedrive_capacity_rejects_bounded_identity_and_missing_state_edges() {
    let overlong_id = "x".repeat(1_025);
    for json in [
        format!(
            r#"{{"id":"{overlong_id}","driveType":"personal","quota":{{"remaining":1,"state":"normal","total":1,"used":0}}}}"#
        ),
        r#"{"id":"drive","driveType":"personal","quota":{"remaining":1,"total":1,"used":0}}"#.into(),
    ] {
        assert!(parse_onedrive_capacity(&json, 1).is_err(), "{json}");
    }
}

#[test]
fn google_capacity_rejects_bounded_identity_and_numeric_overflow_edges() {
    let overlong_permission_id = "x".repeat(1_025);
    for json in [
        format!(
            r#"{{"user":{{"permissionId":"{overlong_permission_id}"}},"storageQuota":{{"limit":"100","usage":"1","usageInDrive":"1","usageInDriveTrash":"0"}},"maxUploadSize":"1"}}"#
        ),
        r#"{"user":{"permissionId":"u"},"storageQuota":{"limit":"100","usage":"18446744073709551616","usageInDrive":"1","usageInDriveTrash":"0"},"maxUploadSize":"1"}"#.into(),
        r#"{"user":{"permissionId":"u"},"storageQuota":{"limit":"18446744073709551616","usage":"1","usageInDrive":"1","usageInDriveTrash":"0"},"maxUploadSize":"1"}"#.into(),
        r#"{"user":{"permissionId":"u"},"storageQuota":{"limit":"100","usage":"1","usageInDrive":"1","usageInDriveTrash":"0"},"maxUploadSize":"18446744073709551616"}"#.into(),
    ] {
        assert!(parse_google_drive_capacity(&json, 1).is_err(), "{json}");
    }
}

#[test]
fn scope_binding_accepts_existing_shared_authority_without_reclassification() {
    let existing = root(CloudAccountScope::Shared);
    let mut verified = snapshot(CloudProvider::Onedrive, CloudCapacityState::Normal, Some(50));
    verified.account_scope = Some(CloudAccountScope::Shared);

    assert_eq!(
        root_with_verified_capacity_scope(&existing, &verified).unwrap(),
        existing
    );
}

#[test]
fn assessment_distinguishes_exact_fit_insufficient_reserve_and_missing_reason() {
    let exact_fit = assess_capacity(
        snapshot(CloudProvider::Onedrive, CloudCapacityState::Normal, Some(50)),
        40,
        40,
        10,
    );
    assert_eq!(exact_fit.required_bytes, Some(50));
    assert_eq!(exact_fit.can_fit, Some(true));
    assert!(exact_fit.blockers.is_empty());

    let insufficient = assess_capacity(
        snapshot(CloudProvider::Onedrive, CloudCapacityState::Normal, Some(49)),
        40,
        40,
        10,
    );
    assert_eq!(insufficient.can_fit, Some(false));
    assert_eq!(
        insufficient.blockers,
        vec!["cloud-capacity-insufficient-with-reserve".to_string()]
    );

    let mut unavailable_without_reason = snapshot(
        CloudProvider::GoogleDrive,
        CloudCapacityState::Unavailable,
        None,
    );
    unavailable_without_reason.evidence_kind = CapacityEvidenceKind::Unavailable;
    unavailable_without_reason.total_bytes = None;
    unavailable_without_reason.used_bytes = None;
    unavailable_without_reason.unavailable_reason = None;
    let unavailable_without_reason = assess_capacity(unavailable_without_reason, 1, 1, 1);
    assert_eq!(unavailable_without_reason.can_fit, None);
    assert_eq!(
        unavailable_without_reason.blockers,
        vec!["cloud-capacity-unavailable".to_string()]
    );
}
