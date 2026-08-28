use disksage_lib::runtime_storage::{RuntimeStorageExecution, RuntimeStorageKind};

#[test]
fn runtime_storage_execution_serialization_omits_guest_output() {
    let secret_stdout = "/Users/customer/Documents/acquisition/private-plan.txt";
    let secret_stderr = "token=customer-secret-runtime-diagnostic";
    let execution = RuntimeStorageExecution {
        schema_kind: "disksage.runtime-storage-execution",
        schema_version: 1,
        runtime: RuntimeStorageKind::Colima,
        command: vec![
            "colima".into(),
            "ssh".into(),
            "--".into(),
            "sudo".into(),
            "fstrim".into(),
            "-av".into(),
        ],
        status_code: 17,
        stdout: secret_stdout.into(),
        stderr: secret_stderr.into(),
        output_truncated: true,
        executed: false,
        executed_at_ms: 42,
        rationale: "operator approved trim".into(),
        volume_comparison: None,
        volume_evidence_error: None,
    };

    let json = serde_json::to_string(&execution).expect("runtime storage execution serializes");

    assert!(!json.contains("\"stdout\""));
    assert!(!json.contains("\"stderr\""));
    assert!(!json.contains(secret_stdout));
    assert!(!json.contains(secret_stderr));
    assert!(json.contains("\"status_code\":17"));
    assert!(json.contains("\"output_truncated\":true"));
}
