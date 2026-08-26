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
fn purge_never_deletes_candidate_added_after_reviewed_approval_snapshot() {
    let home = tempfile::tempdir().unwrap();
    let trash = home.path().join(".Trash");
    fs::create_dir(&trash).unwrap();
    create_npm_cache(&trash);

    let reviewed = proven_cache_trash_candidates(home.path());
    assert_eq!(reviewed.len(), 1);
    assert_eq!(reviewed[0].name, "_cacache");
    let reviewed_phrase = approval_phrase_for_candidates(&reviewed);

    // This second structurally valid cache appears only after the operator-reviewed snapshot.
    // A safe purge may revalidate reviewed objects, but it must never expand deletion authority
    // by rescanning and deleting this newly appeared candidate.
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
    assert!(results.iter().any(|item| item.name == "_cacache" && item.purged));
    assert!(
        !results.iter().any(|item| item.name == "db"),
        "the unreviewed cache must never enter the execution result or deletion authority"
    );
    assert!(
        trash.join("db").exists(),
        "the unreviewed cache candidate must remain in Trash"
    );
}
