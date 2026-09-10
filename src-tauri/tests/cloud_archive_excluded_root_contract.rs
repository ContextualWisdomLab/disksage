use disksage_lib::cloud::collect_archive_files_bounded;
use std::time::Duration;

#[test]
fn archive_scan_prunes_explicit_excluded_roots_without_prefix_false_positives() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let excluded = source.join("excluded");
    let similarly_named = source.join("excluded-copy");
    let ordinary = source.join("ordinary");
    std::fs::create_dir_all(excluded.join("nested")).unwrap();
    std::fs::create_dir_all(&similarly_named).unwrap();
    std::fs::create_dir_all(&ordinary).unwrap();
    std::fs::write(excluded.join("nested/protected-report.pdf"), b"excluded").unwrap();
    std::fs::write(similarly_named.join("retained-report.pdf"), b"retained").unwrap();
    std::fs::write(ordinary.join("ordinary-report.pdf"), b"ordinary").unwrap();

    let collection = collect_archive_files_bounded(
        &source,
        std::slice::from_ref(&excluded),
        100,
        Duration::from_secs(30),
    );

    assert!(
        collection.complete,
        "unexpected scan blockers: {:?}",
        collection.stop_reasons
    );
    let paths: Vec<_> = collection.files.iter().map(|file| file.path.as_path()).collect();
    assert!(paths.contains(&ordinary.join("ordinary-report.pdf").as_path()));
    assert!(paths.contains(&similarly_named.join("retained-report.pdf").as_path()));
    assert!(
        !paths.contains(&excluded.join("nested/protected-report.pdf").as_path()),
        "an explicit excluded root must be pruned before archive candidates are collected"
    );
}
