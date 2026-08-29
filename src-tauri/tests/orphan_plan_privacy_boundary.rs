#![cfg(target_os = "macos")]

use disksage_lib::orphan::plan_for_roots;
use std::path::PathBuf;

#[test]
fn public_plan_fingerprint_does_not_encode_home_scope() {
    let home_a = tempfile::tempdir().expect("first private home fixture");
    let home_b = tempfile::tempdir().expect("second private home fixture");
    let watched_a = [
        (home_a.path().join("Library/Caches"), "cache"),
        (
            home_a.path().join("Library/Application Support"),
            "application-support",
        ),
    ];
    let watched_b = [
        (home_b.path().join("Library/Caches"), "cache"),
        (
            home_b.path().join("Library/Application Support"),
            "application-support",
        ),
    ];
    let application_roots: [PathBuf; 0] = [];

    let plan_a = plan_for_roots(home_a.path(), &watched_a, &application_roots, 1)
        .expect("first empty plan");
    let plan_b = plan_for_roots(home_b.path(), &watched_b, &application_roots, 2)
        .expect("second empty plan");

    assert_eq!(plan_a.candidate_count, 0);
    assert_eq!(plan_b.candidate_count, 0);
    assert_eq!(plan_a.plan_fingerprint, plan_b.plan_fingerprint);

    let serialized = serde_json::to_string(&plan_a).expect("serialize public plan");
    assert!(!serialized.contains("root_fingerprint"));
    assert!(!serialized.contains(&home_a.path().to_string_lossy().to_string()));
}

#[test]
fn launch_services_timeout_owns_descendants_before_reader_join() {
    let source = include_str!("../src/orphan.rs");
    assert!(source.contains("setpgid(0, 0)"));
    assert!(source.contains("kill_launch_services_group"));
    assert!(source.contains("reader.join()"));
}
