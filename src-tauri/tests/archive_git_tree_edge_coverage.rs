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

fn multi_file_zip(files: &[(&str, &[u8], u32)]) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let file = File::create(temp.path().join("fixture.zip")).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    for (path, contents, mode) in files {
        archive
            .start_file(
                *path,
                SimpleFileOptions::default().unix_permissions(*mode),
            )
            .unwrap();
        archive.write_all(contents).unwrap();
    }
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
fn valid_expected_tree_is_trimmed_lowercased_and_matches_the_computed_tree() {
    let fixture = wrapped_file_zip("a.txt", b"hello\n");
    let archive_path = fixture.path().join("fixture.zip");
    let baseline = inspect_zip_git_tree(&archive_path, None).unwrap();
    let decorated = format!("  {}  ", baseline.git_tree_sha1.to_ascii_uppercase());

    let report = inspect_zip_git_tree(&archive_path, Some(&decorated)).unwrap();

    assert_eq!(
        report.expected_git_tree_sha1.as_deref(),
        Some(baseline.git_tree_sha1.as_str())
    );
    assert_eq!(report.matches_expected, Some(true));
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
fn changed_and_additional_files_are_distinguished_from_matching_content() {
    let subset = multi_file_zip(&[
        ("same.txt", b"same", 0o100644),
        ("changed.txt", b"old", 0o100644),
    ]);
    let superset = multi_file_zip(&[
        ("same.txt", b"same", 0o100644),
        ("changed.txt", b"new", 0o100644),
        ("extra.txt", b"extra", 0o100644),
    ]);

    let report = compare_zip_content_inclusion(
        &subset.path().join("fixture.zip"),
        &superset.path().join("fixture.zip"),
        ArchiveTreeRootMode::KeepTopLevel,
    )
    .unwrap();

    assert_eq!(report.matching_file_count, 1);
    assert_eq!(report.missing_file_count, 0);
    assert_eq!(report.changed_file_count, 1);
    assert_eq!(report.additional_file_count, 1);
    assert_eq!(report.changed_paths, ["changed.txt"]);
    assert_eq!(report.additional_paths, ["extra.txt"]);
    assert!(!report.subset_content_included);
    assert!(!report.archives_identical);
    assert!(!report.paths_truncated);
    assert_ne!(report.subset_manifest_sha256, report.superset_manifest_sha256);
}

#[test]
fn case_collisions_make_inclusion_evidence_ambiguous_and_fail_closed() {
    let subset = multi_file_zip(&[
        ("Readme.txt", b"first", 0o100644),
        ("README.txt", b"second", 0o100644),
    ]);
    let superset = single_file_zip("Readme.txt", b"first");

    assert_eq!(
        compare_zip_content_inclusion(
            &subset.path().join("fixture.zip"),
            &superset.path().join("fixture.zip"),
            ArchiveTreeRootMode::KeepTopLevel,
        )
        .unwrap_err(),
        "archive-case-collision-ambiguous"
    );
}

#[test]
fn executable_and_regular_files_are_both_representable_in_the_git_tree() {
    let fixture = multi_file_zip(&[
        ("script.sh", b"#!/bin/sh\nexit 0\n", 0o100755),
        ("notes.txt", b"plain\n", 0o100644),
    ]);

    let report = inspect_zip_git_tree_with_mode(
        &fixture.path().join("fixture.zip"),
        None,
        ArchiveTreeRootMode::KeepTopLevel,
    )
    .unwrap();

    assert_eq!(report.file_count, 2);
    assert_eq!(report.root_prefix, ".");
    assert_eq!(report.matches_expected, None);
}

#[test]
fn mixed_shared_roots_are_rejected_before_a_tree_can_be_attested() {
    let fixture = multi_file_zip(&[
        ("repo/a.txt", b"a", 0o100644),
        ("other/b.txt", b"b", 0o100644),
    ]);

    assert_eq!(
        inspect_zip_git_tree(&fixture.path().join("fixture.zip"), None).unwrap_err(),
        "archive-shared-root-mismatch"
    );
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

#[test]
fn nested_file_then_parent_file_path_conflict_is_rejected_without_overwrite() {
    let fixture = multi_file_zip(&[
        ("same.txt/child.bin", b"nested", 0o100644),
        ("same.txt", b"parent", 0o100644),
    ]);

    assert_eq!(
        inspect_zip_git_tree_with_mode(
            &fixture.path().join("fixture.zip"),
            None,
            ArchiveTreeRootMode::KeepTopLevel,
        )
        .unwrap_err(),
        "archive-entry-duplicate-or-type-conflict"
    );
}

#[test]
fn hostile_archive_paths_fail_closed_at_the_public_inspection_boundary() {
    let cases = [
        ("/absolute.txt", "archive-entry-path-unsafe"),
        ("parent/../escape.txt", "archive-entry-path-unsafe"),
        ("double//separator.txt", "archive-entry-path-unsafe"),
        ("dot/./component.txt", "archive-entry-path-unsafe"),
        ("back\\slash.txt", "archive-entry-path-unsafe"),
    ];

    for (path, expected) in cases {
        let fixture = single_file_zip(path, b"payload");
        assert_eq!(
            inspect_zip_git_tree_with_mode(
                &fixture.path().join("fixture.zip"),
                None,
                ArchiveTreeRootMode::KeepTopLevel,
            )
            .unwrap_err(),
            expected,
            "path={path}"
        );
    }

    let overlong = format!("{}.txt", "a".repeat(4_096));
    let fixture = single_file_zip(&overlong, b"payload");
    assert_eq!(
        inspect_zip_git_tree_with_mode(
            &fixture.path().join("fixture.zip"),
            None,
            ArchiveTreeRootMode::KeepTopLevel,
        )
        .unwrap_err(),
        "archive-entry-path-invalid"
    );
}

#[test]
fn slash_suffixed_file_with_payload_is_rejected_as_a_malformed_directory_entry() {
    let temp = tempfile::tempdir().unwrap();
    let file = File::create(temp.path().join("fixture.zip")).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    archive
        .start_file(
            "pretend-directory/",
            SimpleFileOptions::default().unix_permissions(0o100644),
        )
        .unwrap();
    archive.write_all(b"payload").unwrap();
    archive.finish().unwrap();

    assert_eq!(
        inspect_zip_git_tree_with_mode(
            &temp.path().join("fixture.zip"),
            None,
            ArchiveTreeRootMode::KeepTopLevel,
        )
        .unwrap_err(),
        "archive-directory-entry-has-payload"
    );
}
