use std::fs;

use disksage_lib::cache_cleanup::CacheTrashCandidate;
use disksage_lib::cache_trash_reclaim::approval_phrase_for_candidates;

#[test]
fn approval_phrase_changes_when_reviewed_root_object_is_replaced() {
    let tmp = tempfile::tempdir().unwrap();
    let candidate_path = tmp.path().join("_cacache");
    fs::create_dir(&candidate_path).unwrap();

    let candidate = CacheTrashCandidate {
        name: "_cacache".into(),
        path: candidate_path.to_string_lossy().into_owned(),
        bytes: 0,
        signature: "npm-cacache".into(),
    };
    let reviewed_phrase = approval_phrase_for_candidates(&[candidate.clone()]);

    let original = tmp.path().join("reviewed-original");
    fs::rename(&candidate_path, &original).unwrap();
    fs::create_dir(&candidate_path).unwrap();

    assert_ne!(
        reviewed_phrase,
        approval_phrase_for_candidates(&[candidate]),
        "a pathname-compatible replacement must not retain the reviewed deletion authority",
    );
    assert!(original.exists());
    assert!(candidate_path.exists());
}
