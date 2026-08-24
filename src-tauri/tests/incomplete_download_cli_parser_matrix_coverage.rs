#![cfg(feature = "cloud-cli")]

//! Real-binary parser coverage for bounded incomplete-download operator CLIs.
//!
//! Every case fails during argument admission, before HOME lookup, provider discovery,
//! capacity I/O, private evidence writes, or filesystem mutation.

use std::process::Command;

fn assert_rejected(binary: &str, args: &[&str]) {
    let output = Command::new(binary)
        .args(args)
        .env_remove("HOME")
        .env_remove("USERPROFILE")
        .output()
        .expect("the shipped incomplete-download binary should start");

    assert_eq!(
        output.status.code(),
        Some(2),
        "arguments should fail closed: {args:?}; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty(), "invalid arguments must not emit success JSON");
    let stderr = String::from_utf8(output.stderr).expect("diagnostics should remain UTF-8");
    assert!(!stderr.contains("panicked"), "invalid arguments must not panic: {stderr}");
}

#[test]
fn destination_plan_parser_rejects_duplicate_missing_unbounded_and_unsafe_inputs() {
    let binary = env!("CARGO_BIN_EXE_disksage-incomplete-download-destination-plan");
    let cases: &[&[&str]] = &[
        &[],
        &["--source-root"],
        &["--source-root", "/a", "--source-root", "/b"],
        &["--source-root", "/source"],
        &["--source-root", "/source", "--cloud-root", "/cloud"],
        &["--source-root", "relative", "--cloud-root", "/cloud", "--destination-subdirectory", "safe", "--live-icloud-capacity"],
        &["--source-root", "/source", "--cloud-root", "relative", "--destination-subdirectory", "safe", "--live-icloud-capacity"],
        &["--source-root", "/source", "--cloud-root", "/cloud", "--destination-subdirectory", "../escape", "--live-icloud-capacity"],
        &["--source-root", "/source", "--cloud-root", "/cloud", "--destination-subdirectory", "safe"],
        &["--source-root", "/source", "--cloud-root", "/cloud", "--destination-subdirectory", "safe", "--live-icloud-capacity", "--capacity-snapshot", "/capacity.json"],
        &["--source-root", "/source", "--cloud-root", "/cloud", "--destination-subdirectory", "safe", "--capacity-snapshot", "relative.json"],
        &["--source-root", "/source", "--cloud-root", "/cloud", "--destination-subdirectory", "safe", "--live-icloud-capacity", "--live-icloud-capacity"],
        &["--source-root", "/source", "--cloud-root", "/cloud", "--destination-subdirectory", "safe", "--capacity-snapshot", "/a", "--capacity-snapshot", "/b"],
        &["--source-root", "/source", "--cloud-root", "/cloud", "--destination-subdirectory", "safe", "--live-icloud-capacity", "--private-output", "relative.json"],
        &["--source-root", "/source", "--cloud-root", "/cloud", "--destination-subdirectory", "safe", "--live-icloud-capacity", "--private-output", "/a", "--private-output", "/b"],
        &["--source-root", "/source", "--cloud-root", "/cloud", "--destination-subdirectory", "safe", "--live-icloud-capacity", "--max-entries", "0"],
        &["--source-root", "/source", "--cloud-root", "/cloud", "--destination-subdirectory", "safe", "--live-icloud-capacity", "--max-entries", "not-a-number"],
        &["--source-root", "/source", "--cloud-root", "/cloud", "--destination-subdirectory", "safe", "--live-icloud-capacity", "--stale-after-days", "0"],
        &["--source-root", "/source", "--cloud-root", "/cloud", "--destination-subdirectory", "safe", "--live-icloud-capacity", "--stale-after-days", "not-a-number"],
        &["--source-root", "/source", "--cloud-root", "/cloud", "--destination-subdirectory", "safe", "--live-icloud-capacity", "--capacity-reserve-mib", "not-a-number"],
        &["--source-root", "/source", "--cloud-root", "/cloud", "--destination-subdirectory", "safe", "--live-icloud-capacity", "--capacity-reserve-mib", "1048577"],
        &["--help", "--opaque-secret-payload"],
    ];
    for case in cases {
        assert_rejected(binary, case);
    }
}

#[test]
fn materialize_parser_rejects_duplicate_missing_unbounded_and_unsafe_inputs() {
    let binary = env!("CARGO_BIN_EXE_disksage-incomplete-download-materialize");
    let valid_prefix: &[&str] = &[
        "--source-root", "/source",
        "--destination-plan", "/private/plan.json",
        "--confirm-plan-fingerprint", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "--receipt-dir", "/private/receipts",
        "--approved-by", "human:test",
        "--rationale", "approved exact plan",
        "--execute",
    ];

    for case in [
        vec![],
        vec!["--source-root"],
        vec!["--source-root", "/a", "--source-root", "/b"],
        vec!["--source-root", "relative"],
    ] {
        assert_rejected(binary, &case);
    }

    let mut cases = Vec::<Vec<&str>>::new();
    let mut no_capacity = valid_prefix.to_vec();
    cases.push(no_capacity.clone());

    no_capacity.extend(["--live-icloud-capacity", "--capacity-snapshot", "/capacity.json"]);
    cases.push(no_capacity);

    for suffix in [
        vec!["--live-icloud-capacity", "--live-icloud-capacity"],
        vec!["--capacity-snapshot", "/a", "--capacity-snapshot", "/b"],
        vec!["--capacity-snapshot", "relative.json"],
        vec!["--live-icloud-capacity", "--max-entries", "0"],
        vec!["--live-icloud-capacity", "--max-entries", "not-a-number"],
        vec!["--live-icloud-capacity", "--stale-after-days", "0"],
        vec!["--live-icloud-capacity", "--stale-after-days", "not-a-number"],
        vec!["--execute", "--live-icloud-capacity"],
    ] {
        let mut args = valid_prefix.to_vec();
        args.extend(suffix);
        cases.push(args);
    }

    let mut bad_hex = valid_prefix.to_vec();
    let fingerprint = bad_hex.iter().position(|value| *value == "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
    bad_hex[fingerprint] = "ABC";
    bad_hex.push("--live-icloud-capacity");
    cases.push(bad_hex);

    let mut bad_attribution = valid_prefix.to_vec();
    let attribution = bad_attribution.iter().position(|value| *value == "human:test").unwrap();
    bad_attribution[attribution] = "agent:test";
    bad_attribution.push("--live-icloud-capacity");
    cases.push(bad_attribution);

    let mut relative_plan = valid_prefix.to_vec();
    let plan = relative_plan.iter().position(|value| *value == "/private/plan.json").unwrap();
    relative_plan[plan] = "relative.json";
    relative_plan.push("--live-icloud-capacity");
    cases.push(relative_plan);

    let mut relative_receipt = valid_prefix.to_vec();
    let receipt = relative_receipt.iter().position(|value| *value == "/private/receipts").unwrap();
    relative_receipt[receipt] = "relative";
    relative_receipt.push("--live-icloud-capacity");
    cases.push(relative_receipt);

    cases.push(vec!["--help", "--opaque-secret-payload"]);

    for case in cases {
        assert_rejected(binary, &case);
    }
}
