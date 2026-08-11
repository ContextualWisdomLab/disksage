//! Public-contract coverage for Maven-cache deletion hold boundaries.
//!
//! Every fixture is synthetic and local. These regressions prove that ambiguous or locally
//! enriched Maven repository state remains held instead of becoming deletion authority.

use disksage_lib::maven_cache::{
    audit_maven_repository, MavenCacheAuditOptions, MavenCacheAuditReport,
};
use std::path::Path;

fn version_dir(root: &Path, relative: &str) -> std::path::PathBuf {
    let path = root.join(relative);
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn write_remote_pair(path: &Path, stem: &str) {
    std::fs::write(path.join(format!("{stem}.jar")), b"jar").unwrap();
    std::fs::write(path.join(format!("{stem}.pom")), b"<project/>").unwrap();
    std::fs::write(
        path.join("_remote.repositories"),
        format!("{stem}.jar>central=\n{stem}.pom>central=\n"),
    )
    .unwrap();
}

fn audit(root: &Path) -> MavenCacheAuditReport {
    audit_maven_repository(root, MavenCacheAuditOptions::default(), 1).unwrap()
}

#[test]
fn audit_rejects_missing_and_regular_file_roots() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing-repository");
    assert_eq!(
        audit_maven_repository(&missing, MavenCacheAuditOptions::default(), 1).unwrap_err(),
        "maven-cache-root-unavailable"
    );

    let regular_file = temp.path().join("repository.txt");
    std::fs::write(&regular_file, b"not a repository").unwrap();
    assert_eq!(
        audit_maven_repository(&regular_file, MavenCacheAuditOptions::default(), 1).unwrap_err(),
        "maven-cache-root-not-real-directory"
    );
}

#[test]
fn zero_entry_budget_fails_closed_as_a_truncated_scan() {
    let temp = tempfile::tempdir().unwrap();
    let version = version_dir(temp.path(), "org/example/bounded/1.0.0");
    write_remote_pair(&version, "bounded-1.0.0");

    let report = audit_maven_repository(
        temp.path(),
        MavenCacheAuditOptions {
            max_entries: 0,
            max_candidates: 500,
            max_issues: 200,
        },
        1,
    )
    .unwrap();

    assert!(report.scan_truncated);
    assert!(report.truncated);
    assert_eq!(report.scanned_entries, 0);
    assert_eq!(report.marker_directories, 0);
    assert_eq!(report.remote_recoverable_directories, 0);
    assert!(report.candidates.is_empty());
    assert!(!report.provider_write_executed);
}

#[cfg(unix)]
#[test]
fn audit_rejects_symlink_and_non_utf8_repository_roots() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let real = temp.path().join("real-repository");
    std::fs::create_dir(&real).unwrap();
    let link = temp.path().join("repository-link");
    symlink(&real, &link).unwrap();
    assert_eq!(
        audit_maven_repository(&link, MavenCacheAuditOptions::default(), 1).unwrap_err(),
        "maven-cache-root-not-real-directory"
    );

    let invalid_name = OsString::from_vec(vec![b'r', b'e', b'p', b'o', b'-', 0xff]);
    let invalid_utf8_root = temp.path().join(invalid_name);
    std::fs::create_dir(&invalid_utf8_root).unwrap();
    assert_eq!(
        audit_maven_repository(&invalid_utf8_root, MavenCacheAuditOptions::default(), 1)
            .unwrap_err(),
        "maven-cache-root-not-utf8"
    );
}

#[test]
fn local_metadata_nested_directories_and_missing_payloads_are_held() {
    let temp = tempfile::tempdir().unwrap();

    let local_metadata = version_dir(temp.path(), "org/example/local-meta/1.0.0");
    write_remote_pair(&local_metadata, "local-meta-1.0.0");
    std::fs::write(
        local_metadata.join("maven-metadata-local.xml"),
        b"<metadata/>",
    )
    .unwrap();

    let nested = version_dir(temp.path(), "org/example/nested/1.0.0");
    write_remote_pair(&nested, "nested-1.0.0");
    std::fs::create_dir(nested.join("expanded")).unwrap();

    let no_payload = version_dir(temp.path(), "org/example/no-payload/1.0.0");
    std::fs::write(
        no_payload.join("_remote.repositories"),
        "missing.jar>central=\n",
    )
    .unwrap();

    let report = audit(temp.path());
    assert_eq!(report.remote_recoverable_directories, 0);
    assert_eq!(report.held_directories, 3);
    assert_eq!(report.held_reason_counts.get("local-metadata"), Some(&1));
    assert_eq!(report.held_reason_counts.get("nested-directory"), Some(&1));
    assert_eq!(report.held_reason_counts.get("no-artifact-payload"), Some(&1));
    assert!(report.candidates.is_empty());
    assert!(!report.provider_write_executed);
}

