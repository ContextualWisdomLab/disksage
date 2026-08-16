use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::cloud_local_eviction::{
    ActiveUseEvidence, IcloudLocalEvictionPlan, IcloudLocalState, IcloudStateObservationMethod,
    ICLOUD_LOCAL_EVICTION_VERSION,
};
use disksage_lib::cloud_local_eviction_batch::{
    approve_icloud_local_eviction_batch, IcloudLocalEvictionBatchItem,
    IcloudLocalEvictionBatchPlan, IcloudLocalEvictionBatchUnavailable,
    ICLOUD_LOCAL_EVICTION_BATCH_VERSION, MAX_BATCH_ITEMS,
};

fn root() -> CloudRoot {
    CloudRoot {
        id: "icloud:coverage".into(),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        label: "iCloud coverage".into(),
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
    for unavailable in &plan.unavailable {
        hasher.update(&unavailable.input_index.to_le_bytes());
        hash_field(&mut hasher, unavailable.error_code.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn safe_item_plan() -> IcloudLocalEvictionPlan {
    IcloudLocalEvictionPlan {
        version: ICLOUD_LOCAL_EVICTION_VERSION,
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        cloud_root: "/cloud".into(),
        path: "/cloud/report.pdf".into(),
        logical_bytes: 42,
        allocated_bytes: 24,
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
            provider_reported_bytes: Some(42),
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

fn batch_with_item(item_plan: IcloudLocalEvictionPlan, eligible: bool) -> IcloudLocalEvictionBatchPlan {
    let total_logical_bytes = item_plan.logical_bytes;
    let total_allocated_bytes = item_plan.allocated_bytes;
    let mut plan = IcloudLocalEvictionBatchPlan {
        version: ICLOUD_LOCAL_EVICTION_BATCH_VERSION,
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        cloud_root: "/cloud".into(),
        observed_at_ms: 20,
        input_count: 1,
        planned_count: 1,
        unavailable_count: 0,
        total_logical_bytes,
        total_allocated_bytes,
        items: vec![IcloudLocalEvictionBatchItem {
            input_index: 0,
            plan: item_plan,
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

fn unavailable_only_plan() -> IcloudLocalEvictionBatchPlan {
    let mut plan = IcloudLocalEvictionBatchPlan {
        version: ICLOUD_LOCAL_EVICTION_BATCH_VERSION,
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        cloud_root: "/cloud".into(),
        observed_at_ms: 20,
        input_count: 1,
        planned_count: 0,
        unavailable_count: 1,
        total_logical_bytes: 0,
        total_allocated_bytes: 0,
        items: Vec::new(),
        unavailable: vec![IcloudLocalEvictionBatchUnavailable {
            input_index: 0,
            error_code: "fixture-unavailable".into(),
        }],
        batch_fingerprint: String::new(),
        eligible_after_human_approval: false,
        blockers: vec!["icloud-local-eviction-batch-has-no-planned-items".into()],
        notices: notices(),
    };
    plan.batch_fingerprint = batch_fingerprint(&plan);
    plan
}

fn approval_error(plan: &IcloudLocalEvictionBatchPlan, root: &CloudRoot) -> String {
    approve_icloud_local_eviction_batch(
        plan,
        root,
        &plan.batch_fingerprint,
        21,
        "human:operator",
        "reviewed exact batch",
    )
    .unwrap_err()
}

#[test]
fn batch_plan_shape_mismatches_fail_closed_before_approval() {
    let base = unavailable_only_plan();
    let root = root();

    let mut cases = Vec::new();

    let mut plan = base.clone();
    plan.version += 1;
    cases.push(plan);

    let mut plan = base.clone();
    plan.provider = CloudProvider::Onedrive;
    cases.push(plan);

    let mut plan = base.clone();
    plan.account_scope = CloudAccountScope::Shared;
    cases.push(plan);

    let mut plan = base.clone();
    plan.cloud_root = "/other".into();
    cases.push(plan);

    let mut plan = base.clone();
    plan.input_count = 0;
    cases.push(plan);

    let mut plan = base.clone();
    plan.input_count = u32::try_from(MAX_BATCH_ITEMS + 1).unwrap();
    cases.push(plan);

    let mut plan = base.clone();
    plan.planned_count = 1;
    cases.push(plan);

    let mut plan = base.clone();
    plan.unavailable_count = 0;
    cases.push(plan);

    let mut plan = base.clone();
    plan.input_count = 2;
    cases.push(plan);

    for plan in cases {
        assert_eq!(
            approval_error(&plan, &root),
            "icloud-local-eviction-batch-plan-shape-invalid"
        );
    }

    let mut wrong_root = root.clone();
    wrong_root.provider = CloudProvider::GoogleDrive;
    assert_eq!(
        approval_error(&base, &wrong_root),
        "icloud-local-eviction-batch-plan-shape-invalid"
    );

    let mut wrong_scope = root.clone();
    wrong_scope.account_scope = CloudAccountScope::Organization;
    assert_eq!(
        approval_error(&base, &wrong_scope),
        "icloud-local-eviction-batch-plan-shape-invalid"
    );
}

#[test]
fn unavailable_entries_require_bounded_unique_in_range_identity() {
    let root = root();

    let mut invalid_code = unavailable_only_plan();
    invalid_code.unavailable[0].error_code = "sensitive path: /private/file".into();
    invalid_code.batch_fingerprint = batch_fingerprint(&invalid_code);
    assert_eq!(
        approval_error(&invalid_code, &root),
        "icloud-local-eviction-batch-unavailable-identity-invalid"
    );

    let mut out_of_range = unavailable_only_plan();
    out_of_range.unavailable[0].input_index = 1;
    out_of_range.batch_fingerprint = batch_fingerprint(&out_of_range);
    assert_eq!(
        approval_error(&out_of_range, &root),
        "icloud-local-eviction-batch-unavailable-identity-invalid"
    );

    let mut duplicate = unavailable_only_plan();
    duplicate.input_count = 2;
    duplicate.unavailable_count = 2;
    duplicate.unavailable.push(IcloudLocalEvictionBatchUnavailable {
        input_index: 0,
        error_code: "second-unavailable".into(),
    });
    duplicate.batch_fingerprint = batch_fingerprint(&duplicate);
    assert_eq!(
        approval_error(&duplicate, &root),
        "icloud-local-eviction-batch-unavailable-identity-invalid"
    );
}

#[test]
fn eligible_batch_reaches_attributed_human_approval_boundary() {
    let root = root();
    let plan = batch_with_item(safe_item_plan(), true);

    let approval = approve_icloud_local_eviction_batch(
        &plan,
        &root,
        &plan.batch_fingerprint,
        21,
        "  human:operator  ",
        "  reviewed exact batch  ",
    )
    .unwrap();

    assert_eq!(approval.version, ICLOUD_LOCAL_EVICTION_BATCH_VERSION);
    assert_eq!(approval.batch_fingerprint, plan.batch_fingerprint);
    assert_eq!(approval.approved_at_ms, 21);
    assert_eq!(approval.approved_by, "human:operator");
    assert_eq!(approval.rationale, "reviewed exact batch");
    assert_eq!(approval.approval_id.len(), 64);

    assert_eq!(
        approve_icloud_local_eviction_batch(
            &plan,
            &root,
            &"f".repeat(64),
            21,
            "human:operator",
            "reviewed",
        )
        .unwrap_err(),
        "icloud-local-eviction-batch-fingerprint-mismatch"
    );
    assert_eq!(
        approve_icloud_local_eviction_batch(
            &plan,
            &root,
            &plan.batch_fingerprint,
            21,
            "agent:operator",
            "reviewed",
        )
        .unwrap_err(),
        "icloud-local-eviction-batch-human-attribution-required"
    );
    assert_eq!(
        approve_icloud_local_eviction_batch(
            &plan,
            &root,
            &plan.batch_fingerprint,
            21,
            "human:",
            "reviewed",
        )
        .unwrap_err(),
        "icloud-local-eviction-batch-human-attribution-required"
    );
    assert_eq!(
        approve_icloud_local_eviction_batch(
            &plan,
            &root,
            &plan.batch_fingerprint,
            21,
            "human:operator",
            "   ",
        )
        .unwrap_err(),
        "icloud-local-eviction-batch-rationale-invalid"
    );
    assert_eq!(
        approve_icloud_local_eviction_batch(
            &plan,
            &root,
            &plan.batch_fingerprint,
            21,
            "human:operator",
            &"x".repeat(1_025),
        )
        .unwrap_err(),
        "icloud-local-eviction-batch-rationale-invalid"
    );
    assert_eq!(
        approve_icloud_local_eviction_batch(
            &plan,
            &root,
            &plan.batch_fingerprint,
            19,
            "human:operator",
            "reviewed",
        )
        .unwrap_err(),
        "icloud-local-eviction-batch-approval-predates-plan"
    );
}

#[test]
fn batch_eligibility_fails_closed_for_unsafe_item_evidence_and_supports_foundation_shape() {
    let root = root();

    let mut active = safe_item_plan();
    active.active_use.active = true;
    let active_plan = batch_with_item(active, false);
    assert_eq!(
        approval_error(&active_plan, &root),
        "icloud-local-eviction-batch-fingerprint-mismatch"
    );

    let mut incomplete = safe_item_plan();
    incomplete.active_use.evidence_complete = false;
    let incomplete_plan = batch_with_item(incomplete, false);
    assert_eq!(
        approval_error(&incomplete_plan, &root),
        "icloud-local-eviction-batch-fingerprint-mismatch"
    );

    let mut foundation = safe_item_plan();
    foundation.icloud_state.observation_method =
        IcloudStateObservationMethod::FoundationUbiquitousResourceValues;
    foundation.icloud_state.is_sync_paused = None;
    foundation.icloud_state.is_trashed = None;
    foundation.icloud_state.allows_eviction = None;
    foundation.icloud_state.provider_reported_bytes = None;
    foundation.icloud_state.item_identifier_fingerprint = None;
    let foundation_plan = batch_with_item(foundation, true);
    assert!(approve_icloud_local_eviction_batch(
        &foundation_plan,
        &root,
        &foundation_plan.batch_fingerprint,
        21,
        "human:operator",
        "reviewed foundation evidence",
    )
    .is_ok());
}
