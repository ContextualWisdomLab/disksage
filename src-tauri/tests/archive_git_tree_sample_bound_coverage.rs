use std::fs::File;
use std::io::Write;

use disksage_lib::archive_git_tree::{compare_zip_content_inclusion, ArchiveTreeRootMode};
use zip::write::SimpleFileOptions;

fn many_file_zip(prefix: &str, count: usize) -> tempfile::TempDir {
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
        archive.write_all(b"x").unwrap();
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