#[test]
fn marker_references_to_absent_files_are_not_treated_as_remote_recoverable() {
    let temp = tempfile::tempdir().unwrap();
    let version = version_dir(temp.path(), "org/example/missing-ref/1.0.0");
    std::fs::write(version.join("present.jar"), b"jar").unwrap();
    std::fs::write(
        version.join("_remote.repositories"),
        "present.jar>central=\nmissing.pom>central=\n",
    )
    .unwrap();

    let report = audit(temp.path());
    assert_eq!(report.remote_recoverable_directories, 0);
    assert_eq!(report.held_reason_counts.get("marker-reference-missing"), Some(&1));
}

#[test]
fn malformed_remote_markers_surface_bounded_issue_codes() {
    let cases = [
        ("empty", "# comment only\n", "remote-marker-empty"),
        (
            "conflict",
            "artifact.jar>central=\nartifact.jar>other=\n",
            "remote-marker-attribution-conflict",
        ),
        (
            "unsafe-name",
            "../artifact.jar>central=\n",
            "remote-marker-filename-invalid",
        ),
        ("bad-line", "artifact.jar=central\n", "remote-marker-line-invalid"),
    ];

    for (name, marker, expected_issue) in cases {
        let temp = tempfile::tempdir().unwrap();
        let version = version_dir(temp.path(), &format!("org/example/{name}/1.0.0"));
        std::fs::write(version.join("artifact.jar"), b"jar").unwrap();
        std::fs::write(version.join("_remote.repositories"), marker).unwrap();

        let report = audit(temp.path());
        assert_eq!(report.remote_recoverable_directories, 0, "{name}");
        assert_eq!(report.held_reason_counts.get("invalid-remote-marker"), Some(&1));
        assert_eq!(report.issues.len(), 1, "{name}");
        assert_eq!(report.issues[0].reason, expected_issue, "{name}");
    }
}

#[test]
fn oversized_remote_marker_is_rejected_before_parsing() {
    let temp = tempfile::tempdir().unwrap();
    let version = version_dir(temp.path(), "org/example/oversized/1.0.0");
    std::fs::write(version.join("artifact.jar"), b"jar").unwrap();
    std::fs::write(
        version.join("_remote.repositories"),
        vec![b'x'; 1024 * 1024 + 1],
    )
    .unwrap();

    let report = audit(temp.path());
    assert_eq!(report.remote_recoverable_directories, 0);
    assert_eq!(report.held_reason_counts.get("invalid-remote-marker"), Some(&1));
    assert_eq!(report.issues[0].reason, "remote-marker-too-large");
}

#[cfg(unix)]
#[test]
fn symlink_entries_hold_an_otherwise_remote_version_directory() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let version = version_dir(temp.path(), "org/example/symlinked/1.0.0");
    write_remote_pair(&version, "symlinked-1.0.0");
    let external = temp.path().join("external.bin");
    std::fs::write(&external, b"external").unwrap();
    symlink(&external, version.join("linked.bin")).unwrap();

    let report = audit(temp.path());
    assert_eq!(report.remote_recoverable_directories, 0);
    assert_eq!(report.held_reason_counts.get("symlink-entry"), Some(&1));
}

#[test]
fn remote_support_sidecars_do_not_become_untracked_payloads() {
    let temp = tempfile::tempdir().unwrap();
    let version = version_dir(temp.path(), "org/example/sidecars/1.0.0");
    write_remote_pair(&version, "sidecars-1.0.0");

    for name in [
        "resolver-status.properties",
        "sidecars-1.0.0.jar.lastUpdated",
        "sidecars-1.0.0.jar.sha1",
        "sidecars-1.0.0.jar.md5",
        "sidecars-1.0.0.jar.sha256",
        "sidecars-1.0.0.jar.sha512",
        "maven-metadata-central.xml",
    ] {
        std::fs::write(version.join(name), b"metadata").unwrap();
    }

    let report = audit(temp.path());
    assert_eq!(report.remote_recoverable_directories, 1);
    assert_eq!(report.held_directories, 0);
    assert_eq!(report.candidates.len(), 1);
    assert_eq!(report.candidates[0].artifact_files, 2);
}
