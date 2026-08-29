//! Redacted export of a DiskSage cloud-capacity assessment for Naruon validation.

use crate::cloud::{CloudAccountScope, CloudPlanReport, CloudProvider};
use crate::provider_capacity::{
    self, CapacityEvidenceKind, CloudCapacityAssessment, CloudCapacitySnapshot, CloudCapacityState,
};

pub const NARUON_CLOUD_CAPACITY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NaruonCloudCapacityEnvelope {
    pub schema_kind: String,
    pub schema_version: u32,
    pub decision_batch_fingerprint_version: u32,
    pub decision_batch_fingerprint: String,
    pub provider: CloudProvider,
    pub destination_account_scope: CloudAccountScope,
    pub capacity: CloudCapacityAssessment,
}

pub fn export_naruon_cloud_capacity_assessment(
    report: &CloudPlanReport,
) -> Result<NaruonCloudCapacityEnvelope, String> {
    let capacity = report
        .capacity
        .as_ref()
        .ok_or_else(|| "naruon-capacity-assessment-missing".to_string())?;
    validate_cloud_capacity_assessment(capacity)?;
    if capacity.snapshot.provider != report.cloud_root.provider {
        return Err("naruon-capacity-provider-mismatch".into());
    }
    if capacity
        .snapshot
        .account_scope
        .is_some_and(|scope| scope != report.cloud_root.account_scope)
    {
        return Err("naruon-capacity-account-scope-mismatch".into());
    }

    let largest_candidate_bytes = report
        .candidates
        .iter()
        .filter(|candidate| candidate.blocked_reason.is_none())
        .map(|candidate| candidate.bytes)
        .max()
        .unwrap_or_default();
    if capacity.requested_bytes != report.potentially_reclaimable_bytes
        || capacity.largest_candidate_bytes != largest_candidate_bytes
    {
        return Err("naruon-capacity-plan-binding-mismatch".into());
    }
    Ok(NaruonCloudCapacityEnvelope {
        schema_kind: "disksage.cloud-capacity-assessment".into(),
        schema_version: NARUON_CLOUD_CAPACITY_SCHEMA_VERSION,
        decision_batch_fingerprint_version: crate::cloud::CLOUD_DECISION_BATCH_FINGERPRINT_VERSION,
        decision_batch_fingerprint: crate::cloud::cloud_decision_batch_fingerprint(report),
        provider: report.cloud_root.provider,
        destination_account_scope: report.cloud_root.account_scope,
        capacity: capacity.clone(),
    })
}

pub fn validate_cloud_capacity_assessment(
    capacity: &CloudCapacityAssessment,
) -> Result<(), String> {
    validate_snapshot(&capacity.snapshot)?;
    let expected = provider_capacity::assess_capacity(
        capacity.snapshot.clone(),
        capacity.requested_bytes,
        capacity.largest_candidate_bytes,
        capacity.reserve_bytes,
    );
    if *capacity != expected {
        return Err("naruon-capacity-assessment-inconsistent".into());
    }
    Ok(())
}

fn is_lower_hex_64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_reason_code(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || (byte == b'-' && index > 0)
        })
}

fn validate_snapshot(snapshot: &CloudCapacitySnapshot) -> Result<(), String> {
    if snapshot.schema_version != provider_capacity::CAPACITY_SCHEMA_VERSION {
        return Err("naruon-capacity-snapshot-version-invalid".into());
    }
    if snapshot
        .evidence_fingerprint
        .as_deref()
        .is_some_and(|value| !is_lower_hex_64(value))
        || snapshot
            .unavailable_reason
            .as_deref()
            .is_some_and(|value| !is_reason_code(value))
    {
        return Err("naruon-capacity-snapshot-evidence-invalid".into());
    }

    let bytes = [
        snapshot.total_bytes,
        snapshot.used_bytes,
        snapshot.remaining_bytes,
        snapshot.trashed_bytes,
        snapshot.max_upload_size_bytes,
    ];
    match snapshot.evidence_kind {
        CapacityEvidenceKind::Unavailable => {
            if snapshot.account_scope.is_some()
                || snapshot.state != CloudCapacityState::Unavailable
                || bytes.iter().any(Option::is_some)
                || snapshot.evidence_fingerprint.is_some()
                || snapshot.unavailable_reason.is_none()
            {
                return Err("naruon-capacity-unavailable-shape-invalid".into());
            }
        }
        CapacityEvidenceKind::ProviderNativeStatus => {
            let expected_state = match snapshot.remaining_bytes {
                Some(0) => CloudCapacityState::Exceeded,
                Some(_) => CloudCapacityState::Available,
                None => return Err("naruon-capacity-native-shape-invalid".into()),
            };
            if snapshot.provider != CloudProvider::Icloud
                || snapshot.account_scope != Some(CloudAccountScope::Personal)
                || snapshot.total_bytes.is_some()
                || snapshot.used_bytes.is_some()
                || snapshot.trashed_bytes.is_some()
                || snapshot.max_upload_size_bytes.is_some()
                || snapshot.state != expected_state
                || snapshot.evidence_fingerprint.is_none()
                || snapshot.unavailable_reason.is_some()
            {
                return Err("naruon-capacity-native-shape-invalid".into());
            }
        }
        CapacityEvidenceKind::ProviderApi => {
            if snapshot.provider == CloudProvider::Icloud
                || snapshot.evidence_fingerprint.is_none()
                || snapshot.unavailable_reason.is_some()
            {
                return Err("naruon-capacity-provider-api-shape-invalid".into());
            }
            validate_provider_api_snapshot(snapshot)?;
        }
    }
    Ok(())
}

