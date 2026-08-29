use std::fs;

#[test]
fn nested_symlink_is_manifested_without_hiding_the_cache_target() {
    let temp = tempfile::tempdir().expect("temporary cache-manifest fixture");
    let cache_root = temp.path().join("cache-root");
    let target = cache_root.join("toolchain-jdk");
    let outside = temp.path().join("outside-user-data");
    fs::create_dir_all(target.join("bin")).expect("create generated cache target");
    fs::write(target.join("bin").join("java"), b"generated-runtime")
        .expect("write generated runtime fixture");
    fs::create_dir(&outside).expect("create outside fixture");
    fs::write(outside.join("keep.txt"), b"must-not-be-followed")
        .expect("write outside fixture");
    std::os::unix::fs::symlink(&outside, target.join("external-link"))
        .expect("create nested symlink fixture");

    let targets = crate::rules::cache_targets(&cache_root).expect("enumerate cache targets");

    assert_eq!(
        targets.len(),
        1,
        "a nested symlink must be represented without hiding the enclosing generated cache target"
    );
    assert_eq!(targets[0].path, target.to_string_lossy());
    assert_eq!(
        fs::read(outside.join("keep.txt")).expect("outside fixture must remain readable"),
        b"must-not-be-followed",
        "manifest construction must not follow or mutate a nested symlink target"
    );
}
