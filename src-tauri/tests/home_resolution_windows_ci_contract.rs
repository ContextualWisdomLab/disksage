//! Control-plane regression for the platform-specific absolute-home contract.
//!
//! `home_resolution_contract.rs` deliberately uses Windows path semantics under `cfg!(windows)`.
//! A Linux-only test job cannot prove that `C:\\...` is absolute to `std::path::PathBuf` on the
//! shipped Windows target. Keep one narrow Windows runner that executes the real regression.

#[test]
fn test_workflow_executes_home_resolution_contract_on_windows() {
    let workflow = include_str!("../../.github/workflows/test.yml");

    assert!(
        workflow.contains("windows-home-resolution:"),
        "test workflow must keep a dedicated Windows home-resolution job"
    );
    assert!(
        workflow.contains("runs-on: windows-latest"),
        "home-resolution contract must execute with Windows path semantics"
    );
    assert!(
        workflow.contains("rustc --edition=2021 --test src-tauri/tests/home_resolution_contract.rs"),
        "Windows job must execute the real home_resolution_contract.rs regression"
    );
}
