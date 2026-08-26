#![cfg(target_os = "macos")]

use std::fs;

use disksage_lib::cache_cleanup::proven_cache_trash_candidates;
use disksage_lib::cache_trash_reclaim::{
    approval_phrase_for_candidates, purge_approved_cache_trash,
};

fn create_npm_cache(trash: &std::path::Path) {
    let cache = trash.join("_cacache");
    fs::create_dir_all(cache.join("content-v2")).unwrap();
    fs::create_dir(cache.join("tmp")).unwrap();
    fs::write(cache.join("content-v2").join("entry"), b"cache").unwrap();
}

fn create_trivy_cache(trash: &std::path::Path) {
    let cache = trash.join("db");
    fs::create_dir(&cache).unwrap();
    fs::write(cache.join("trivy.db"), b"db").unwrap();
    fs::write(cache.join("metadata.json"), b"{}").unwrap();
}

#[test]
fn purge_never_expands_beyond_reviewed_snapshot_or_uses_path_only_delete() {
    let home = tempfile::tempdir().unwrap();
    let trash = home.path().join(".Trash");
    fs::create_dir(&trash).unwrap();
    create_npm_cache(&trash);

    let reviewed = proven_cache_trash_candidates(home.path());
    assert_eq!(reviewed.len(), 1);
    assert_eq!(reviewed[0].name, "_cacache");
    let reviewed_phrase = approval_phrase_for_candidates(&reviewed);

    // This second structurally valid cache appears only after the operator-reviewed snapshot.
    // Revalidation must never expand authority to it, and until the irreversible removal primitive
    // itself is object-bound the reviewed cache must also be preserved rather than deleted by path.
    create_trivy_cache(&trash);
    let current = proven_cache_trash_candidates(home.path());
    assert_eq!(current.len(), 2);
    assert_ne!(reviewed_phrase, approval_phrase_for_candidates(&current));

    let journal = home.path().join("journal.jsonl");
    let results = purge_approved_cache_trash(
        home.path(),
        &reviewed,
        &reviewed_phrase,
        &journal,
        7,
    )
    .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "_cacache");
    assert!(!results[0].purged);
    assert_eq!(
        results[0].error,
        "cache-trash-identity-bound-permanent-delete-unavailable"
    );
    assert!(trash.join("_cacache").exists());
    assert!(
        !results.iter().any(|item| item.name == "db"),
        "the unreviewed cache must never enter the execution result or deletion authority"
    );
    assert!(
        trash.join("db").exists(),
        "the unreviewed cache candidate must remain in Trash"
    );
}
