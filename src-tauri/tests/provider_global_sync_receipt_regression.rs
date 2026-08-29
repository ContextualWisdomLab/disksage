use disksage_lib::cloud::CloudProvider;
use disksage_lib::provider_global_sync::{
    parse_dump, require_new_copy_admission, ProviderProbeNextAction, ProviderProbeOutcome,
    ProviderProbeReceipt,
};

const QUIET_DUMP: &str = r#"
com.microsoft.OneDrive.FileProvider
sync engine state:
    + scheduling state: idle
    + pending-indexable-count: 0
    + errors: 0
"#;

#[test]
fn partial_timeout_preserves_bounded_keep_local_receipt() {
    let dump = format!(
        "{QUIET_DUMP}\n+ provider-global-sync-probe-timeout: yes\n"
    );
    let report = parse_dump(CloudProvider::Onedrive, &dump).expect("partial dump remains parseable");

    assert!(!report.evidence_complete);
    let receipt = report
        .probe_receipt
        .expect("every timeout must produce an inconclusive keep-local receipt");
    assert_eq!(receipt.outcome, ProviderProbeOutcome::Inconclusive);
    assert!(receipt.keep_local);
    assert_eq!(receipt.next_action, ProviderProbeNextAction::KeepLocalAndRescan);
    assert_eq!(
        receipt.audit_reason_codes,
        vec!["provider-global-sync-probe-timeout"]
    );
    let encoded = serde_json::to_string(&receipt).expect("receipt serializes");
    assert!(encoded.len() < 1_024);
    assert!(!encoded.contains('/'));
}

#[test]
fn clear_report_cannot_carry_inconclusive_receipt() {
    let mut report = parse_dump(CloudProvider::Onedrive, QUIET_DUMP).expect("quiet report");
    report.probe_receipt = Some(ProviderProbeReceipt {
        schema_kind: "disksage.provider-probe-receipt".into(),
        schema_version: 1,
        observed_at_ms: 42,
        outcome: ProviderProbeOutcome::Inconclusive,
        keep_local: true,
        next_action: ProviderProbeNextAction::KeepLocalAndRescan,
        audit_reason_codes: vec!["provider-global-sync-probe-timeout".into()],
    });

    assert_eq!(
        require_new_copy_admission(&report).expect_err("contradictory evidence must fail closed"),
        "provider-global-sync-evidence-invalid"
    );
}
