use std::process::Command;

#[test]
fn rejected_only_audit_returns_nonzero_after_emitting_json_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing.png");
    let output = Command::new(env!("CARGO_BIN_EXE_disksage-photo-duplicate-audit"))
        .arg(&missing)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(3));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["inspected_input_count"], 0);
    assert_eq!(report["evidence_complete"], false);
    assert_eq!(
        report["rejected_input_counts"]["photo-input-metadata-unavailable"],
        1
    );
}
