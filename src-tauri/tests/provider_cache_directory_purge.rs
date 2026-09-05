#![cfg(target_os = "macos")]

use disksage_lib::provider_cache::{plan_with_runtime, ProviderCacheKind};
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
fn public_plan_never_advertises_permanent_directory_purge() {
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
    assert!(plan
        .candidates
        .iter()
        .any(|candidate| candidate.kind == ProviderCacheKind::EdgeSupersededInstalledCopy));
    assert!(
        plan.exact_approval_phrase.is_none(),
        "public Rust planning must not mint irreversible provider-cache approval"
    );
    assert!(stale_cache.is_dir());
    assert_eq!(fs::read(stale_cache.join("first.bin")).unwrap(), b"first");
    assert_eq!(fs::read(stale_cache.join("second.bin")).unwrap(), b"second");
}
