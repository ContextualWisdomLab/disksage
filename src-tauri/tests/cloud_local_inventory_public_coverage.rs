//! Public-contract coverage for bounded local-cloud inventory admission and timeout recovery.
//!
//! These tests exercise deterministic fail-closed option and checkpoint boundaries without
//! provider credentials, network calls, or file-content inspection.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::cloud_local_inventory::{
    hard_timeout_inventory, hard_timeout_inventory_from_checkpoint, CloudLocalInventoryOptions,
};

fn root() -> CloudRoot {
    CloudRoot {
        id: "icloud:test".into(),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        label: "iCloud test".into(),
        path: "/Cloud".into(),
        readable: true,
        access_issue: None,
    }
}

fn options() -> CloudLocalInventoryOptions {
    CloudLocalInventoryOptions {
        min_allocated_bytes: 1,
        max_entries: 100,
        max_results: 10,
        max_depth: 4,
        max_duration_ms: 10_000,
        max_issues: 10,
    }
}

#[test]
fn default_inventory_options_are_bounded_and_serializable() {
    let defaults = CloudLocalInventoryOptions::default();
    assert_eq!(defaults.min_allocated_bytes, 128 * 1024 * 1024);
    assert_eq!(defaults.max_entries, 100_000);
    assert_eq!(defaults.max_results, 200);
    assert_eq!(defaults.max_depth, 4);
    assert_eq!(defaults.max_duration_ms, 30_000);
    assert_eq!(defaults.max_issues, 200);

    let encoded = serde_json::to_string(&defaults).unwrap();
    let decoded: CloudLocalInventoryOptions = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, defaults);
    assert!(serde_json::from_str::<CloudLocalInventoryOptions>(
        r#"{"min_allocated_bytes":1,"max_entries":1,"max_results":1,"max_depth":0,"max_duration_ms":1,"max_issues":1,"unexpected":true}"#
    )
    .is_err());
}

#[test]
fn hard_timeout_option_bounds_fail_closed_at_every_public_limit() {
    let cases = [
        (
            CloudLocalInventoryOptions {
                max_entries: 0,
                ..options()
            },
            "cloud-local-inventory-max-entries-invalid",
        ),
        (
            CloudLocalInventoryOptions {
                max_entries: 1_000_001,
                ..options()
            },
            "cloud-local-inventory-max-entries-invalid",
        ),
        (
            CloudLocalInventoryOptions {
                max_results: 0,
                ..options()
            },
            "cloud-local-inventory-max-results-invalid",
        ),
        (
            CloudLocalInventoryOptions {
                max_results: 10_001,
                ..options()
            },
            "cloud-local-inventory-max-results-invalid",
        ),
        (
            CloudLocalInventoryOptions {
                max_depth: 65,
                ..options()
            },
            "cloud-local-inventory-max-depth-invalid",
        ),
        (
            CloudLocalInventoryOptions {
                max_duration_ms: 0,
                ..options()
            },
            "cloud-local-inventory-max-duration-invalid",
        ),
        (
            CloudLocalInventoryOptions {
                max_duration_ms: 300_001,
                ..options()
            },
            "cloud-local-inventory-max-duration-invalid",
        ),
        (
            CloudLocalInventoryOptions {
                max_issues: 0,
                ..options()
            },
            "cloud-local-inventory-max-issues-invalid",
        ),
        (
            CloudLocalInventoryOptions {
                max_issues: 1_001,
                ..options()
            },
            "cloud-local-inventory-max-issues-invalid",
        ),
    ];

    for (invalid, expected) in cases {
        assert_eq!(
            hard_timeout_inventory(&root(), invalid, 1).unwrap_err(),
            expected
        );
    }
}

#[test]
fn checkpoint_recovery_binds_every_scope_field_and_terminal_marker() {
    let root = root();
    let options = options();
    let mut checkpoint = hard_timeout_inventory(&root, options, 123).unwrap();
    checkpoint.stop_reasons.clear();
    checkpoint.notices.clear();
    checkpoint.notices.push("inventory-checkpoint-not-terminal".into());

    let recovered =
        hard_timeout_inventory_from_checkpoint(&root, options, checkpoint.clone()).unwrap();
    assert!(!recovered.evidence_complete);
    assert!(recovered
        .stop_reasons
        .contains(&"hard-timeout-reached".to_string()));
    assert!(recovered.notices.contains(&"inventory-incomplete".to_string()));
    assert!(recovered.notices.contains(&"worker-hard-timeout".to_string()));
    assert!(recovered
        .notices
        .contains(&"partial-inventory-recovered-from-worker-checkpoint".to_string()));
    assert!(!recovered
        .notices
        .contains(&"inventory-checkpoint-not-terminal".to_string()));

    let mut invalid = Vec::new();

    let mut wrong_version = checkpoint.clone();
    wrong_version.version = 1;
    invalid.push(wrong_version);

    let mut wrong_root_id = checkpoint.clone();
    wrong_root_id.cloud_root_id = "icloud:other".into();
    invalid.push(wrong_root_id);

    let mut wrong_provider = checkpoint.clone();
    wrong_provider.provider = CloudProvider::GoogleDrive;
    invalid.push(wrong_provider);

    let mut wrong_scope = checkpoint.clone();
    wrong_scope.account_scope = CloudAccountScope::Shared;
    invalid.push(wrong_scope);

    let mut wrong_path = checkpoint.clone();
    wrong_path.cloud_root = "/Other".into();
    invalid.push(wrong_path);

    let mut wrong_options = checkpoint.clone();
    wrong_options.options.max_entries += 1;
    invalid.push(wrong_options);

    let mut falsely_complete = checkpoint.clone();
    falsely_complete.evidence_complete = true;
    invalid.push(falsely_complete);

    let mut missing_marker = checkpoint;
    missing_marker.notices.clear();
    invalid.push(missing_marker);

    for candidate in invalid {
        assert_eq!(
            hard_timeout_inventory_from_checkpoint(&root, options, candidate).unwrap_err(),
            "cloud-local-inventory-checkpoint-invalid"
        );
    }
}
