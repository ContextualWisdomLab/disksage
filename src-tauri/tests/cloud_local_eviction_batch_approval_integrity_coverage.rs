//! Exact-head coverage for fail-closed iCloud local-eviction batch approval integrity.
//!
//! These tests exercise the public execution boundary only. Tampered approval records must be
//! rejected before any preflight filesystem probe or immutable record publication can occur.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::cloud_local_eviction::{
    ActiveUseEvidence, IcloudLocalEvictionPlan, IcloudLocalState, IcloudStateObservationMethod,
    ICLOUD_LOCAL_EVICTION_VERSION,
};
use disksage_lib::cloud_local_eviction_batch::{
    approve_icloud_local_eviction_batch, execute_icloud_local_eviction_batch,
    IcloudLocalEvictionBatchApproval, IcloudLocalEvictionBatchItem, IcloudLocalEvictionBatchPlan,
    ICLOUD_LOCAL_EVICTION_BATCH_VERSION,
};

fn root() -> CloudRoot {
    CloudRoot {
        id: "icloud:batch-approval-integrity".into(),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        label: "iCloud batch approval integrity".into(),
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

fn approval_id(approval: &IcloudLocalEvictionBatchApproval) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-icloud-local-eviction-batch-approval-v1\0");
    hash_field(&mut hasher, approval.batch_fingerprint.as_bytes());
    hash_field(&mut hasher, approval.approved_by.as_bytes());
    hash_field(&mut hasher, approval.rationale.as_bytes());
    hasher.update(&approval.approved_at_ms.to_le_bytes());
    hasher.finalize().to_hex().to_string()
}

fn eligible_plan() -> IcloudLocalEvictionBatchPlan {
    let item = IcloudLocalEvictionPlan {
        version: ICLOUD_LOCAL_EVICTION_VERSION,
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        cloud_root: "/cloud".into(),
        path: "/cloud/reviewed-item.bin".into(),
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
            method: "approval-integrity-coverage".into(),
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
    };
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
        eligible_after_human_approval: true,
        blockers: vec!["human-local-eviction-batch-approval-required".into()],
        notices: notices(),
    };
    plan.batch_fingerprint = batch_fingerprint(&plan);
    plan
}

fn valid_approval(plan: &IcloudLocalEvictionBatchPlan) -> IcloudLocalEvictionBatchApproval {
    approve_icloud_local_eviction_batch(
        plan,
        &root(),
        &plan.batch_fingerprint,
        21,
        "human:operator",
        "reviewed exact batch",
    )
    .expect("fixture must produce a valid attributed approval")
}

fn assert_integrity_rejection(
    plan: &IcloudLocalEvictionBatchPlan,
    approval: &IcloudLocalEvictionBatchApproval,
    confirmation: &str,
) {
    let record_parent = tempfile::tempdir().expect("temporary evidence parent");
    let record_dir = record_parent.path().join("records-must-not-exist");

    let error = execute_icloud_local_eviction_batch(
        &root(),
        plan,
        approval,
        confirmation,
        &record_dir,
        22,
    )
    .expect_err("tampered approval must fail before preflight or record publication");

    assert_eq!(
        error,
        "icloud-local-eviction-batch-approval-integrity-mismatch"
    );
    assert!(
        !record_dir.exists(),
        "approval-integrity rejection must not publish authority records"
    );
}

#[test]
fn execution_rejects_each_tampered_batch_approval_guard_before_preflight() {
    let plan = eligible_plan();
    let base = valid_approval(&plan);

    let mut wrong_version = base.clone();
    wrong_version.version += 1;
    assert_integrity_rejection(&plan, &wrong_version, &plan.batch_fingerprint);

    let mut wrong_plan_fingerprint = base.clone();
    wrong_plan_fingerprint.batch_fingerprint = "c".repeat(64);
    wrong_plan_fingerprint.approval_id = approval_id(&wrong_plan_fingerprint);
    assert_integrity_rejection(
        &plan,
        &wrong_plan_fingerprint,
        &wrong_plan_fingerprint.batch_fingerprint,
    );

    assert_integrity_rejection(&plan, &base, &"d".repeat(64));

    let mut wrong_approval_id = base.clone();
    wrong_approval_id.approval_id = "e".repeat(64);
    assert_integrity_rejection(&plan, &wrong_approval_id, &plan.batch_fingerprint);

    let mut predates_plan = base.clone();
    predates_plan.approved_at_ms = 19;
    predates_plan.approval_id = approval_id(&predates_plan);
    assert_integrity_rejection(&plan, &predates_plan, &plan.batch_fingerprint);

    let mut padded_reviewer = base.clone();
    padded_reviewer.approved_by = " human:operator".into();
    padded_reviewer.approval_id = approval_id(&padded_reviewer);
    assert_integrity_rejection(&plan, &padded_reviewer, &plan.batch_fingerprint);

    let mut non_human_reviewer = base.clone();
    non_human_reviewer.approved_by = "agent:operator".into();
    non_human_reviewer.approval_id = approval_id(&non_human_reviewer);
    assert_integrity_rejection(&plan, &non_human_reviewer, &plan.batch_fingerprint);

    let mut empty_human_reviewer = base.clone();
    empty_human_reviewer.approved_by = "human:".into();
    empty_human_reviewer.approval_id = approval_id(&empty_human_reviewer);
    assert_integrity_rejection(&plan, &empty_human_reviewer, &plan.batch_fingerprint);

    let mut padded_rationale = base.clone();
    padded_rationale.rationale = " reviewed exact batch".into();
    padded_rationale.approval_id = approval_id(&padded_rationale);
    assert_integrity_rejection(&plan, &padded_rationale, &plan.batch_fingerprint);

    let mut empty_rationale = base.clone();
    empty_rationale.rationale.clear();
    empty_rationale.approval_id = approval_id(&empty_rationale);
    assert_integrity_rejection(&plan, &empty_rationale, &plan.batch_fingerprint);

    let mut oversized_rationale = base;
    oversized_rationale.rationale = "x".repeat(1_025);
    oversized_rationale.approval_id = approval_id(&oversized_rationale);
    assert_integrity_rejection(&plan, &oversized_rationale, &plan.batch_fingerprint);
}
