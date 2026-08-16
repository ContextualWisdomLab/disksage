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
        id: "icloud:item-integrity".into(),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        label: "iCloud item integrity".into(),
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

fn item(index: usize) -> IcloudLocalEvictionPlan {
    IcloudLocalEvictionPlan {
        version: ICLOUD_LOCAL_EVICTION_VERSION,
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        cloud_root: "/cloud".into(),
        path: format!("/cloud/file-{index}.bin"),
        logical_bytes: 100 + u64::try_from(index).unwrap(),
        allocated_bytes: 80 + u64::try_from(index).unwrap(),
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
            provider_reported_bytes: Some(100 + u64::try_from(index).unwrap()),
            item_identifier_fingerprint: Some(format!("{:064x}", index + 101)),
        },
        active_use: ActiveUseEvidence {
            method: "coverage-fixture".into(),
            evidence_complete: true,
            active: false,
            observed_pids: Vec::new(),
            results_truncated: false,
            error: None,
        },
        plan_fingerprint: format!("{:064x}", index + 1),
        eligible_after_human_approval: true,
        blockers: vec!["human-local-eviction-approval-required".into()],
        notices: Vec::new(),
    }
}

fn plan_from_items(items: Vec<IcloudLocalEvictionBatchItem>) -> IcloudLocalEvictionBatchPlan {
    let total_logical_bytes = items.iter().map(|entry| entry.plan.logical_bytes).sum();
    let total_allocated_bytes = items.iter().map(|entry| entry.plan.allocated_bytes).sum();
    let mut plan = IcloudLocalEvictionBatchPlan {
        version: ICLOUD_LOCAL_EVICTION_BATCH_VERSION,
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        cloud_root: "/cloud".into(),
        observed_at_ms: 20,
        input_count: u32::try_from(items.len()).unwrap(),
        planned_count: u32::try_from(items.len()).unwrap(),
        unavailable_count: 0,
        total_logical_bytes,
        total_allocated_bytes,
        items,
        unavailable: Vec::new(),
        batch_fingerprint: String::new(),
        eligible_after_human_approval: true,
        blockers: vec!["human-local-eviction-batch-approval-required".into()],
        notices: notices(),
    };
    plan.batch_fingerprint = batch_fingerprint(&plan);
    plan
}

fn approval_error(plan: &IcloudLocalEvictionBatchPlan) -> String {
    approve_icloud_local_eviction_batch(
        plan,
        &root(),
        &plan.batch_fingerprint,
        21,
        "human:operator",
        "reviewed",
    )
    .unwrap_err()
}

#[test]
fn batch_item_identity_rejects_out_of_range_duplicate_and_scope_drift() {
    let first = IcloudLocalEvictionBatchItem {
        input_index: 0,
        plan: item(0),
    };
    let second = IcloudLocalEvictionBatchItem {
        input_index: 1,
        plan: item(1),
    };

    let mut out_of_range = plan_from_items(vec![first.clone()]);
    out_of_range.items[0].input_index = 1;
    out_of_range.batch_fingerprint = batch_fingerprint(&out_of_range);
    assert_eq!(
        approval_error(&out_of_range),
        "icloud-local-eviction-batch-item-identity-invalid"
    );

    let mut duplicate_index = plan_from_items(vec![first.clone(), second.clone()]);
    duplicate_index.items[1].input_index = 0;
    duplicate_index.batch_fingerprint = batch_fingerprint(&duplicate_index);
    assert_eq!(
        approval_error(&duplicate_index),
        "icloud-local-eviction-batch-item-identity-invalid"
    );

    let mut duplicate_path = plan_from_items(vec![first.clone(), second.clone()]);
    duplicate_path.items[1].plan.path = duplicate_path.items[0].plan.path.clone();
    duplicate_path.batch_fingerprint = batch_fingerprint(&duplicate_path);
    assert_eq!(
        approval_error(&duplicate_path),
        "icloud-local-eviction-batch-item-identity-invalid"
    );

    let mut duplicate_fingerprint = plan_from_items(vec![first.clone(), second.clone()]);
    duplicate_fingerprint.items[1].plan.plan_fingerprint =
        duplicate_fingerprint.items[0].plan.plan_fingerprint.clone();
    duplicate_fingerprint.batch_fingerprint = batch_fingerprint(&duplicate_fingerprint);
    assert_eq!(
        approval_error(&duplicate_fingerprint),
        "icloud-local-eviction-batch-item-identity-invalid"
    );

    let mut wrong_provider = plan_from_items(vec![first.clone()]);
    wrong_provider.items[0].plan.provider = CloudProvider::GoogleDrive;
    wrong_provider.batch_fingerprint = batch_fingerprint(&wrong_provider);
    assert_eq!(
        approval_error(&wrong_provider),
        "icloud-local-eviction-batch-item-identity-invalid"
    );

    let mut wrong_scope = plan_from_items(vec![first.clone()]);
    wrong_scope.items[0].plan.account_scope = CloudAccountScope::Organization;
    wrong_scope.batch_fingerprint = batch_fingerprint(&wrong_scope);
    assert_eq!(
        approval_error(&wrong_scope),
        "icloud-local-eviction-batch-item-identity-invalid"
    );

    let mut wrong_root = plan_from_items(vec![first]);
    wrong_root.items[0].plan.cloud_root = "/other".into();
    wrong_root.batch_fingerprint = batch_fingerprint(&wrong_root);
    assert_eq!(
        approval_error(&wrong_root),
        "icloud-local-eviction-batch-item-identity-invalid"
    );
}

