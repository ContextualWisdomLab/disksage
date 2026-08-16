//! Public coverage for cloud-local-inventory checkpoint authority and option bounds.
//!
//! These tests use only a temporary empty directory and in-memory evidence. They verify that a
//! checkpoint cannot be replayed under a different provider or account scope, and that the public
//! watchdog constructor enforces both sides of every documented option bound without contacting a
//! provider, opening file contents, or authorizing eviction.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::cloud_local_inventory::{
    hard_timeout_inventory, hard_timeout_inventory_from_checkpoint,
    inventory_cloud_local_allocations_with_checkpoints, CloudLocalAllocationInventory,
    CloudLocalInventoryOptions,
};
use std::path::Path;

fn cloud_root(path: &Path) -> CloudRoot {
    CloudRoot {
        id: "icloud:checkpoint-authority-coverage".into(),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        label: "checkpoint authority coverage".into(),
        path: path.to_string_lossy().into_owned(),
        readable: true,
        access_issue: None,
    }
}

fn bounded_options() -> CloudLocalInventoryOptions {
    CloudLocalInventoryOptions {
        min_allocated_bytes: 1,
        max_entries: 100,
        max_results: 10,
        max_depth: 4,
        max_duration_ms: 10_000,
        max_issues: 10,
    }
}

fn checkpoint_fixture() -> (CloudRoot, CloudLocalInventoryOptions, CloudLocalAllocationInventory) {
    let directory = tempfile::tempdir().expect("temporary cloud inventory root");
    let root = cloud_root(directory.path());
    let options = bounded_options();
    let mut checkpoint = None;
    let terminal = inventory_cloud_local_allocations_with_checkpoints(
        &root,
        options,
        123,
        |value| {
            checkpoint = Some(value.clone());
            Ok(())
        },
    )
    .expect("empty metadata-only inventory");
    assert!(terminal.evidence_complete);
    (
        root,
        options,
        checkpoint.expect("initial non-terminal checkpoint"),
    )
}

#[test]
fn checkpoint_recovery_rejects_provider_and_account_authority_drift() {
    let (root, options, checkpoint) = checkpoint_fixture();

    let mut wrong_provider = checkpoint.clone();
    wrong_provider.provider = CloudProvider::GoogleDrive;
    assert_eq!(
        hard_timeout_inventory_from_checkpoint(&root, options, wrong_provider).unwrap_err(),
        "cloud-local-inventory-checkpoint-invalid"
    );

    let mut wrong_account_scope = checkpoint;
    wrong_account_scope.account_scope = CloudAccountScope::Organization;
    assert_eq!(
        hard_timeout_inventory_from_checkpoint(&root, options, wrong_account_scope).unwrap_err(),
        "cloud-local-inventory-checkpoint-invalid"
    );
}

#[test]
fn checkpoint_recovery_rejects_identity_state_and_contract_drift() {
    let (root, options, checkpoint) = checkpoint_fixture();

    let mut wrong_version = checkpoint.clone();
    wrong_version.version = 1;
    assert_eq!(
        hard_timeout_inventory_from_checkpoint(&root, options, wrong_version).unwrap_err(),
        "cloud-local-inventory-checkpoint-invalid"
    );

    let mut wrong_root_id = checkpoint.clone();
    wrong_root_id.cloud_root_id = "icloud:other-root".into();
    assert_eq!(
        hard_timeout_inventory_from_checkpoint(&root, options, wrong_root_id).unwrap_err(),
        "cloud-local-inventory-checkpoint-invalid"
    );

    let mut wrong_root_path = checkpoint.clone();
    wrong_root_path.cloud_root = "/different/root".into();
    assert_eq!(
        hard_timeout_inventory_from_checkpoint(&root, options, wrong_root_path).unwrap_err(),
        "cloud-local-inventory-checkpoint-invalid"
    );

    let mut wrong_options = checkpoint.clone();
    wrong_options.options.max_results += 1;
    assert_eq!(
        hard_timeout_inventory_from_checkpoint(&root, options, wrong_options).unwrap_err(),
        "cloud-local-inventory-checkpoint-invalid"
    );

    let mut terminal_claim = checkpoint.clone();
    terminal_claim.evidence_complete = true;
    assert_eq!(
        hard_timeout_inventory_from_checkpoint(&root, options, terminal_claim).unwrap_err(),
        "cloud-local-inventory-checkpoint-invalid"
    );

    let mut missing_checkpoint_notice = checkpoint;
    missing_checkpoint_notice
        .notices
        .retain(|notice| notice != "inventory-checkpoint-not-terminal");
    assert_eq!(
        hard_timeout_inventory_from_checkpoint(&root, options, missing_checkpoint_notice).unwrap_err(),
        "cloud-local-inventory-checkpoint-invalid"
    );
}

