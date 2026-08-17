//! Regression contract for bounded GitHub-hosted runner disk usage.
//!
//! The ordinary `Test` job deliberately compiles several large Rust feature
//! combinations before it reaches the frontend checks. A real exact-head run
//! exhausted the hosted runner filesystem while entering the archive proof
//! stage. This test keeps the repair reviewable: the workflow must release the
//! reusable Cargo build directory after the duplicate-audit batch and before
//! compiling the archive feature batch, without deleting or skipping either
//! test family.

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
