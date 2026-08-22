//! Deterministic public-contract coverage for Maven-cache fail-closed boundary branches.
//!
//! These fixtures are synthetic and local. They exercise boundary states that are difficult to
//! reach through the ordinary happy-path audit while preserving the production contract: an
//! ambiguous repository is held, never converted into deletion authority.

use disksage_lib::maven_cache::{
    audit_maven_repository, prune_maven_repository, MavenCacheAuditOptions,
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

#[test]
fn root_level_remote_marker_is_held_as_an_unsafe_relative_path() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("artifact.jar"), b"jar").unwrap();
    std::fs::write(
        temp.path().join("_remote.repositories"),
        "artifact.jar>central=\n",
    )
    .unwrap();

    let report =
        audit_maven_repository(temp.path(), MavenCacheAuditOptions::default(), 11).unwrap();

    assert_eq!(report.marker_directories, 1);
    assert_eq!(report.remote_recoverable_directories, 0);
    assert_eq!(report.held_directories, 1);
    assert_eq!(report.held_bytes, 0);
    assert_eq!(report.held_reason_counts.get("unsafe-relative-path"), Some(&1));
    assert_eq!(report.issues.len(), 1);
    assert_eq!(report.issues[0].relative_path, "<unavailable>");
    assert_eq!(report.issues[0].reason, "maven-cache-relative-path-invalid");
    assert!(report.candidates.is_empty());
    assert!(!report.provider_write_executed);
}

#[cfg(unix)]
#[test]
fn non_utf8_version_entry_is_held_before_remote_attribution_can_authorize_it() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let temp = tempfile::tempdir().unwrap();
    let version = version_dir(temp.path(), "org/example/non-utf8-entry/1.0.0");
    write_remote_pair(&version, "non-utf8-entry-1.0.0");
    let invalid_name = OsString::from_vec(vec![b'l', b'o', b'c', b'a', b'l', b'-', 0xff]);
    std::fs::write(version.join(invalid_name), b"local-only").unwrap();

    let report =
        audit_maven_repository(temp.path(), MavenCacheAuditOptions::default(), 12).unwrap();

    assert_eq!(report.marker_directories, 1);
    assert_eq!(report.remote_recoverable_directories, 0);
    assert_eq!(report.held_directories, 1);
    assert_eq!(report.held_reason_counts.get("non-utf8-entry"), Some(&1));
    assert_eq!(report.issues.len(), 1);
    assert_eq!(report.issues[0].reason, "maven-version-entry-not-utf8");
    assert!(report.candidates.is_empty());
}

#[cfg(unix)]
#[test]
fn symlinked_subtree_is_not_followed_during_marker_discovery() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let external_version = version_dir(external.path(), "org/example/external/1.0.0");
    write_remote_pair(&external_version, "external-1.0.0");
    symlink(external.path(), temp.path().join("linked-repository")).unwrap();

    let report =
        audit_maven_repository(temp.path(), MavenCacheAuditOptions::default(), 13).unwrap();

    assert_eq!(report.marker_directories, 0);
    assert_eq!(report.remote_recoverable_directories, 0);
    assert_eq!(report.held_directories, 0);
    assert!(report.candidates.is_empty());
    assert!(report.issues.is_empty());
    assert!(!report.truncated);
}

#[test]
fn prune_rejects_a_valid_but_stale_candidate_set_fingerprint() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repository");
    let version = version_dir(&root, "org/example/stale/1.0.0");
    write_remote_pair(&version, "stale-1.0.0");

    let audit = audit_maven_repository(&root, MavenCacheAuditOptions::default(), 14).unwrap();
    let stale = "0".repeat(64);
    assert_ne!(stale, audit.candidate_set_fingerprint);

    assert_eq!(
        prune_maven_repository(&root, &stale, false, 10_000, 15).unwrap_err(),
        "maven-cache-prune-candidate-set-mismatch"
    );
    assert!(version.exists());
}

#[test]
fn prune_rejects_truncated_revalidation_even_with_the_previous_exact_fingerprint() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repository");
    let version = version_dir(&root, "org/example/bounded/1.0.0");
    write_remote_pair(&version, "bounded-1.0.0");

    let audit = audit_maven_repository(&root, MavenCacheAuditOptions::default(), 16).unwrap();

    assert_eq!(
        prune_maven_repository(
            &root,
            &audit.candidate_set_fingerprint,
            false,
            1,
            17,
        )
        .unwrap_err(),
        "maven-cache-prune-audit-truncated"
    );
    assert!(version.exists());
}

#[test]
fn prune_dry_run_preserves_candidates_and_reports_no_filesystem_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repository");
    let version = version_dir(&root, "org/example/dry-run/1.0.0");
    write_remote_pair(&version, "dry-run-1.0.0");

    let audit = audit_maven_repository(&root, MavenCacheAuditOptions::default(), 18).unwrap();
    let report = prune_maven_repository(
        &root,
        &audit.candidate_set_fingerprint,
        false,
        10_000,
        19,
    )
    .unwrap();

    assert_eq!(report.candidate_directories, 1);
    assert!(report.candidate_bytes > 0);
    assert_eq!(report.removed_directories, 0);
    assert_eq!(report.removed_bytes, 0);
    assert_eq!(report.skipped_directories, 0);
    assert!(report.skip_reason_counts.is_empty());
    assert!(!report.apply_requested);
    assert!(!report.filesystem_mutation_executed);
    assert!(report.complete);
    assert!(version.exists());
}
