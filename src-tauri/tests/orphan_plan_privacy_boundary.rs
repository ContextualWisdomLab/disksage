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
fn installed_reverse_dns_bundle_id_is_not_limited_to_a_small_prefix_allowlist() {
    let home = tempfile::tempdir().expect("private home fixture");
    let applications = home.path().join("Applications");
    let contents = applications.join("Editor.app/Contents");
    let caches = home.path().join("Library/Caches/dev.example.editor");
    std::fs::create_dir_all(&contents).expect("create app bundle");
    std::fs::create_dir_all(&caches).expect("create matching cache");
    std::fs::write(
        contents.join("Info.plist"),
        br#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>CFBundleIdentifier</key><string>dev.example.editor</string></dict></plist>"#,
    )
    .expect("write Info.plist");

    let watched = [
        (home.path().join("Library/Caches"), "cache"),
        (
            home.path().join("Library/Application Support"),
            "application-support",
        ),
    ];
    let plan = plan_for_roots(home.path(), &watched, &[applications], 1)
        .expect("plan with non-com prefix installed app");

    assert!(plan.scan_complete);
    assert_eq!(plan.candidate_count, 0);
}
