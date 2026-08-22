//! Exercise every public cloud-local-inventory option bound through the shipped API.
//!
//! These tests intentionally use the pure hard-timeout report path so invalid-option
//! behavior is verified without network access, provider credentials, or filesystem mutation.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::cloud_local_inventory::{
    hard_timeout_inventory, CloudLocalInventoryOptions,
};

fn root() -> CloudRoot {
    CloudRoot {
        id: "icloud:coverage-option-bounds".into(),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        label: "iCloud coverage option bounds".into(),
        path: "/Cloud".into(),
        readable: true,
        access_issue: None,
    }
}

fn assert_option_error(
    options: CloudLocalInventoryOptions,
    expected: &str,
) {
    assert_eq!(
        hard_timeout_inventory(&root(), options, 1).unwrap_err(),
        expected
    );
}

#[test]
fn rejects_each_invalid_option_boundary() {
    let defaults = CloudLocalInventoryOptions::default();

    assert_option_error(
        CloudLocalInventoryOptions {
            max_entries: 0,
            ..defaults
        },
        "cloud-local-inventory-max-entries-invalid",
    );
    assert_option_error(
        CloudLocalInventoryOptions {
            max_entries: 1_000_001,
            ..defaults
        },
        "cloud-local-inventory-max-entries-invalid",
    );
    assert_option_error(
        CloudLocalInventoryOptions {
            max_results: 0,
            ..defaults
        },
        "cloud-local-inventory-max-results-invalid",
    );
    assert_option_error(
        CloudLocalInventoryOptions {
            max_results: 10_001,
            ..defaults
        },
        "cloud-local-inventory-max-results-invalid",
    );
    assert_option_error(
        CloudLocalInventoryOptions {
            max_depth: 65,
            ..defaults
        },
        "cloud-local-inventory-max-depth-invalid",
    );
    assert_option_error(
        CloudLocalInventoryOptions {
            max_duration_ms: 0,
            ..defaults
        },
        "cloud-local-inventory-max-duration-invalid",
    );
    assert_option_error(
        CloudLocalInventoryOptions {
            max_duration_ms: 300_001,
            ..defaults
        },
        "cloud-local-inventory-max-duration-invalid",
    );
    assert_option_error(
        CloudLocalInventoryOptions {
            max_issues: 0,
            ..defaults
        },
        "cloud-local-inventory-max-issues-invalid",
    );
    assert_option_error(
        CloudLocalInventoryOptions {
            max_issues: 1_001,
            ..defaults
        },
        "cloud-local-inventory-max-issues-invalid",
    );
}

#[test]
fn accepts_exact_upper_bounds_without_filesystem_work() {
    let options = CloudLocalInventoryOptions {
        min_allocated_bytes: u64::MAX,
        max_entries: 1_000_000,
        max_results: 10_000,
        max_depth: 64,
        max_duration_ms: 300_000,
        max_issues: 1_000,
    };

    let report = hard_timeout_inventory(&root(), options, 42).unwrap();
    assert_eq!(report.options, options);
    assert_eq!(report.observed_at_ms, 42);
    assert!(!report.evidence_complete);
    assert_eq!(report.stop_reasons, vec!["hard-timeout-reached"]);
    assert!(report.candidates.is_empty());
}
