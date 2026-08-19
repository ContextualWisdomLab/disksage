//! Repository contracts for DiskSage's reviewed Rust compiler baseline.

const RUST_TOOLCHAIN: &str = include_str!("../../rust-toolchain.toml");
const CARGO_MANIFEST: &str = include_str!("../Cargo.toml");
const TEST_WORKFLOW: &str = include_str!("../../.github/workflows/test.yml");
const RELEASE_WORKFLOW: &str = include_str!("../../.github/workflows/release.yml");
const DEPENDABOT: &str = include_str!("../../.github/dependabot.yml");

#[test]
fn local_package_and_ci_use_the_same_exact_compiler() {
    assert!(RUST_TOOLCHAIN.contains("channel = \"1.97.1\""));
    assert!(!RUST_TOOLCHAIN.contains("channel = \"stable\""));
    assert!(CARGO_MANIFEST.contains("rust-version = \"1.97.1\""));
    assert_eq!(TEST_WORKFLOW.matches("toolchain: 1.97.1").count(), 2);
    assert!(!TEST_WORKFLOW.contains("toolchain: stable"));
    assert!(!TEST_WORKFLOW.contains("rust-version = \"1.88\""));
}

#[test]
fn release_commands_remain_under_the_root_toolchain_override() {
    assert!(RELEASE_WORKFLOW.contains(
        "cargo build --manifest-path src-tauri/Cargo.toml --release --features cloud-cli"
    ));
    assert!(RELEASE_WORKFLOW.contains("npm run tauri -- build --features llm-engine"));
    assert!(!RELEASE_WORKFLOW.contains("working-directory: src-tauri"));
    assert!(!RELEASE_WORKFLOW.contains("toolchain: stable"));
}

#[test]
fn compiler_updates_are_reviewable() {
    assert!(DEPENDABOT.contains("package-ecosystem: \"rust-toolchain\""));
    assert!(DEPENDABOT.contains("interval: \"weekly\""));
}
