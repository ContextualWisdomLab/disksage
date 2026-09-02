use disksage_lib::cache_cleanup::{
    proven_cache_trash_candidates, purge_proven_cache_trash,
};
use std::fs;

#[test]
fn permanent_cache_trash_public_execution_fails_closed() {
    let tmp = tempfile::tempdir().expect("create isolated home");
    let cache = tmp.path().join(".Trash").join("_cacache");
    let reviewed_file = cache.join("content-v2").join("reviewed.bin");
    fs::create_dir_all(cache.join("content-v2")).expect("create npm cache content directory");
    fs::create_dir_all(cache.join("tmp")).expect("create npm cache temp directory");
    fs::write(&reviewed_file, b"reviewed-cache").expect("write reviewed cache fixture");

    let approved = proven_cache_trash_candidates(tmp.path());
    assert_eq!(approved.len(), 1, "fixture must reach the proven-cache boundary");

    let journal = tmp.path().join("purge-journal.jsonl");
    let error = purge_proven_cache_trash(tmp.path(), &approved, &journal, 7)
        .expect_err("irreversible cache purge must fail closed until race-safe deletion exists");

    assert_eq!(
        error,
        "cache-trash-permanent-purge-disabled-until-race-safe"
    );
    assert!(
        cache.exists(),
        "a disabled irreversible boundary must leave the reviewed directory intact"
    );
    assert_eq!(
        fs::read(&reviewed_file).expect("reviewed cache fixture must remain readable"),
        b"reviewed-cache",
        "a disabled irreversible boundary must preserve the exact reviewed bytes"
    );
    assert!(
        !journal.exists(),
        "a rejected purge must not emit a pending mutation receipt"
    );
}
