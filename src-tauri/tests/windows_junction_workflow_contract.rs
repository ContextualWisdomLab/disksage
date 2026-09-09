const TEST_WORKFLOW: &str = include_str!("../../.github/workflows/test.yml");

fn windows_home_resolution_job() -> &'static str {
    let (_, after_job) = TEST_WORKFLOW
        .split_once("  windows-home-resolution:")
        .expect("Windows regression job must exist");
    after_job
        .split_once("\n  macos-bound-root:")
        .map(|(job, _)| job)
        .unwrap_or(after_job)
}

#[test]
fn windows_junction_regression_is_executed_on_a_windows_runner() {
    assert!(
        windows_home_resolution_job().contains(
            "cargo test --manifest-path src-tauri/Cargo.toml --test windows_junction_no_follow --locked"
        ),
        "the Windows junction no-follow regression must run on the hosted Windows Test job"
    );
}

#[test]
fn windows_junction_regression_has_full_rust_test_timeout_budget() {
    assert!(
        windows_home_resolution_job().contains("    timeout-minutes: 30"),
        "the Windows junction regression performs a full Cargo compile and must not inherit a 10-minute cold-build timeout"
    );
}
