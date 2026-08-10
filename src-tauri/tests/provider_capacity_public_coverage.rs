//! Public-contract coverage for provider-authoritative capacity evidence.
//!
//! These tests parse synthetic bounded provider responses only. They never contact a provider,
//! open a credential store, or materialize bearer tokens.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::provider_capacity::{
    assess_capacity, parse_google_drive_capacity, parse_icloud_brctl_quota,
    parse_onedrive_capacity, provider_capacity_url, root_with_verified_capacity_scope,
    unavailable_capacity, unavailable_capacity_from_error, CapacityEvidenceKind,
    CloudCapacitySnapshot, CloudCapacityState, CAPACITY_SCHEMA_VERSION,
};

fn root(provider: CloudProvider, scope: CloudAccountScope) -> CloudRoot {
    CloudRoot {
        id: "capacity-root".into(),
        provider,
        account_scope: scope,
        label: "Capacity Root".into(),
        path: "/Cloud/Capacity".into(),
        readable: true,
        access_issue: None,
    }
}

fn snapshot(provider: CloudProvider, state: CloudCapacityState) -> CloudCapacitySnapshot {
    CloudCapacitySnapshot {
        schema_version: CAPACITY_SCHEMA_VERSION,
        provider,
        account_scope: None,
        evidence_kind: CapacityEvidenceKind::ProviderApi,
        observed_at_ms: 1,
        total_bytes: Some(100),
        used_bytes: Some(50),
        remaining_bytes: Some(50),
        trashed_bytes: None,
        max_upload_size_bytes: None,
        state,
        evidence_fingerprint: None,
        unavailable_reason: None,
    }
}

#[test]
fn capacity_scope_binding_preserves_or_refines_only_verified_authority() {
    let root = root(CloudProvider::Onedrive, CloudAccountScope::Unknown);
    let no_scope = snapshot(CloudProvider::Onedrive, CloudCapacityState::Normal);
    assert_eq!(root_with_verified_capacity_scope(&root, &no_scope).unwrap(), root);

    let mut personal = no_scope.clone();
    personal.account_scope = Some(CloudAccountScope::Personal);
    let refined = root_with_verified_capacity_scope(&root, &personal).unwrap();
    assert_eq!(refined.account_scope, CloudAccountScope::Personal);

    let already_personal = root(CloudProvider::Onedrive, CloudAccountScope::Personal);
    assert_eq!(
        root_with_verified_capacity_scope(&already_personal, &personal).unwrap(),
        already_personal
    );

    let organization = root(CloudProvider::Onedrive, CloudAccountScope::Organization);
    assert_eq!(
        root_with_verified_capacity_scope(&organization, &personal).unwrap_err(),
        "cloud-capacity-account-scope-conflict"
    );

    let mut wrong_provider = personal;
    wrong_provider.provider = CloudProvider::GoogleDrive;
    assert_eq!(
        root_with_verified_capacity_scope(&root, &wrong_provider).unwrap_err(),
        "cloud-capacity-provider-mismatch"
    );
}

#[test]
fn fixed_capacity_urls_and_icloud_native_parser_cover_exact_wire_shapes() {
    assert!(provider_capacity_url(CloudProvider::Onedrive)
        .unwrap()
        .starts_with("https://graph.microsoft.com/"));
    assert!(provider_capacity_url(CloudProvider::GoogleDrive)
        .unwrap()
        .starts_with("https://www.googleapis.com/"));
    assert_eq!(
        provider_capacity_url(CloudProvider::Icloud).unwrap_err(),
        "icloud-quota-api-unavailable"
    );

    for (wire, remaining, state) in [
        (
            "1 bytes of quota remaining in personal account",
            1,
            CloudCapacityState::Available,
        ),
        (
            "2 bytes of quota remaining in personal account\r\n",
            2,
            CloudCapacityState::Available,
        ),
        (
            "0 bytes of quota remaining in personal account\n",
            0,
            CloudCapacityState::Exceeded,
        ),
    ] {
        let parsed = parse_icloud_brctl_quota(wire, 77).unwrap();
        assert_eq!(parsed.remaining_bytes, Some(remaining));
        assert_eq!(parsed.state, state);
        assert_eq!(parsed.account_scope, Some(CloudAccountScope::Personal));
        assert_eq!(parsed.evidence_fingerprint.as_ref().unwrap().len(), 64);
    }

    for invalid in [
        "",
        " 1 bytes of quota remaining in personal account",
        "01 bytes of quota remaining in personal account",
        "1,000 bytes of quota remaining in personal account",
        "x bytes of quota remaining in personal account",
        "18446744073709551616 bytes of quota remaining in personal account",
        "1 bytes of quota remaining in organization account",
        "1 bytes of quota remaining in personal account\nextra",
        "1 bytes of quota remaining in personal account\n\n",
        "é bytes of quota remaining in personal account",
    ] {
        assert!(parse_icloud_brctl_quota(invalid, 1).is_err(), "{invalid:?}");
    }
}

