use std::fs::File;
use std::io::Write;

use disksage_lib::archive_git_tree::{
    compare_zip_content_inclusion, inspect_zip_git_tree_with_mode, ArchiveTreeRootMode,
};
use zip::write::SimpleFileOptions;

fn zip_with_entries(entries: &[(&str, &[u8])]) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let file = File::create(temp.path().join("fixture.zip")).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    for (path, contents) in entries {
        archive
            .start_file(
                *path,
                SimpleFileOptions::default().unix_permissions(0o100644),
            )
            .unwrap();
        archive.write_all(contents).unwrap();
    }
    archive.finish().unwrap();
    temp
}

#[test]
fn git_tree_ordering_handles_file_and_directory_prefixes_deterministically() {
    // Git tree ordering treats an exhausted file name as NUL and an exhausted directory name as
    // '/'. Exercise both prefix orientations through the public archive inspection boundary.
    let file_prefix_forward = zip_with_entries(&[
        ("foo", b"root file"),
        ("foo.bar/child.txt", b"nested file"),
    ]);
    let file_prefix_reverse = zip_with_entries(&[
        ("foo.bar/child.txt", b"nested file"),
        ("foo", b"root file"),
    ]);
    let tree_prefix_forward = zip_with_entries(&[
        ("foo/child.txt", b"nested file"),
        ("foo.bar", b"root file"),
    ]);
    let tree_prefix_reverse = zip_with_entries(&[
        ("foo.bar", b"root file"),
        ("foo/child.txt", b"nested file"),
    ]);

    let inspect = |fixture: &tempfile::TempDir| {
        inspect_zip_git_tree_with_mode(
            &fixture.path().join("fixture.zip"),
            None,
            ArchiveTreeRootMode::KeepTopLevel,
        )
        .unwrap()
    };

    let file_prefix_forward_report = inspect(&file_prefix_forward);
    let file_prefix_reverse_report = inspect(&file_prefix_reverse);
    assert_eq!(
        file_prefix_forward_report.git_tree_sha1,
        file_prefix_reverse_report.git_tree_sha1
    );
    assert_eq!(file_prefix_forward_report.file_count, 2);
    assert_eq!(file_prefix_forward_report.directory_count, 1);

    let tree_prefix_forward_report = inspect(&tree_prefix_forward);
    let tree_prefix_reverse_report = inspect(&tree_prefix_reverse);
    assert_eq!(
        tree_prefix_forward_report.git_tree_sha1,
        tree_prefix_reverse_report.git_tree_sha1
    );
    assert_eq!(tree_prefix_forward_report.file_count, 2);
    assert_eq!(tree_prefix_forward_report.directory_count, 1);
    assert_ne!(
        file_prefix_forward_report.git_tree_sha1,
        tree_prefix_forward_report.git_tree_sha1
    );
}

#[test]
fn comparison_fingerprint_binds_the_selected_root_mode() {
    let subset = zip_with_entries(&[("repo/a.txt", b"same bytes")]);
    let superset = zip_with_entries(&[("repo/a.txt", b"same bytes")]);
    let subset_path = subset.path().join("fixture.zip");
    let superset_path = superset.path().join("fixture.zip");

    let stripped = compare_zip_content_inclusion(
        &subset_path,
        &superset_path,
        ArchiveTreeRootMode::StripSharedRoot,
    )
    .unwrap();
    let kept = compare_zip_content_inclusion(
        &subset_path,
        &superset_path,
        ArchiveTreeRootMode::KeepTopLevel,
    )
    .unwrap();

    assert!(stripped.archives_identical);
    assert!(kept.archives_identical);
    assert_eq!(stripped.root_mode, "strip-shared-root");
    assert_eq!(kept.root_mode, "keep-top-level");
    assert_eq!(stripped.subset_root_prefix, "repo");
    assert_eq!(kept.subset_root_prefix, ".");
    assert_ne!(
        stripped.subset_manifest_sha256,
        kept.subset_manifest_sha256,
        "root-mode semantics must be bound into the manifest evidence"
    );
    assert_ne!(
        stripped.comparison_fingerprint_sha256,
        kept.comparison_fingerprint_sha256,
        "comparison evidence must not be replayable across root modes"
    );
}

#[test]
fn excessive_case_collision_groups_fail_closed_before_evidence_is_accepted() {
    // The report bounds collision evidence at 1,000 groups. Build 1,001 independent case-folded
    // collisions so the public inspector proves that oversized ambiguous evidence fails closed.
    let temp = tempfile::tempdir().unwrap();
    let file = File::create(temp.path().join("fixture.zip")).unwrap();
    let mut archive = zip::ZipWriter::new(file);
    for index in 0..=1_000 {
        for path in [
            format!("Case-{index:04}.txt"),
            format!("case-{index:04}.txt"),
        ] {
            archive
                .start_file(
                    path,
                    SimpleFileOptions::default().unix_permissions(0o100644),
                )
                .unwrap();
            archive.write_all(b"x").unwrap();
        }
    }
    archive.finish().unwrap();

    assert_eq!(
        inspect_zip_git_tree_with_mode(
            &temp.path().join("fixture.zip"),
            None,
            ArchiveTreeRootMode::KeepTopLevel,
        )
        .unwrap_err(),
        "archive-case-collision-groups-out-of-bounds"
    );
}