fn validate_provider_api_snapshot(snapshot: &CloudCapacitySnapshot) -> Result<(), String> {
    match snapshot.provider {
        CloudProvider::Icloud => Err("naruon-capacity-provider-api-shape-invalid".into()),
        CloudProvider::Onedrive => {
            let (Some(total), Some(_used), Some(remaining)) = (
                snapshot.total_bytes,
                snapshot.used_bytes,
                snapshot.remaining_bytes,
            ) else {
                return Err("naruon-capacity-onedrive-shape-invalid".into());
            };
            if snapshot.account_scope.is_none()
                || snapshot.account_scope == Some(CloudAccountScope::Unknown)
                || snapshot.max_upload_size_bytes.is_some()
                || remaining > total
                || !matches!(
                    snapshot.state,
                    CloudCapacityState::Normal
                        | CloudCapacityState::Nearing
                        | CloudCapacityState::Critical
                        | CloudCapacityState::Exceeded
                )
            {
                return Err("naruon-capacity-onedrive-shape-invalid".into());
            }
            Ok(())
        }
        CloudProvider::GoogleDrive => {
            let (Some(used), Some(_trashed), Some(_max_upload)) = (
                snapshot.used_bytes,
                snapshot.trashed_bytes,
                snapshot.max_upload_size_bytes,
            ) else {
                return Err("naruon-capacity-google-drive-shape-invalid".into());
            };
            if snapshot.account_scope.is_some() {
                return Err("naruon-capacity-google-drive-shape-invalid".into());
            }
            match snapshot.total_bytes {
                None if snapshot.remaining_bytes.is_none()
                    && snapshot.state == CloudCapacityState::Unlimited =>
                {
                    Ok(())
                }
                Some(total)
                    if snapshot.remaining_bytes == Some(total.saturating_sub(used))
                        && snapshot.state == google_state_from_limit(total, used) =>
                {
                    Ok(())
                }
                _ => Err("naruon-capacity-google-drive-shape-invalid".into()),
            }
        }
    }
}