#[test]
fn onedrive_parser_covers_account_types_states_and_required_fields() {
    for (drive_type, scope) in [
        ("personal", CloudAccountScope::Personal),
        ("business", CloudAccountScope::Organization),
        ("documentLibrary", CloudAccountScope::Shared),
    ] {
        for state in ["normal", "nearing", "critical", "exceeded"] {
            let json = format!(
                r#"{{"id":"drive-{drive_type}-{state}","driveType":"{drive_type}","quota":{{"deleted":3,"remaining":10,"state":"{state}","total":100,"used":90}}}}"#
            );
            let parsed = parse_onedrive_capacity(&json, 9).unwrap();
            assert_eq!(parsed.account_scope, Some(scope));
            assert_eq!(parsed.remaining_bytes, Some(10));
            assert_eq!(parsed.trashed_bytes, Some(3));
            assert_eq!(parsed.evidence_fingerprint.as_ref().unwrap().len(), 64);
        }
    }

    let invalid_documents = [
        "not-json",
        r#"{"driveType":"personal","quota":{"remaining":1,"state":"normal","total":1,"used":0}}"#,
        r#"{"id":"","driveType":"personal","quota":{"remaining":1,"state":"normal","total":1,"used":0}}"#,
        r#"{"id":"drive\nsecret","driveType":"personal","quota":{"remaining":1,"state":"normal","total":1,"used":0}}"#,
        r#"{"id":"drive","driveType":"future","quota":{"remaining":1,"state":"normal","total":1,"used":0}}"#,
        r#"{"id":"drive","driveType":"personal"}"#,
        r#"{"id":"drive","driveType":"personal","quota":{"remaining":1,"state":"normal","used":0}}"#,
        r#"{"id":"drive","driveType":"personal","quota":{"remaining":1,"state":"normal","total":1}}"#,
        r#"{"id":"drive","driveType":"personal","quota":{"state":"normal","total":1,"used":0}}"#,
        r#"{"id":"drive","driveType":"personal","quota":{"remaining":1,"total":1,"used":0}}"#,
        r#"{"id":"drive","driveType":"personal","quota":{"remaining":1,"state":"unknown","total":1,"used":0}}"#,
        r#"{"id":"drive","driveType":"personal","quota":{"remaining":2,"state":"normal","total":1,"used":0}}"#,
    ];
    for json in invalid_documents {
        assert!(parse_onedrive_capacity(json, 1).is_err(), "{json}");
    }
}

