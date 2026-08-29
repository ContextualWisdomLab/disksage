use disksage_lib::rules::cache_targets;
use std::fs;

#[test]
fn npm_cache_root_expands_npx_environments_independently() {
    let temp = tempfile::tempdir().expect("create isolated npm-cache fixture");
    let npm_cache = temp.path().join("npm-cache");
    fs::create_dir_all(npm_cache.join("_npx/live")).unwrap();
    fs::create_dir_all(npm_cache.join("_npx/inactive")).unwrap();
    fs::create_dir(npm_cache.join("_cacache")).unwrap();
    fs::write(npm_cache.join("_npx/live/package.json"), b"{}").unwrap();
    fs::write(npm_cache.join("_npx/inactive/package.json"), b"{}").unwrap();

    let targets = cache_targets(&npm_cache).expect("enumerate Windows-style npm cache root");

    assert_eq!(targets.len(), 3);
    assert!(targets.iter().any(|target| target.path.ends_with("_npx/live")));
    assert!(targets
        .iter()
        .any(|target| target.path.ends_with("_npx/inactive")));
    assert!(targets.iter().all(|target| !target.path.ends_with("_npx")));
}