#[test]
fn hard_timeout_enforces_lower_option_bounds_and_accepts_zero_depth() {
    let root = cloud_root(Path::new("/Cloud"));
    let base = bounded_options();
    let invalid_cases = [
        (
            CloudLocalInventoryOptions {
                max_entries: 0,
                ..base
            },
            "cloud-local-inventory-max-entries-invalid",
        ),
        (
            CloudLocalInventoryOptions {
                max_results: 0,
                ..base
            },
            "cloud-local-inventory-max-results-invalid",
        ),
        (
            CloudLocalInventoryOptions {
                max_duration_ms: 0,
                ..base
            },
            "cloud-local-inventory-max-duration-invalid",
        ),
        (
            CloudLocalInventoryOptions {
                max_issues: 0,
                ..base
            },
            "cloud-local-inventory-max-issues-invalid",
        ),
    ];

    for (invalid, expected) in invalid_cases {
        assert_eq!(hard_timeout_inventory(&root, invalid, 1).unwrap_err(), expected);
    }

    let zero_depth = CloudLocalInventoryOptions {
        max_depth: 0,
        ..base
    };
    let report = hard_timeout_inventory(&root, zero_depth, 2)
        .expect("zero depth is a valid root-only inventory bound");
    assert_eq!(report.options, zero_depth);
    assert!(!report.evidence_complete);
    assert_eq!(report.stop_reasons, vec!["hard-timeout-reached"]);
}

#[test]
fn hard_timeout_enforces_upper_option_bounds_and_accepts_exact_maxima() {
    let root = cloud_root(Path::new("/Cloud"));
    let base = bounded_options();
    let invalid_cases = [
        (
            CloudLocalInventoryOptions {
                max_entries: 1_000_001,
                ..base
            },
            "cloud-local-inventory-max-entries-invalid",
        ),
        (
            CloudLocalInventoryOptions {
                max_results: 10_001,
                ..base
            },
            "cloud-local-inventory-max-results-invalid",
        ),
        (
            CloudLocalInventoryOptions {
                max_depth: 65,
                ..base
            },
            "cloud-local-inventory-max-depth-invalid",
        ),
        (
            CloudLocalInventoryOptions {
                max_duration_ms: 300_001,
                ..base
            },
            "cloud-local-inventory-max-duration-invalid",
        ),
        (
            CloudLocalInventoryOptions {
                max_issues: 1_001,
                ..base
            },
            "cloud-local-inventory-max-issues-invalid",
        ),
    ];

    for (invalid, expected) in invalid_cases {
        assert_eq!(hard_timeout_inventory(&root, invalid, 1).unwrap_err(), expected);
    }

    let maxima = CloudLocalInventoryOptions {
        min_allocated_bytes: u64::MAX,
        max_entries: 1_000_000,
        max_results: 10_000,
        max_depth: 64,
        max_duration_ms: 300_000,
        max_issues: 1_000,
    };
    let report = hard_timeout_inventory(&root, maxima, 2).expect("exact public maxima are valid");
    assert_eq!(report.options, maxima);
    assert!(!report.evidence_complete);
    assert_eq!(report.stop_reasons, vec!["hard-timeout-reached"]);
    assert!(report.notices.iter().any(|notice| notice == "worker-hard-timeout"));
}