#[test]
fn google_parser_covers_capacity_states_unlimited_and_numeric_failures() {
    for (limit, usage, expected) in [
        ("100", "10", CloudCapacityState::Normal),
        ("100", "95", CloudCapacityState::Nearing),
        ("100", "100", CloudCapacityState::Exceeded),
    ] {
        let json = format!(
            r#"{{"user":{{"permissionId":"user-{usage}"}},"storageQuota":{{"limit":"{limit}","usage":"{usage}","usageInDrive":"1","usageInDriveTrash":"0"}},"maxUploadSize":"50"}}"#
        );
        let parsed = parse_google_drive_capacity(&json, 11).unwrap();
        assert_eq!(parsed.state, expected);
        assert_eq!(parsed.max_upload_size_bytes, Some(50));
        assert_eq!(parsed.evidence_fingerprint.as_ref().unwrap().len(), 64);
    }

    let critical = parse_google_drive_capacity(
        r#"{"user":{"permissionId":"critical"},"storageQuota":{"limit":"1000","usage":"991","usageInDrive":"1","usageInDriveTrash":"0"},"maxUploadSize":"50"}"#,
        11,
    )
    .unwrap();
    assert_eq!(critical.state, CloudCapacityState::Critical);

    let unlimited = parse_google_drive_capacity(
        r#"{"user":{"permissionId":"unlimited"},"storageQuota":{"usage":"10","usageInDrive":"1","usageInDriveTrash":"0"},"maxUploadSize":"50"}"#,
        11,
    )
    .unwrap();
    assert_eq!(unlimited.state, CloudCapacityState::Unlimited);
    assert_eq!(unlimited.total_bytes, None);
    assert_eq!(unlimited.remaining_bytes, None);

    for json in [
        "not-json",
        r#"{"storageQuota":{"limit":"100","usage":"1","usageInDrive":"1","usageInDriveTrash":"0"},"maxUploadSize":"1"}"#,
        r#"{"user":{"permissionId":""},"storageQuota":{"limit":"100","usage":"1","usageInDrive":"1","usageInDriveTrash":"0"},"maxUploadSize":"1"}"#,
        r#"{"user":{"permissionId":"u"},"maxUploadSize":"1"}"#,
        r#"{"user":{"permissionId":"u"},"storageQuota":{"limit":"100","usageInDrive":"1","usageInDriveTrash":"0"},"maxUploadSize":"1"}"#,
        r#"{"user":{"permissionId":"u"},"storageQuota":{"limit":"100","usage":"x","usageInDrive":"1","usageInDriveTrash":"0"},"maxUploadSize":"1"}"#,
        r#"{"user":{"permissionId":"u"},"storageQuota":{"limit":"100","usage":"1","usageInDriveTrash":"0"},"maxUploadSize":"1"}"#,
        r#"{"user":{"permissionId":"u"},"storageQuota":{"limit":"100","usage":"1","usageInDrive":"x","usageInDriveTrash":"0"},"maxUploadSize":"1"}"#,
        r#"{"user":{"permissionId":"u"},"storageQuota":{"limit":"100","usage":"1","usageInDrive":"2","usageInDriveTrash":"0"},"maxUploadSize":"1"}"#,
        r#"{"user":{"permissionId":"u"},"storageQuota":{"limit":"100","usage":"1","usageInDrive":"1"},"maxUploadSize":"1"}"#,
        r#"{"user":{"permissionId":"u"},"storageQuota":{"limit":"100","usage":"1","usageInDrive":"1","usageInDriveTrash":"x"},"maxUploadSize":"1"}"#,
        r#"{"user":{"permissionId":"u"},"storageQuota":{"limit":"x","usage":"1","usageInDrive":"1","usageInDriveTrash":"0"},"maxUploadSize":"1"}"#,
        r#"{"user":{"permissionId":"u"},"storageQuota":{"limit":"100","usage":"1","usageInDrive":"1","usageInDriveTrash":"0"}}"#,
        r#"{"user":{"permissionId":"u"},"storageQuota":{"limit":"100","usage":"1","usageInDrive":"1","usageInDriveTrash":"0"},"maxUploadSize":"x"}"#,
    ] {
        assert!(parse_google_drive_capacity(json, 1).is_err(), "{json}");
    }
}

