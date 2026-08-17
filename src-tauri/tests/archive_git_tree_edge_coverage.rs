use std::fs::File;
use std::io::Write;

use disksage_lib::archive_git_tree::{
    compare_zip_content_inclusion, inspect_zip_git_tree, inspect_zip_git_tree_with_mode,
    ArchiveTreeRootMode,
};
use zip::write::SimpleFileOptions;

fn empty_zip() -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let file = File::create(temp.path().join("fixture.zip")).unwrap();
    zip::ZipWriter::new(file).finish().unwrap();
    temp
}

fn single_file_zip(path: &str, contents: &[u8]) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let file = File::create(temp.path().join("fixture.zip")).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    archive
        .start_file(
            path,
            SimpleFileOptions::default().unix_permissions(0o100644),
        )
        .unwrap();
    archive.write_all(contents).unwrap();
    archive.finish().unwrap();
    temp
}

fn wrapped_file_zip(path: &str, contents: &[u8]) -> tempfile::TempDir {
    single_file_zip(&format!("repo/{path}"), contents)
}

#[test]
fn inspection_validates_expected_tree_before_touching_the_archive() {
    let missing = std::env::temp_dir().join("disksage-archive-does-not-exist.zip");
    assert_eq!(
        inspect_zip_git_tree(&missing, Some("not-a-tree")).unwrap_err(),
        "expected-git-tree-sha1-invalid"
    );
    assert_eq!(
        inspect_zip_git_tree(&missing, None).unwrap_err(),
        "archive-open-failed"
    );
}

#[test]
fn inspection_rejects_malformed_and_empty_zip_containers() {
    let malformed = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(malformed.path(), b"not a zip central directory").unwrap();
    assert_eq!(
        inspect_zip_git_tree(malformed.path(), None).unwrap_err(),
        "archive-central-directory-invalid"
    );

    let empty = empty_zip();
    assert_eq!(
        inspect_zip_git_tree_with_mode(
            &empty.path().join("fixture.zip"),
            None,
            ArchiveTreeRootMode::KeepTopLevel,
        )
        .unwrap_err(),
        "archive-entry-count-out-of-bounds"
    );
}

#[test]
fn strip_shared_root_rejects_a_root_level_file_with_no_relative_path() {
    let root_file = single_file_zip("repo", b"payload");
    assert_eq!(
        inspect_zip_git_tree(&root_file.path().join("fixture.zip"), None).unwrap_err(),
        "archive-entry-empty-relative-path"
    );
}

#[test]
fn directory_only_archive_has_no_git_files() {
    let temp = tempfile::tempdir().unwrap();
    let file = File::create(temp.path().join("fixture.zip")).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    archive
        .add_directory("repo/", SimpleFileOptions::default())
        .unwrap();
    archive.finish().unwrap();

    assert_eq!(
        inspect_zip_git_tree(&temp.path().join("fixture.zip"), None).unwrap_err(),
        "archive-no-git-files"
    );
}

#[test]
fn expected_tree_mismatch_is_reported_without_rejecting_valid_content() {
    let fixture = wrapped_file_zip("a.txt", b"hello\n");
    let report = inspect_zip_git_tree(
        &fixture.path().join("fixture.zip"),
        Some("0000000000000000000000000000000000000000"),
    )
    .unwrap();

    assert_eq!(report.matches_expected, Some(false));
    assert_eq!(
        report.expected_git_tree_sha1.as_deref(),
        Some("0000000000000000000000000000000000000000")
    );
    assert_eq!(report.file_count, 1);
}

#[test]
fn identical_archives_are_proven_as_identical_inclusion() {
    let subset = single_file_zip("a.txt", b"same bytes");
    let superset = single_file_zip("a.txt", b"same bytes");
    let report = compare_zip_content_inclusion(
        &subset.path().join("fixture.zip"),
        &superset.path().join("fixture.zip"),
        ArchiveTreeRootMode::KeepTopLevel,
    )
    .unwrap();

    assert!(report.subset_content_included);
    assert!(report.archives_identical);
    assert_eq!(report.matching_file_count, 1);
    assert_eq!(report.additional_file_count, 0);
    assert!(report.missing_paths.is_empty());
    assert!(report.changed_paths.is_empty());
    assert!(report.additional_paths.is_empty());
}

#[test]
fn file_then_nested_file_path_conflict_reaches_the_production_guard() {
    let temp = tempfile::tempdir().unwrap();
    let file = File::create(temp.path().join("fixture.zip")).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    archive
        .start_file(
            "same.txt",
            SimpleFileOptions::default().unix_permissions(0o100644),
        )
        .unwrap();
    archive.write_all(b"first").unwrap();
    archive
        .start_file(
            "same.txt/child.bin",
            SimpleFileOptions::default().unix_permissions(0o100644),
        )
        .unwrap();
    archive.write_all(b"second").unwrap();
    archive.finish().unwrap();

    assert_eq!(
        inspect_zip_git_tree_with_mode(
            &temp.path().join("fixture.zip"),
            None,
            ArchiveTreeRootMode::KeepTopLevel,
        )
        .unwrap_err(),
        "archive-entry-file-directory-conflict"
    );
}
