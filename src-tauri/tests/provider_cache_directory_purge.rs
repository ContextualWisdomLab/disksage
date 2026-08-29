#![cfg(target_os = "macos")]

use disksage_lib::provider_cache_reclaim::{
    execute, plan_with_runtime, ProviderCacheCleanupMode, ProviderCacheCleanupRequest,
    ProviderCacheKind,
};
use std::fs;
use std::path::{Path, PathBuf};

fn write_edge_version(app: PathBuf, version: &str) {
    fs::create_dir_all(app.join("Contents")).expect("create Edge fixture");
    let mut dictionary = plist::Dictionary::new();
    dictionary.insert("CFBundleShortVersionString".into(), version.into());
    plist::Value::Dictionary(dictionary)
        .to_file_xml(app.join("Contents/Info.plist"))
        .expect("write Edge version fixture");
}

#[test]
fn permanent_directory_purge_removes_an_unchanged_exact_edge_cache() {
    let temp = tempfile::tempdir().expect("temporary provider-cache fixture");
    let home = temp.path().join("home");
    let applications = temp.path().join("Applications");
    write_edge_version(applications.join("Microsoft Edge.app"), "2.0");
    let stale_cache = home.join(
        "Library/Application Support/Microsoft/EdgeUpdater/apps/msedge-stable/1.0",
    );
    write_edge_version(stale_cache.join("Microsoft Edge.app"), "1.0");
    fs::write(stale_cache.join("first.bin"), b"first").expect("write first cache child");
    fs::write(stale_cache.join("second.bin"), b"second").expect("write second cache child");

    let plan = plan_with_runtime(
        &home,
        &applications,
        Path::new("/missing/podman"),
        1,
    );
    assert!(plan.evidence_complete, "{:?}", plan.issues);
    let candidate = plan
        .candidates
        .iter()
        .find(|candidate| candidate.kind == ProviderCacheKind::EdgeSupersededInstalledCopy)
        .expect("stale Edge cache candidate");
    let request = ProviderCacheCleanupRequest {
        path: candidate.path.clone(),
        evidence_fingerprint: candidate.evidence_fingerprint.clone(),
        object_id: candidate.object_id.clone(),
    };
    let data = temp.path().join("data");
    fs::create_dir_all(&data).expect("create audit data directory");

    let result = execute(
        &home,
        &applications,
        Path::new("/missing/podman"),
        &[request],
        &plan.plan_fingerprint,
        &plan.plan_fingerprint,
        plan.exact_approval_phrase
            .as_deref()
            .expect("permanent approval phrase"),
        "verified regenerable provider cache",
        &data.join("journal.jsonl"),
        &data.join("receipts"),
        ProviderCacheCleanupMode::PermanentPurge,
        2,
    )
    .expect("cleanup result envelope");

    assert_eq!(result.completed_count, 1);
    assert_eq!(result.items.len(), 1);
    assert!(result.items[0].completed);
    assert!(result.items[0].error.is_none());
    assert!(!stale_cache.exists());
    assert!(Path::new(&result.immutable_receipt_path).is_file());
}
