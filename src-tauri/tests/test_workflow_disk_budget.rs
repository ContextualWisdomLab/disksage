//! Regression contracts for bounded GitHub-hosted runner disk and linker usage.
//!
//! The ordinary `Test` job deliberately compiles several large Rust feature
//! combinations before it reaches the frontend checks. Real exact-head runs
//! have exhausted hosted-runner resources while entering later feature
//! batches. These tests keep the repair reviewable: the workflow must release
//! reusable Cargo build space between large batches and must not compile every
//! integration-test target merely to exercise a focused feature-gated library
//! surface.

use std::fs;
use std::path::PathBuf;

fn workflow_source() -> String {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workflow_path = manifest_dir
        .parent()
        .expect("src-tauri must have a repository parent")
        .join(".github/workflows/test.yml");
    fs::read_to_string(workflow_path).expect("Test workflow must be readable")
}

#[test]
fn test_job_reclaims_rust_build_space_before_archive_feature_batch() {
    let workflow = workflow_source();
    let duplicate = workflow
        .find("- name: Exact duplicate audit tests")
        .expect("duplicate-audit batch must remain in the Test job");
    let reclaim = workflow
        .find("- name: Reclaim Rust test build space before archive proofs")
        .expect("Test job must reclaim Rust build space before archive proofs");
    let archive = workflow
        .find("- name: Extraction-free archive tree proof tests")
        .expect("archive proof batch must remain in the Test job");

    assert!(
        duplicate < reclaim && reclaim < archive,
        "disk reclamation must occur after duplicate tests and before archive tests"
    );

    let reclaim_block = &workflow[reclaim..archive];
    assert!(
        reclaim_block.contains("cargo clean --manifest-path src-tauri/Cargo.toml"),
        "reclamation must remove only Cargo build artifacts through cargo clean"
    );
    assert!(
        reclaim_block.contains("df -h ."),
        "reclamation must leave bounded disk-availability evidence in the job log"
    );
}

#[test]
fn duplicate_audit_feature_batch_builds_only_the_intended_library_and_cli_targets() {
    let workflow = workflow_source();
    let duplicate = workflow
        .find("- name: Exact duplicate audit tests")
        .expect("duplicate-audit batch must remain in the Test job");
    let reclaim = workflow
        .find("- name: Reclaim Rust test build space before archive proofs")
        .expect("duplicate-audit batch must remain bounded before reclamation");
    let duplicate_block = &workflow[duplicate..reclaim];

    assert!(
        duplicate_block.contains(
            "cargo test --locked --manifest-path src-tauri/Cargo.toml --lib --features cloud-cli duplicate_audit"
        ),
        "the cloud-cli duplicate-audit library proof must stay lockfile-bound and use --lib so Cargo does not relink every integration-test target"
    );
    assert!(
        duplicate_block.contains(
            "cargo test --locked --manifest-path src-tauri/Cargo.toml --features cloud-cli --bin disksage-duplicate-audit"
        ),
        "the dedicated duplicate-audit CLI proof must remain explicit and lockfile-bound"
    );
}

#[test]
fn archive_feature_batch_builds_only_the_intended_library_and_cli_targets() {
    let workflow = workflow_source();
    let archive = workflow
        .find("- name: Extraction-free archive tree proof tests")
        .expect("archive proof batch must remain in the Test job");
    let node_setup = workflow[archive..]
        .find("- uses: actions/setup-node@")
        .map(|offset| archive + offset)
        .expect("Node setup must remain after the archive proof batch");
    let archive_block = &workflow[archive..node_setup];

    assert!(
        archive_block.contains(
            "cargo test --locked --manifest-path src-tauri/Cargo.toml --lib --features archive-cli archive_git_tree"
        ),
        "the archive-cli library proof must stay lockfile-bound and use --lib so Cargo does not relink every integration-test target"
    );
    assert!(
        archive_block.contains(
            "cargo test --locked --manifest-path src-tauri/Cargo.toml --features archive-cli --bin disksage-archive-tree"
        ),
        "the dedicated archive-tree CLI proof must remain explicit and lockfile-bound"
    );
}
