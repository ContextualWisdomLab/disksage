use disksage_lib::maven_cache::{
    audit_maven_repository, prune_maven_repository, MavenCacheAuditOptions,
};
use std::fs;

#[test]
fn apply_fails_closed_without_identity_bound_reversible_recycle() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repository");
    let version = root.join("org/example/demo/1.0.0");
    fs::create_dir_all(&version).unwrap();
    let jar = version.join("demo-1.0.0.jar");
    let pom = version.join("demo-1.0.0.pom");
    let marker = version.join("_remote.repositories");
    fs::write(&jar, b"remote jar bytes").unwrap();
    fs::write(&pom, b"<project/>").unwrap();
    fs::write(
        &marker,
        "demo-1.0.0.jar>central=\ndemo-1.0.0.pom>central=\n",
    )
    .unwrap();

    let original_jar = fs::read(&jar).unwrap();
    let original_pom = fs::read(&pom).unwrap();
    let original_marker = fs::read(&marker).unwrap();
    let audit =
        audit_maven_repository(&root, MavenCacheAuditOptions::default(), 123).unwrap();
    assert_eq!(audit.remote_recoverable_directories, 1);

    let error = prune_maven_repository(
        &root,
        &audit.candidate_set_fingerprint,
        true,
        10_000,
        456,
    )
    .unwrap_err();

    assert_eq!(
        error,
        "maven-cache-prune-identity-bound-recycle-unavailable"
    );
    assert!(version.is_dir());
    assert_eq!(fs::read(&jar).unwrap(), original_jar);
    assert_eq!(fs::read(&pom).unwrap(), original_pom);
    assert_eq!(fs::read(&marker).unwrap(), original_marker);
}
