//! Public fail-closed option-bound coverage for local cloud allocation inventory.
//!
//! These tests exercise the real hard-timeout report constructor, which validates the same bounded
//! options used by live inventory before returning a no-I/O terminal report. No cloud provider,
//! credential, network request, or filesystem mutation is involved.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::cloud_local_inventory::{
    hard_timeout_inventory, CloudLocalInventoryOptions,
};

fn root() -> CloudRoot {
    CloudRoot {
        id: "cloud-local-option-coverage".into(),
        provider: CloudProvider::Onedrive,
        account_scope: CloudAccountScope::Personal,
        label: "Coverage root".into(),
        path: "/coverage/cloud".into(),
        readable: true,
        access_issue: None,
    }
}

fn assert_invalid(mut options: CloudLocalInventoryOptions, mutate: impl FnOnce(&mut CloudLocalInventoryOptions), expected: &str) {
    mutate(&mut options);
    assert_eq!(hard_timeout_inventory(&root(), options, 42).unwrap_err(), expected);
}

#[test]
fn hard_timeout_rejects_every_zero_and_upper_bound_violation() {
    let defaults = CloudLocalInventoryOptions::default();

    assert_invalid(defaults, |o| o.max_entries = 0, "cloud-local-inventory-max-entries-invalid");
    assert_invalid(defaults, |o| o.max_entries = 1_000_001, "cloud-local-inventory-max-entries-invalid");
    assert_invalid(defaults, |o| o.max_results = 0, "cloud-local-inventory-max-results-invalid");
    assert_invalid(defaults, |o| o.max_results = 10_001, "cloud-local-inventory-max-results-invalid");
    assert_invalid(defaults, |o| o.max_depth = 65, "cloud-local-inventory-max-depth-invalid");
    assert_invalid(defaults, |o| o.max_duration_ms = 0, "cloud-local-inventory-max-duration-invalid");
    assert_invalid(defaults, |o| o.max_duration_ms = 300_001, "cloud-local-inventory-max-duration-invalid");
    assert_invalid(defaults, |o| o.max_issues = 0, "cloud-local-inventory-max-issues-invalid");
    assert_invalid(defaults, |o| o.max_issues = 1_001, "cloud-local-inventory-max-issues-invalid");
}

#[test]
fn hard_timeout_returns_bounded_non_authorizing_terminal_evidence() {
    let options = CloudLocalInventoryOptions::default();
    let report = hard_timeout_inventory(&root(), options, 42).unwrap();

    assert_eq!(report.version, 2);
    assert_eq!(report.cloud_root_id, "cloud-local-option-coverage");
    assert_eq!(report.provider, CloudProvider::Onedrive);
    assert_eq!(report.account_scope, CloudAccountScope::Personal);
    assert_eq!(report.observed_at_ms, 42);
    assert_eq!(report.options, options);
    assert_eq!(report.visited_entries, 0);
    assert_eq!(report.visited_files, 0);
    assert_eq!(report.visited_directories, 0);
    assert_eq!(report.skipped_entries, 0);
    assert!(report.issues.is_empty());
    assert!(!report.issues_truncated);
    assert_eq!(report.allocated_candidate_bytes, 0);
    assert!(report.candidates.is_empty());
    assert!(!report.results_truncated);
    assert!(!report.evidence_complete);
    assert_eq!(report.stop_reasons, vec!["hard-timeout-reached"]);
    assert!(report.notices.iter().any(|notice| notice == "metadata-only-content-not-opened"));
    assert!(report.notices.iter().any(|notice| notice == "provider-sync-not-attested"));
    assert!(report.notices.iter().any(|notice| notice == "inventory-does-not-authorize-eviction"));
    assert!(report.notices.iter().any(|notice| notice == "inventory-incomplete"));
    assert!(report.notices.iter().any(|notice| notice == "worker-hard-timeout"));
}
