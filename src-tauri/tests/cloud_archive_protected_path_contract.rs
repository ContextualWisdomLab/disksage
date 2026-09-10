use disksage_lib::cloud::collect_archive_files_bounded;
use std::time::Duration;

#[test]
fn archive_scan_prunes_explicitly_protected_subtrees() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let protected = source.join("protected");
    let ordinary = source.join("ordinary");
    std::fs::create_dir_all(&protected).unwrap();
    std::fs::create_dir_all(&ordinary).unwrap();
    std::fs::write(protected.join(".disksage-protected"), b"retain\n").unwrap();
    std::fs::write(protected.join("protected-report.pdf"), b"protected").unwrap();
    std::fs::write(ordinary.join("ordinary-report.pdf"), b"ordinary").unwrap();

    let collection = collect_archive_files_bounded(
        &source,
        &[],
        100,
        Duration::from_secs(30),
    );

    assert!(collection.complete, "unexpected scan blockers: {:?}", collection.stop_reasons);
    let paths: Vec<_> = collection.files.iter().map(|file| file.path.as_path()).collect();
    assert!(paths.contains(&ordinary.join("ordinary-report.pdf").as_path()));
    assert!(
        !paths.contains(&protected.join("protected-report.pdf").as_path()),
        "explicitly protected content must never become a cloud archive candidate"
    );
}
