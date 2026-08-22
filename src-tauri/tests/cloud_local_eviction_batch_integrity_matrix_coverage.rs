//! Exact-head coverage for iCloud local-eviction batch integrity branches.
//!
//! These tests stay at the public approval boundary. They deliberately construct evidence records
//! that are structurally plausible but internally contradictory, proving validation fails closed
//! before any execution or filesystem mutation can occur.

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
        id: "icloud:integrity-matrix".into(),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        label: "iCloud integrity matrix".into(),
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

fn safe_item(path: &str, fingerprint_nibble: char, logical: u64, allocated: u64) -> IcloudLocalEvictionPlan {
    IcloudLocalEvictionPlan {
        version: ICLOUD_LOCAL_EVICTION_VERSION,
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        cloud_root: "/cloud".into(),
        path: path.into(),
        logical_bytes: logical,
        allocated_bytes: allocated,
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
            provider_reported_bytes: Some(logical),
            item_identifier_fingerprint: Some("d".repeat(64)),
        },
        active_use: ActiveUseEvidence {
            method: "coverage-fixture".into(),
            evidence_complete: true,
            active: false,
            observed_pids: Vec::new(),
            results_truncated: false,
            error: None,
        },
        plan_fingerprint: fingerprint_nibble.to_string().repeat(64),
        eligible_after_human_approval: true,
        blockers: vec!["human-local-eviction-approval-required".into()],
        notices: Vec::new(),
    }
}

fn batch_with_totals(
    item_plans: Vec<IcloudLocalEvictionPlan>,
    total_logical_bytes: u64,
    total_allocated_bytes: u64,
) -> IcloudLocalEvictionBatchPlan {
    let items: Vec<_> = item_plans
        .into_iter()
        .enumerate()
        .map(|(index, plan)| IcloudLocalEvictionBatchItem {
            input_index: u32::try_from(index).unwrap(),
            plan,
        })
        .collect();
    let count = u32::try_from(items.len()).unwrap();
    let mut plan = IcloudLocalEvictionBatchPlan {
        version: ICLOUD_LOCAL_EVICTION_BATCH_VERSION,
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        cloud_root: "/cloud".into(),
        observed_at_ms: 20,
        input_count: count,
        planned_count: count,
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

fn valid_two_item_batch() -> IcloudLocalEvictionBatchPlan {
    batch_with_totals(
        vec![
            safe_item("/cloud/a.bin", 'a', 2, 3),
            safe_item("/cloud/b.bin", 'b', 5, 7),
        ],
        7,
        10,
    )
}

fn approval_error(plan: &IcloudLocalEvictionBatchPlan) -> String {
    approve_icloud_local_eviction_batch(
        plan,
        &root(),
        &plan.batch_fingerprint,
        21,
        "human:operator",
        "reviewed exact batch integrity",
    )
    .unwrap_err()
}

fn refingerprint(plan: &mut IcloudLocalEvictionBatchPlan) {
    plan.batch_fingerprint = batch_fingerprint(plan);
}

#[test]
fn batch_item_identity_matrix_rejects_each_ambiguous_or_foreign_identity() {
    let base = valid_two_item_batch();
    let mut cases = Vec::new();

    let mut plan = base.clone();
    plan.items[1].input_index = plan.input_count;
    refingerprint(&mut plan);
    cases.push(plan);

    let mut plan = base.clone();
    plan.items[1].input_index = plan.items[0].input_index;
    refingerprint(&mut plan);
    cases.push(plan);

    let mut plan = base.clone();
    plan.items[1].plan.path = plan.items[0].plan.path.clone();
    refingerprint(&mut plan);
    cases.push(plan);

    let mut plan = base.clone();
    plan.items[1].plan.plan_fingerprint = plan.items[0].plan.plan_fingerprint.clone();
    refingerprint(&mut plan);
    cases.push(plan);

    let mut plan = base.clone();
    plan.items[1].plan.provider = CloudProvider::GoogleDrive;
    refingerprint(&mut plan);
    cases.push(plan);

    let mut plan = base.clone();
    plan.items[1].plan.account_scope = CloudAccountScope::Shared;
    refingerprint(&mut plan);
    cases.push(plan);

    let mut plan = base;
    plan.items[1].plan.cloud_root = "/other-cloud-root".into();
    refingerprint(&mut plan);
    cases.push(plan);

    for plan in cases {
        assert_eq!(
            approval_error(&plan),
            "icloud-local-eviction-batch-item-identity-invalid"
        );
    }
}

#[test]
fn batch_totals_fail_closed_on_u64_overflow_before_approval() {
    let logical_overflow = batch_with_totals(
        vec![
            safe_item("/cloud/max-logical.bin", 'a', u64::MAX, 1),
            safe_item("/cloud/one-logical.bin", 'b', 1, 1),
        ],
        0,
        2,
    );
    assert_eq!(
        approval_error(&logical_overflow),
        "icloud-local-eviction-batch-logical-total-overflow"
    );

    let allocated_overflow = batch_with_totals(
        vec![
            safe_item("/cloud/max-allocated.bin", 'a', 1, u64::MAX),
            safe_item("/cloud/one-allocated.bin", 'b', 1, 1),
        ],
        2,
        0,
    );
    assert_eq!(
        approval_error(&allocated_overflow),
        "icloud-local-eviction-batch-allocated-total-overflow"
    );
}

#[test]
fn batch_integrity_matrix_rejects_each_recomputed_or_stale_contradiction() {
    let base = valid_two_item_batch();
    let mut cases = Vec::new();

    let mut plan = base.clone();
    plan.total_logical_bytes = plan.total_logical_bytes.saturating_add(1);
    refingerprint(&mut plan);
    cases.push(plan);

    let mut plan = base.clone();
    plan.total_allocated_bytes = plan.total_allocated_bytes.saturating_add(1);
    refingerprint(&mut plan);
    cases.push(plan);

    let mut plan = base.clone();
    plan.blockers = vec!["unexpected-review-state".into()];
    refingerprint(&mut plan);
    cases.push(plan);

    let mut plan = base.clone();
    plan.eligible_after_human_approval = false;
    refingerprint(&mut plan);
    cases.push(plan);

    let mut plan = base.clone();
    plan.notices.pop();
    refingerprint(&mut plan);
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