#[test]
fn batch_plan_integrity_rejects_totals_blockers_notices_and_fingerprint_drift() {
    let base = plan_from_items(vec![IcloudLocalEvictionBatchItem {
        input_index: 0,
        plan: item(0),
    }]);

    let mut cases = Vec::new();

    let mut plan = base.clone();
    plan.total_logical_bytes += 1;
    cases.push(plan);

    let mut plan = base.clone();
    plan.total_allocated_bytes += 1;
    cases.push(plan);

    let mut plan = base.clone();
    plan.blockers = vec!["unexpected".into()];
    cases.push(plan);

    let mut plan = base.clone();
    plan.eligible_after_human_approval = false;
    cases.push(plan);

    let mut plan = base.clone();
    plan.notices.pop();
    cases.push(plan);

    let mut plan = base.clone();
    plan.batch_fingerprint = "short".into();
    cases.push(plan);

    let mut plan = base;
    plan.batch_fingerprint = "f".repeat(64);
    cases.push(plan);

    for plan in cases {
        assert_eq!(
            approval_error(&plan),
            "icloud-local-eviction-batch-plan-integrity-mismatch"
        );
    }
}

#[test]
fn batch_plan_totals_fail_closed_on_logical_and_allocated_overflow() {
    let mut first = item(0);
    first.logical_bytes = u64::MAX;
    first.allocated_bytes = 1;
    first.icloud_state.provider_reported_bytes = Some(u64::MAX);
    let mut second = item(1);
    second.logical_bytes = 1;
    second.allocated_bytes = 1;
    second.icloud_state.provider_reported_bytes = Some(1);

    let mut logical_overflow = IcloudLocalEvictionBatchPlan {
        version: ICLOUD_LOCAL_EVICTION_BATCH_VERSION,
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        cloud_root: "/cloud".into(),
        observed_at_ms: 20,
        input_count: 2,
        planned_count: 2,
        unavailable_count: 0,
        total_logical_bytes: 0,
        total_allocated_bytes: 2,
        items: vec![
            IcloudLocalEvictionBatchItem {
                input_index: 0,
                plan: first.clone(),
            },
            IcloudLocalEvictionBatchItem {
                input_index: 1,
                plan: second.clone(),
            },
        ],
        unavailable: Vec::new(),
        batch_fingerprint: "a".repeat(64),
        eligible_after_human_approval: true,
        blockers: vec!["human-local-eviction-batch-approval-required".into()],
        notices: notices(),
    };
    logical_overflow.batch_fingerprint = batch_fingerprint(&logical_overflow);
    assert_eq!(
        approval_error(&logical_overflow),
        "icloud-local-eviction-batch-logical-total-overflow"
    );

    first.logical_bytes = 1;
    first.allocated_bytes = u64::MAX;
    first.icloud_state.provider_reported_bytes = Some(1);
    second.logical_bytes = 1;
    second.allocated_bytes = 1;
    let mut allocated_overflow = logical_overflow;
    allocated_overflow.total_logical_bytes = 2;
    allocated_overflow.total_allocated_bytes = 0;
    allocated_overflow.items[0].plan = first;
    allocated_overflow.items[1].plan = second;
    allocated_overflow.batch_fingerprint = batch_fingerprint(&allocated_overflow);
    assert_eq!(
        approval_error(&allocated_overflow),
        "icloud-local-eviction-batch-allocated-total-overflow"
    );
}
