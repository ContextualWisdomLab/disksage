//! Source contract for cloud-transfer coverage instrumentation.
//!
//! The approval fixture is required by ordinary Rust tests even when the central coverage runner
//! adds `--cfg coverage`. This regression prevents a future refactor from excluding the helper and
//! breaking the receipt-lineage tests before coverage can be measured.

/// Verifies that the deterministic approval fixture remains compiled for every test build.
#[test]
fn approval_fixture_remains_available_during_coverage_builds() {
    let source = include_str!("../src/cloud_transfer.rs");

    assert!(
        source.contains("#[cfg(test)]\nfn test_copy_approval("),
        "test_copy_approval must remain available when cfg(coverage) is active"
    );
    assert!(
        !source.contains("#[cfg(all(test, not(coverage)))]\nfn test_copy_approval("),
        "test_copy_approval must not be excluded from coverage-mode test compilation"
    );
}
