#[test]
fn successful_onedrive_unpin_preserves_restart_failure_as_verification_evidence() {
    let recovery = include_str!("../src/provider_recovery.rs");
    let eviction = include_str!("../src/cloud_local_eviction.rs");

    assert!(
        recovery.contains("struct OneDriveUnpinOutcome")
            && recovery.contains("restart_blockers: Vec<String>"),
        "OneDrive unpin must distinguish a completed local eviction request from provider restart evidence"
    );
    assert!(
        !recovery.contains("restart.and(operation)"),
        "a successful unpin must not disappear behind a later provider restart failure"
    );
    assert!(
        eviction.contains("request_blockers")
            && eviction.contains("build_result(")
            && eviction.contains("verification_blockers"),
        "post-eviction verification must retain provider restart blockers in the immutable result"
    );
}