fn google_state_from_limit(limit: u64, usage: u64) -> CloudCapacityState {
    if usage >= limit {
        return CloudCapacityState::Exceeded;
    }
    let remaining = limit - usage;
    if u128::from(remaining) * 100 < u128::from(limit) {
        CloudCapacityState::Critical
    } else if u128::from(remaining) * 10 < u128::from(limit) {
        CloudCapacityState::Nearing
    } else {
        CloudCapacityState::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::{CloudRoot, ExactDuplicateSummary};
    use crate::provider_capacity::{
        assess_capacity, parse_google_drive_capacity, parse_icloud_brctl_quota,
        parse_onedrive_capacity,
    };

    fn report() -> CloudPlanReport {
        let snapshot = parse_icloud_brctl_quota(
            "4338720014827 bytes of quota remaining in personal account\n",
            25,
        )
        .unwrap();
        CloudPlanReport {
            cloud_root: CloudRoot {
                id: "icloud:/private/cloud-root".into(),
                provider: CloudProvider::Icloud,
                account_scope: CloudAccountScope::Personal,
                label: "Private iCloud".into(),
                path: "/private/cloud-root".into(),
                readable: true,
                access_issue: None,
            },
            generated_at_ms: 30,
            source_selection_policy: Some(crate::cloud::CloudPlanOptions::default()),
            candidates: Vec::new(),
            candidate_bytes: 100,
            potentially_reclaimable_bytes: 100,
            exact_duplicates: ExactDuplicateSummary::default(),
            capacity: Some(assess_capacity(snapshot, 100, 0, 10)),
            local_volume: None,
            pre_copy_evidence: None,
            notices: Vec::new(),
        }
    }

    #[test]
    fn export_is_redacted_and_bound_to_the_exact_plan() {
        let report = report();
        let envelope = export_naruon_cloud_capacity_assessment(&report).unwrap();

        assert_eq!(envelope.schema_kind, "disksage.cloud-capacity-assessment");
        assert_eq!(
            envelope.schema_version,
            NARUON_CLOUD_CAPACITY_SCHEMA_VERSION
        );
        assert_eq!(
            envelope.decision_batch_fingerprint_version,
            crate::cloud::CLOUD_DECISION_BATCH_FINGERPRINT_VERSION
        );
        assert_eq!(
            envelope.decision_batch_fingerprint,
            crate::cloud::cloud_decision_batch_fingerprint(&report)
        );
        assert_eq!(envelope.provider, CloudProvider::Icloud);
        assert_eq!(
            envelope.destination_account_scope,
            CloudAccountScope::Personal
        );
        assert_eq!(envelope.capacity.can_fit, Some(true));

        let json = serde_json::to_string(&envelope).unwrap();
        assert!(!json.contains("/private/cloud-root"));
        assert!(!json.contains("Private iCloud"));
        assert!(!json.contains("icloud:/private"));
    }

    #[test]
    fn export_rejects_missing_or_forged_capacity_claims() {
        let mut missing = report();
        missing.capacity = None;
        assert_eq!(
            export_naruon_cloud_capacity_assessment(&missing).unwrap_err(),
            "naruon-capacity-assessment-missing"
        );

        let mut forged = report();
        forged.capacity.as_mut().unwrap().can_fit = Some(false);
        assert_eq!(
            export_naruon_cloud_capacity_assessment(&forged).unwrap_err(),
            "naruon-capacity-assessment-inconsistent"
        );
    }

    #[test]
    fn export_rejects_provider_or_account_scope_switching() {
        let mut wrong_provider = report();
        wrong_provider.cloud_root.provider = CloudProvider::Onedrive;
        assert_eq!(
            export_naruon_cloud_capacity_assessment(&wrong_provider).unwrap_err(),
            "naruon-capacity-provider-mismatch"
        );

        let mut wrong_scope = report();
        wrong_scope.cloud_root.account_scope = CloudAccountScope::Organization;
        assert_eq!(
            export_naruon_cloud_capacity_assessment(&wrong_scope).unwrap_err(),
            "naruon-capacity-account-scope-mismatch"
        );
    }

    #[test]
    fn export_accepts_provider_specific_api_shapes_and_rejects_forged_state() {
        let cases = [
            (
                parse_onedrive_capacity(
                    r#"{"id":"drive-id","driveType":"business","quota":{"deleted":5,"remaining":4000,"state":"normal","total":10000,"used":6000}}"#,
                    30,
                )
                .unwrap(),
                CloudAccountScope::Organization,
            ),
            (
                parse_google_drive_capacity(
                    r#"{"user":{"permissionId":"google-user-id"},"storageQuota":{"limit":"10000","usage":"9951","usageInDrive":"8000","usageInDriveTrash":"300"},"maxUploadSize":"5000"}"#,
                    40,
                )
                .unwrap(),
                CloudAccountScope::Unknown,
            ),
        ];
        for (snapshot, scope) in cases {
            let provider = snapshot.provider;
            let mut report = report();
            report.cloud_root.provider = provider;
            report.cloud_root.account_scope = scope;
            report.capacity = Some(assess_capacity(snapshot, 100, 0, 10));
            let envelope = export_naruon_cloud_capacity_assessment(&report).unwrap();
            assert_eq!(envelope.provider, provider);
            assert_eq!(envelope.destination_account_scope, scope);
        }

        let mut forged = parse_google_drive_capacity(
            r#"{"user":{"permissionId":"google-user-id"},"storageQuota":{"limit":"10000","usage":"9951","usageInDrive":"8000","usageInDriveTrash":"300"},"maxUploadSize":"5000"}"#,
            40,
        )
        .unwrap();
        forged.state = CloudCapacityState::Normal;
        let mut report = report();
        report.cloud_root.provider = CloudProvider::GoogleDrive;
        report.cloud_root.account_scope = CloudAccountScope::Unknown;
        report.capacity = Some(assess_capacity(forged, 100, 0, 10));
        assert_eq!(
            export_naruon_cloud_capacity_assessment(&report).unwrap_err(),
            "naruon-capacity-google-drive-shape-invalid"
        );
    }
}
