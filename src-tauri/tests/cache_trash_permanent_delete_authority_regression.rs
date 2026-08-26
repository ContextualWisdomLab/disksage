#![cfg(target_os = "macos")]

use std::fs;

use disksage_lib::cache_cleanup::proven_cache_trash_candidates;
use disksage_lib::cache_trash_reclaim::{
    approval_phrase_for_candidates, purge_approved_cache_trash,
};

#[test]
fn reviewed_cache_remains_when_final_object_bound_delete_is_unavailable() {
    let home = tempfile::tempdir().unwrap();
    let trash = home.path().join(".Trash");
    let cache = trash.join("_cacache");
    fs::create_dir_all(cache.join("content-v2")).unwrap();
    fs::create_dir(cache.join("tmp")).unwrap();
    fs::write(cache.join("content-v2").join("entry"), b"cache").unwrap();

    let reviewed = proven_cache_trash_candidates(home.path());
    assert_eq!(reviewed.len(), 1);
    let phrase = approval_phrase_for_candidates(&reviewed);
    let journal = home.path().join("journal.jsonl");

    let results = purge_approved_cache_trash(home.path(), &reviewed, &phrase, &journal, 11).unwrap();

    assert_eq!(results.len(), 1);
    assert!(!results[0].purged);
    assert_eq!(
        results[0].error,
        "cache-trash-identity-bound-permanent-delete-unavailable"
    );
    assert!(
        cache.join("content-v2").join("entry").exists(),
        "DiskSage must preserve the reviewed cache until the irreversible deletion primitive itself is object-bound"
    );
}