#[test]
fn capacity_assessment_and_error_redaction_cover_every_public_state_family() {
    let nearing = assess_capacity(
        snapshot(CloudProvider::Onedrive, CloudCapacityState::Nearing),
        10,
        10,
        10,
    );
    assert_eq!(nearing.can_fit, Some(true));
    assert!(nearing
        .notices
        .contains(&"cloud-capacity-provider-state-nearing".to_string()));

    let critical = assess_capacity(
        snapshot(CloudProvider::Onedrive, CloudCapacityState::Critical),
        10,
        10,
        10,
    );
    assert!(critical
        .notices
        .contains(&"cloud-capacity-provider-state-critical".to_string()));

    let mut exceeded = snapshot(CloudProvider::GoogleDrive, CloudCapacityState::Exceeded);
    exceeded.max_upload_size_bytes = Some(5);
    let blocked = assess_capacity(exceeded, u64::MAX, 6, 1);
    for code in [
        "cloud-capacity-provider-state-exceeded",
        "cloud-capacity-required-bytes-overflow",
        "cloud-max-upload-size-exceeded",
        "google-capacity-may-reflect-pooled-organization-storage",
    ] {
        assert!(
            blocked.blockers.contains(&code.to_string()) || blocked.notices.contains(&code.to_string()),
            "{code}"
        );
    }

    let mut unlimited = snapshot(CloudProvider::GoogleDrive, CloudCapacityState::Unlimited);
    unlimited.total_bytes = None;
    unlimited.remaining_bytes = None;
    let unlimited = assess_capacity(unlimited, 10, 10, 10);
    assert_eq!(unlimited.can_fit, Some(true));
    assert!(unlimited
        .notices
        .contains(&"cloud-capacity-provider-reports-unlimited".to_string()));

    let mut unverified = snapshot(CloudProvider::Onedrive, CloudCapacityState::Normal);
    unverified.remaining_bytes = None;
    let unverified = assess_capacity(unverified, 1, 1, 1);
    assert_eq!(unverified.can_fit, Some(false));
    assert!(unverified
        .blockers
        .contains(&"cloud-capacity-remaining-unverified".to_string()));

    let unavailable = assess_capacity(
        unavailable_capacity(CloudProvider::Onedrive, 1, "explicit-unavailable"),
        1,
        1,
        1,
    );
    assert_eq!(unavailable.can_fit, None);
    assert_eq!(unavailable.blockers, vec!["explicit-unavailable".to_string()]);

    for (provider, error, expected) in [
        (CloudProvider::Icloud, "icloud-native-quota-command-unavailable", "icloud-native-quota-command-unavailable"),
        (CloudProvider::Icloud, "icloud-native-quota-command-timeout", "icloud-native-quota-command-timeout"),
        (CloudProvider::Icloud, "icloud-native-quota-unsupported-platform", "icloud-native-quota-unsupported-platform"),
        (CloudProvider::Icloud, "secret provider detail", "icloud-native-quota-unavailable"),
        (CloudProvider::Onedrive, "provider-oauth-connection-missing", "provider-oauth-connection-missing"),
        (CloudProvider::Onedrive, "provider-capacity-oauth-connections-required", "provider-oauth-connection-missing"),
        (CloudProvider::Onedrive, "provider-oauth-connection-ambiguous", "provider-oauth-connection-ambiguous"),
        (CloudProvider::Onedrive, "oauth-connection-document-too-large", "provider-oauth-connection-document-invalid"),
        (CloudProvider::Onedrive, "provider-oauth-keyring-unavailable", "provider-oauth-credential-unavailable"),
        (CloudProvider::Onedrive, "provider-oauth-refresh-token-unavailable", "provider-oauth-credential-unavailable"),
        (CloudProvider::Onedrive, "provider-oauth-refresh-token-invalid", "provider-oauth-credential-unavailable"),
        (CloudProvider::Onedrive, "oauth-token-http-status:401", "provider-oauth-refresh-failed"),
        (CloudProvider::Onedrive, "oauth-access-token-invalid", "provider-oauth-refresh-failed"),
        (CloudProvider::Onedrive, "oauth-required-scope-missing", "provider-oauth-refresh-failed"),
        (CloudProvider::Onedrive, "secret transport detail", "cloud-capacity-provider-api-unavailable"),
    ] {
        let redacted = unavailable_capacity_from_error(provider, 2, error);
        assert_eq!(redacted.unavailable_reason.as_deref(), Some(expected));
        assert!(!serde_json::to_string(&redacted).unwrap().contains("secret transport detail"));
    }
}
