#![cfg(target_os = "macos")]

mod cli_under_test {
    include!("../src/bin/disksage-onedrive-temp-reclaim.rs");

    pub fn execution_is_failure(
        execution: &onedrive_temp_reclaim::OneDriveTempExecution,
    ) -> bool {
        execution_failed(execution)
    }
}

use disksage_lib::onedrive_temp_reclaim::OneDriveTempExecution;

fn execution(failure: Option<&str>, verification_complete: bool) -> OneDriveTempExecution {
    OneDriveTempExecution {
        ontology_class: "https://disksage.app/ontology#CloudTransferTemporaryArtifact",
        candidate_set_fingerprint: "a".repeat(64),
        planned_count: 2,
        removed_count: usize::from(failure.is_none()) * 2 + usize::from(failure.is_some()),
        removed_allocated_bytes_upper_bound: 4096,
        executed_at_ms: 1,
        filesystem_mutation_executed: true,
        verification_complete,
        failure: failure.map(str::to_string),
        recoverability: "not-recoverable; remote OneDrive content retained",
    }
}

#[test]
fn partial_execution_receipt_requires_failure_exit() {
    let receipt = execution(Some("onedrive-temp-remove-failed"), false);
    assert!(cli_under_test::execution_is_failure(&receipt));
}

#[test]
fn complete_execution_receipt_remains_successful() {
    let receipt = execution(None, true);
    assert!(!cli_under_test::execution_is_failure(&receipt));
}
