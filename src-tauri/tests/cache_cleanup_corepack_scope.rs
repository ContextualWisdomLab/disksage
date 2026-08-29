#[cfg(target_os = "macos")]
#[test]
fn automatic_node_cleanup_reclaims_only_corepack_subtree() {
    use std::fs;
    use std::process::Command;

    let tmp = tempfile::tempdir().expect("create isolated cache-cleanup fixture");
    let home = tmp.path().join("home");
    let cache_home = tmp.path().join("cache");
    let data_home = tmp.path().join("data");
    let temp_root = tmp.path().join("tmp");
    let corepack_archive = cache_home.join("node/corepack/archive");
    let unrelated_node_cache = cache_home.join("node/unrelated-tool/state");
    let journal = tmp.path().join("journal.jsonl");

    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&data_home).unwrap();
    fs::create_dir_all(&temp_root).unwrap();
    fs::create_dir_all(&corepack_archive).unwrap();
    fs::create_dir_all(&unrelated_node_cache).unwrap();
    fs::write(corepack_archive.join("package.tgz"), b"regenerable corepack cache").unwrap();
    fs::write(unrelated_node_cache.join("keep.bin"), b"unrelated node tooling state").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_disksage-cache-cleanup"))
        .env("HOME", &home)
        .env("XDG_CACHE_HOME", &cache_home)
        .env("XDG_DATA_HOME", &data_home)
        .env("TMPDIR", &temp_root)
        .arg("--execute")
        .arg("--journal-path")
        .arg(&journal)
        .output()
        .expect("run production cache-cleanup binary");

    assert!(
        output.status.success(),
        "cache cleanup failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !corepack_archive.exists(),
        "Corepack cache archive should be reclaimed while retaining its catalog root"
    );
    assert!(
        cache_home.join("node/corepack").is_dir(),
        "automatic cleanup must retain the Corepack catalog root"
    );
    assert_eq!(
        fs::read(unrelated_node_cache.join("keep.bin")).unwrap(),
        b"unrelated node tooling state",
        "automatic cleanup must not touch unrelated Node tooling data"
    );
}
