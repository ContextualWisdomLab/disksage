//! Public option-bound contracts for cloud-local allocation inventory.
//!
//! These tests exercise the shipped fail-closed validation boundary without opening filesystem
//! content or contacting a provider. They intentionally use the hard-timeout projection because
//! it shares the same production option validator while requiring no readable cloud root.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::cloud_local_inventory::{
    hard_timeout_inventory, CloudLocalInventoryOptions,
};

fn root() -> CloudRoot {
    CloudRoot {
        id: "icloud:coverage-options".into(),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        label: "Coverage options".into(),
        path: "/not-opened-by-hard-timeout-contract".into(),
        readable: false,
        access_issue: Some("not-opened".into()),
    }
}

fn assert_invalid(options: CloudLocalInventoryOptions, expected: &str) {
    let error = hard_timeout_inventory(&root(), options, 101).unwrap_err();
    assert_eq!(error, expected);
}

#[test]
fn hard_timeout_rejects_every_out_of_contract_inventory_bound() {
    let mut options = CloudLocalInventoryOptions::default();
    options.max_entries = 0;
    assert_invalid(options, "cloud-local-inventory-max-entries-invalid");

    let mut options = CloudLocalInventoryOptions::default();
    options.max_entries = 1_000_001;
    assert_invalid(options, "cloud-local-inventory-max-entries-invalid");

    let mut options = CloudLocalInventoryOptions::default();
    options.max_results = 0;
    assert_invalid(options, "cloud-local-inventory-max-results-invalid");

    let mut options = CloudLocalInventoryOptions::default();
    options.max_results = 10_001;
    assert_invalid(options, "cloud-local-inventory-max-results-invalid");

    let mut options = CloudLocalInventoryOptions::default();
    options.max_depth = 65;
    assert_invalid(options, "cloud-local-inventory-max-depth-invalid");

    let mut options = CloudLocalInventoryOptions::default();
    options.max_duration_ms = 0;
    assert_invalid(options, "cloud-local-inventory-max-duration-invalid");

    let mut options = CloudLocalInventoryOptions::default();
    options.max_duration_ms = 300_001;
    assert_invalid(options, "cloud-local-inventory-max-duration-invalid");

    let mut options = CloudLocalInventoryOptions::default();
    options.max_issues = 0;
    assert_invalid(options, "cloud-local-inventory-max-issues-invalid");

    let mut options = CloudLocalInventoryOptions::default();
    options.max_issues = 1_001;
    assert_invalid(options, "cloud-local-inventory-max-issues-invalid");
}

#[test]
fn hard_timeout_accepts_default_bounds_without_touching_cloud_storage() {
    let options = CloudLocalInventoryOptions::default();
    let report = hard_timeout_inventory(&root(), options, 202).unwrap();

    assert_eq!(report.version, 2);
    assert_eq!(report.observed_at_ms, 202);
    assert_eq!(report.options, options);
    assert!(!report.evidence_complete);
    assert_eq!(report.stop_reasons, vec!["hard-timeout-reached"]);
    assert!(report.notices.iter().any(|notice| notice == "inventory-incomplete"));
    assert!(report.notices.iter().any(|notice| notice == "worker-hard-timeout"));
    assert_eq!(report.visited_entries, 0);
    assert!(report.candidates.is_empty());
}
