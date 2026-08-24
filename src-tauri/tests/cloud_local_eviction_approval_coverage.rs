use disksage_lib::cloud::{CloudAccountScope, CloudProvider};
use disksage_lib::cloud_local_eviction::{
    approve_icloud_local_eviction, ActiveUseEvidence, IcloudLocalEvictionPlan, IcloudLocalState,
    IcloudStateObservationMethod, ICLOUD_LOCAL_EVICTION_VERSION,
};

fn eligible_plan() -> IcloudLocalEvictionPlan {
    IcloudLocalEvictionPlan {
        version: ICLOUD_LOCAL_EVICTION_VERSION,
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        cloud_root: "/cloud".into(),
        path: "/cloud/report.pdf".into(),
        logical_bytes: 42,
        allocated_bytes: 42,
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
            method: "lsof-fp+ps-command".into(),
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

#[test]
fn eligible_plan_produces_integrity_bound_trimmed_human_approval() {
    let plan = eligible_plan();
    let approval = approve_icloud_local_eviction(
        &plan,
        &plan.plan_fingerprint,
        21,
        "  human:operator  ",
        "  reviewed exact local eviction evidence  ",
    )
    .unwrap();

    assert_eq!(approval.version, ICLOUD_LOCAL_EVICTION_VERSION);
    assert_eq!(approval.plan_fingerprint, plan.plan_fingerprint);
    assert_eq!(approval.approved_at_ms, 21);
    assert_eq!(approval.approved_by, "human:operator");
    assert_eq!(approval.rationale, "reviewed exact local eviction evidence");
    assert_eq!(approval.approval_id.len(), 64);
    assert!(approval
        .approval_id
        .bytes()
        .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));
}

#[test]
fn approval_admission_fails_closed_for_identity_eligibility_and_time_drift() {
    let plan = eligible_plan();

    assert_eq!(
        approve_icloud_local_eviction(&plan, &"f".repeat(64), 21, "human:operator", "reviewed")
            .unwrap_err(),
        "icloud-local-eviction-plan-fingerprint-mismatch"
    );

    let mut malformed_fingerprint = plan.clone();
    malformed_fingerprint.plan_fingerprint = "short".into();
    assert_eq!(
        approve_icloud_local_eviction(
            &malformed_fingerprint,
            "short",
            21,
            "human:operator",
            "reviewed",
        )
        .unwrap_err(),
        "icloud-local-eviction-plan-fingerprint-mismatch"
    );

    let mut blocked = plan.clone();
    blocked.eligible_after_human_approval = false;
    blocked.blockers.push("active-file-use-detected".into());
    assert_eq!(
        approve_icloud_local_eviction(
            &blocked,
            &blocked.plan_fingerprint,
            21,
            "human:operator",
            "reviewed",
        )
        .unwrap_err(),
        "icloud-local-eviction-plan-not-eligible"
    );

    assert_eq!(
        approve_icloud_local_eviction(
            &plan,
            &plan.plan_fingerprint,
            21,
            "agent:operator",
            "reviewed",
        )
        .unwrap_err(),
        "icloud-local-eviction-human-attribution-required"
    );
    assert_eq!(
        approve_icloud_local_eviction(
            &plan,
            &plan.plan_fingerprint,
            21,
            "human:",
            "reviewed",
        )
        .unwrap_err(),
        "icloud-local-eviction-human-attribution-required"
    );

    assert_eq!(
        approve_icloud_local_eviction(
            &plan,
            &plan.plan_fingerprint,
            21,
            "human:operator",
            "   ",
        )
        .unwrap_err(),
        "icloud-local-eviction-rationale-invalid"
    );
    assert_eq!(
        approve_icloud_local_eviction(
            &plan,
            &plan.plan_fingerprint,
            21,
            "human:operator",
            &"x".repeat(1025),
        )
        .unwrap_err(),
        "icloud-local-eviction-rationale-invalid"
    );

    assert_eq!(
        approve_icloud_local_eviction(
            &plan,
            &plan.plan_fingerprint,
            19,
            "human:operator",
            "reviewed",
        )
        .unwrap_err(),
        "icloud-local-eviction-approval-predates-plan"
    );
}
