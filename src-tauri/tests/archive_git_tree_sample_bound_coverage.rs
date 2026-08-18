use std::fs::File;
use std::io::Write;

use disksage_lib::archive_git_tree::{compare_zip_content_inclusion, ArchiveTreeRootMode};
use zip::write::SimpleFileOptions;

fn many_file_zip(prefix: &str, count: usize) -> tempfile::TempDir {
    many_file_zip_with_content(prefix, count, b"x")
}

fn many_file_zip_with_content(prefix: &str, count: usize, content: &[u8]) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let file = File::create(temp.path().join("fixture.zip")).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    for index in 0..count {
        archive
            .start_file(
                format!("{prefix}-{index:04}.txt"),
                SimpleFileOptions::default().unix_permissions(0o100644),
            )
            .unwrap();
        archive.write_all(content).unwrap();
    }
    archive.finish().unwrap();
    temp
}

fn changed_and_additional_zip(changed_count: usize, additional_count: usize) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let file = File::create(temp.path().join("fixture.zip")).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    for index in 0..changed_count {
        archive
            .start_file(
                format!("changed-{index:04}.txt"),
                SimpleFileOptions::default().unix_permissions(0o100644),
            )
            .unwrap();
        archive.write_all(b"new").unwrap();
    }
    for index in 0..additional_count {
        archive
            .start_file(
                format!("additional-{index:04}.txt"),
                SimpleFileOptions::default().unix_permissions(0o100644),
            )
            .unwrap();
        archive.write_all(b"extra").unwrap();
    }
    archive.finish().unwrap();
    temp
}

#[test]
fn inclusion_report_bounds_missing_path_samples_without_hiding_total_failures() {
    // The production report intentionally caps path samples at 1,000 while retaining complete
    // counts. Exercise the first omitted path so oversized evidence stays bounded without becoming
    // a false-green inclusion result.
    let subset = many_file_zip("required", 1_001);
    let superset = many_file_zip("different", 1);

    let report = compare_zip_content_inclusion(
        &subset.path().join("fixture.zip"),
        &superset.path().join("fixture.zip"),
        ArchiveTreeRootMode::KeepTopLevel,
    )
    .unwrap();

    assert!(!report.subset_content_included);
    assert!(!report.archives_identical);
    assert_eq!(report.missing_file_count, 1_001);
    assert_eq!(report.missing_paths.len(), 1_000);
    assert_eq!(report.missing_paths.first().map(String::as_str), Some("required-0000.txt"));
    assert_eq!(report.missing_paths.last().map(String::as_str), Some("required-0999.txt"));
    assert!(!report.missing_paths.iter().any(|path| path == "required-1000.txt"));
    assert_eq!(report.additional_file_count, 1);
    assert_eq!(report.additional_paths, ["different-0000.txt"]);
    assert!(report.paths_truncated);
}

#[test]
fn inclusion_report_bounds_changed_and_additional_samples_independently() {
    // Missing-path truncation is a separate branch from changed/additional evidence. Drive both
    // remaining bounded collectors past their cap while preserving exact complete counts.
    let subset = many_file_zip_with_content("changed", 1_001, b"old");
    let superset = changed_and_additional_zip(1_001, 1_001);

    let report = compare_zip_content_inclusion(
        &subset.path().join("fixture.zip"),
        &superset.path().join("fixture.zip"),
        ArchiveTreeRootMode::KeepTopLevel,
    )
    .unwrap();

    assert_eq!(report.missing_file_count, 0);
    assert!(report.missing_paths.is_empty());
    assert_eq!(report.changed_file_count, 1_001);
    assert_eq!(report.changed_paths.len(), 1_000);
    assert_eq!(report.changed_paths.first().map(String::as_str), Some("changed-0000.txt"));
    assert_eq!(report.changed_paths.last().map(String::as_str), Some("changed-0999.txt"));
    assert!(!report.changed_paths.iter().any(|path| path == "changed-1000.txt"));
    assert_eq!(report.additional_file_count, 1_001);
    assert_eq!(report.additional_paths.len(), 1_000);
    assert_eq!(
        report.additional_paths.first().map(String::as_str),
        Some("additional-0000.txt")
    );
    assert_eq!(
        report.additional_paths.last().map(String::as_str),
        Some("additional-0999.txt")
    );
    assert!(!report
        .additional_paths
        .iter()
        .any(|path| path == "additional-1000.txt"));
    assert!(!report.subset_content_included);
    assert!(!report.archives_identical);
    assert!(report.paths_truncated);
}
