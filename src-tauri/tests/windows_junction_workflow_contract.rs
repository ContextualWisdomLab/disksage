const TEST_WORKFLOW: &str = include_str!("../../.github/workflows/test.yml");

#[test]
fn windows_junction_regression_is_executed_on_a_windows_runner() {
    assert!(
        TEST_WORKFLOW.contains(
            "cargo test --manifest-path src-tauri/Cargo.toml --test windows_junction_no_follow"
        ),
        "the Windows junction no-follow regression must run on the hosted Windows Test job"
    );
}
