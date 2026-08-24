use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::cloud_local_eviction::{
    ActiveUseEvidence, IcloudLocalEvictionPlan, IcloudLocalState, IcloudStateObservationMethod,
    ICLOUD_LOCAL_EVICTION_VERSION,
};
use disksage_lib::cloud_local_eviction_batch::{
    approve_icloud_local_eviction_batch, IcloudLocalEvictionBatchItem,
    IcloudLocalEvictionBatchPlan, ICLOUD_LOCAL_EVICTION_BATCH_VERSION,
};

fn root() -> CloudRoot {
    CloudRoot {
        id: "icloud:safety-matrix".into(),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        label: "iCloud safety matrix".into(),
        path: "/cloud".into(),
        readable: true,
        access_issue: None,
    }
}

fn notices() -> Vec<String> {
    [
        "all-selected-items-are-replanned-before-first-mutation",
        "unavailable-inputs-are-excluded-and-fingerprint-bound",
        "execution-stops-after-first-unverified-or-failed-item",
        "allocated-byte-reduction-is-not-volume-free-space-proof",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn hash_field(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&u64::try_from(value.len()).unwrap().to_le_bytes());
    hasher.update(value);
}

fn batch_fingerprint(plan: &IcloudLocalEvictionBatchPlan) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-icloud-local-eviction-batch-plan-v1\0");
    hash_field(&mut hasher, plan.provider.as_str().as_bytes());
    hash_field(&mut hasher, plan.account_scope.as_str().as_bytes());
    hash_field(&mut hasher, plan.cloud_root.as_bytes());
    hasher.update(&plan.input_count.to_le_bytes());
    hasher.update(&plan.planned_count.to_le_bytes());
    hasher.update(&plan.unavailable_count.to_le_bytes());
    hasher.update(&plan.total_logical_bytes.to_le_bytes());
    hasher.update(&plan.total_allocated_bytes.to_le_bytes());
    for item in &plan.items {
        hasher.update(&item.input_index.to_le_bytes());
        hash_field(&mut hasher, item.plan.plan_fingerprint.as_bytes());
        hasher.update(&item.plan.logical_bytes.to_le_bytes());
        hasher.update(&item.plan.allocated_bytes.to_le_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn safe_item() -> IcloudLocalEvictionPlan {
    IcloudLocalEvictionPlan {
        version: ICLOUD_LOCAL_EVICTION_VERSION,
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        cloud_root: "/cloud".into(),
        path: "/cloud/safety.bin".into(),
        logical_bytes: 64,
        allocated_bytes: 32,
        filesystem_modified_ms: 10,
        observed_at_ms: 20,
        icloud_state: IcloudLocalState {
            observation_method: IcloudStateObservationMethod::FileProviderCtlEvaluate,
            is_ubiquitous: true,
            is_uploaded: true,
            is_uploading: false,
            is_downloading: false,
            downloading_status_current: true,
            has_unresolved_conflicts: false,
            is_excluded_from_sync: false,
            is_sync_paused: Some(false),
            is_trashed: Some(false),
            allows_eviction: Some(true),
            provider_reported_bytes: Some(64),
            item_identifier_fingerprint: Some("b".repeat(64)),
        },
        active_use: ActiveUseEvidence {
            method: "coverage-fixture".into(),
            evidence_complete: true,
            active: false,
            observed_pids: Vec::new(),
            results_truncated: false,
            error: None,
        },
        plan_fingerprint: "a".repeat(64),
        eligible_after_human_approval: true,
        blockers: vec!["human-local-eviction-approval-required".into()],
        notices: Vec::new(),
    }
}

fn batch(item: IcloudLocalEvictionPlan, eligible: bool) -> IcloudLocalEvictionBatchPlan {
    let mut plan = IcloudLocalEvictionBatchPlan {
        version: ICLOUD_LOCAL_EVICTION_BATCH_VERSION,
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        cloud_root: "/cloud".into(),
        observed_at_ms: 20,
        input_count: 1,
        planned_count: 1,
        unavailable_count: 0,
        total_logical_bytes: item.logical_bytes,
        total_allocated_bytes: item.allocated_bytes,
        items: vec![IcloudLocalEvictionBatchItem {
            input_index: 0,
            plan: item,
        }],
        unavailable: Vec::new(),
        batch_fingerprint: String::new(),
        eligible_after_human_approval: eligible,
        blockers: vec![if eligible {
            "human-local-eviction-batch-approval-required".into()
        } else {
            "icloud-local-eviction-batch-item-not-eligible".into()
        }],
        notices: notices(),
    };
    plan.batch_fingerprint = batch_fingerprint(&plan);
    plan
}

fn assert_item_is_ineligible(item: IcloudLocalEvictionPlan) {
    let plan = batch(item, false);
    assert_eq!(
        approve_icloud_local_eviction_batch(
            &plan,
            &root(),
            &plan.batch_fingerprint,
            21,
            "human:operator",
            "reviewed",
        )
        .unwrap_err(),
        "icloud-local-eviction-batch-fingerprint-mismatch"
    );
}

#[test]
fn item_safety_rejects_invalid_core_and_active_use_evidence() {
    let mut cases = Vec::new();

    let mut item = safe_item();
    item.version += 1;
    cases.push(item);

    let mut item = safe_item();
    item.plan_fingerprint = "short".into();
    cases.push(item);

    let mut item = safe_item();
    item.logical_bytes = 0;
    item.icloud_state.provider_reported_bytes = Some(0);
    cases.push(item);

    let mut item = safe_item();
    item.allocated_bytes = 0;
    cases.push(item);

    let mut item = safe_item();
    item.eligible_after_human_approval = false;
    cases.push(item);

    let mut item = safe_item();
    item.blockers.push("unexpected".into());
    cases.push(item);

    let mut item = safe_item();
    item.active_use.evidence_complete = false;
    cases.push(item);

    let mut item = safe_item();
    item.active_use.active = true;
    cases.push(item);

    let mut item = safe_item();
    item.active_use.results_truncated = true;
    cases.push(item);

    for item in cases {
        assert_item_is_ineligible(item);
    }
}

#[test]
fn item_safety_rejects_incomplete_icloud_state_evidence() {
    let mut cases = Vec::new();

    let mut item = safe_item();
    item.icloud_state.is_ubiquitous = false;
    cases.push(item);

    let mut item = safe_item();
    item.icloud_state.is_uploaded = false;
    cases.push(item);

    let mut item = safe_item();
    item.icloud_state.is_uploading = true;
    cases.push(item);

    let mut item = safe_item();
    item.icloud_state.is_downloading = true;
    cases.push(item);

    let mut item = safe_item();
    item.icloud_state.downloading_status_current = false;
    cases.push(item);

    let mut item = safe_item();
    item.icloud_state.has_unresolved_conflicts = true;
    cases.push(item);

    let mut item = safe_item();
    item.icloud_state.is_excluded_from_sync = true;
    cases.push(item);

    for item in cases {
        assert_item_is_ineligible(item);
    }
}

#[test]
fn file_provider_item_safety_rejects_each_provider_attestation_gap() {
    let mut cases = Vec::new();

    let mut item = safe_item();
    item.icloud_state.is_sync_paused = None;
    cases.push(item);

    let mut item = safe_item();
    item.icloud_state.is_sync_paused = Some(true);
    cases.push(item);

    let mut item = safe_item();
    item.icloud_state.is_trashed = None;
    cases.push(item);

    let mut item = safe_item();
    item.icloud_state.is_trashed = Some(true);
    cases.push(item);

    let mut item = safe_item();
    item.icloud_state.allows_eviction = None;
    cases.push(item);

    let mut item = safe_item();
    item.icloud_state.allows_eviction = Some(false);
    cases.push(item);

    let mut item = safe_item();
    item.icloud_state.provider_reported_bytes = None;
    cases.push(item);

    let mut item = safe_item();
    item.icloud_state.provider_reported_bytes = Some(63);
    cases.push(item);

    let mut item = safe_item();
    item.icloud_state.item_identifier_fingerprint = None;
    cases.push(item);

    let mut item = safe_item();
    item.icloud_state.item_identifier_fingerprint = Some("short".into());
    cases.push(item);

    for item in cases {
        assert_item_is_ineligible(item);
    }
}

#[test]
fn foundation_item_requires_absent_file_provider_only_fields() {
    let mut foundation = safe_item();
    foundation.icloud_state.observation_method =
        IcloudStateObservationMethod::FoundationUbiquitousResourceValues;
    foundation.icloud_state.is_sync_paused = None;
    foundation.icloud_state.is_trashed = None;
    foundation.icloud_state.allows_eviction = None;
    foundation.icloud_state.provider_reported_bytes = None;
    foundation.icloud_state.item_identifier_fingerprint = None;

    let valid = batch(foundation.clone(), true);
    assert!(approve_icloud_local_eviction_batch(
        &valid,
        &root(),
        &valid.batch_fingerprint,
        21,
        "human:operator",
        "reviewed foundation evidence",
    )
    .is_ok());

    let mut cases = Vec::new();

    let mut item = foundation.clone();
    item.icloud_state.is_sync_paused = Some(false);
    cases.push(item);

    let mut item = foundation.clone();
    item.icloud_state.is_trashed = Some(false);
    cases.push(item);

    let mut item = foundation.clone();
    item.icloud_state.allows_eviction = Some(true);
    cases.push(item);

    let mut item = foundation.clone();
    item.icloud_state.provider_reported_bytes = Some(item.logical_bytes);
    cases.push(item);

    let mut item = foundation;
    item.icloud_state.item_identifier_fingerprint = Some("c".repeat(64));
    cases.push(item);

    for item in cases {
        assert_item_is_ineligible(item);
    }
}
