use std::fs;

use disksage_lib::cache_cleanup::{
    proven_cache_trash_snapshot, purge_proven_cache_trash,
};

#[cfg(not(windows))]
fn trash_directory(home: &std::path::Path) -> std::path::PathBuf {
    #[cfg(target_os = "macos")]
    {
        return home.join(".Trash");
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        home.join(".local").join("share").join("Trash").join("files")
    }
}

#[cfg(not(windows))]
fn create_npm_cache(trash: &std::path::Path) {
    let cache = trash.join("_cacache");
    fs::create_dir_all(cache.join("content-v2")).unwrap();
    fs::create_dir(cache.join("tmp")).unwrap();
    fs::write(cache.join("content-v2").join("entry"), b"cache").unwrap();
}

#[cfg(not(windows))]
fn create_trivy_cache(trash: &std::path::Path) {
    let cache = trash.join("db");
    fs::create_dir(&cache).unwrap();
    fs::write(cache.join("trivy.db"), b"db").unwrap();
    fs::write(cache.join("metadata.json"), b"{}").unwrap();
}

#[cfg(not(windows))]
#[test]
fn purge_never_deletes_candidate_added_after_reviewed_approval_snapshot() {
    let home = tempfile::tempdir().unwrap();
    let trash = trash_directory(home.path());
    fs::create_dir_all(&trash).unwrap();
    create_npm_cache(&trash);

    let snapshot = proven_cache_trash_snapshot(home.path());
    assert_eq!(snapshot.candidates.len(), 1);
    assert_eq!(snapshot.candidates[0].name, "_cacache");

    // This second structurally valid cache appears only after the operator-reviewed snapshot.
    // A safe purge may revalidate reviewed objects, but it must never expand deletion authority
    // by rescanning and deleting this newly appeared candidate.
    create_trivy_cache(&trash);
    assert_ne!(
        snapshot.approval_phrase,
        proven_cache_trash_snapshot(home.path()).approval_phrase
    );

    let journal = home.path().join("journal.jsonl");
    let results = purge_proven_cache_trash(home.path(), &journal, 7, &snapshot).unwrap();

    assert!(results.iter().any(|item| item.name == "_cacache" && item.purged));
    assert!(
        !results.iter().any(|item| item.name == "db" && item.purged),
        "purge must not authorize a structurally valid cache that appeared after the reviewed snapshot"
    );
    assert!(
        trash.join("db").exists(),
        "the unreviewed cache candidate must remain in Trash"
    );
}
