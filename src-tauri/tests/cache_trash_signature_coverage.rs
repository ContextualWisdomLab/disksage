use disksage_lib::cache_cleanup::{
    proven_cache_trash_candidates, proven_cache_trash_snapshot, purge_proven_cache_trash,
};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const DELETE_UNAVAILABLE: &str = "cache-trash-identity-bound-permanent-delete-unavailable";

fn mkdir(path: impl AsRef<Path>) {
    fs::create_dir_all(path).unwrap();
}

fn write(path: impl AsRef<Path>) {
    fs::write(path, b"x").unwrap();
}

#[cfg(target_os = "macos")]
fn trash_root(home: &Path) -> PathBuf {
    home.join(".Trash")
}

#[cfg(all(unix, not(target_os = "macos")))]
fn trash_root(home: &Path) -> PathBuf {
    home.join(".local/share/Trash/files")
}

#[cfg(unix)]
#[test]
fn catalog_signatures_are_read_only_and_permanent_purge_fails_closed() {
    let temp = tempfile::tempdir().unwrap();
    let trash = trash_root(temp.path());
    mkdir(&trash);

    let npm = trash.join("_cacache");
    mkdir(npm.join("content-v2"));
    mkdir(npm.join("tmp"));
    write(npm.join("content-v2/object"));

    let pnpm = trash.join("v11");
    mkdir(pnpm.join("metadata"));
    mkdir(pnpm.join("metadata-full"));
    write(pnpm.join("metadata/index"));

    let simple = trash.join("simple-v21");
    mkdir(simple.join("pypi"));
    write(simple.join("pypi/index"));

    let typequest = trash.join("typequest");
    mkdir(typequest.join("common"));
    mkdir(typequest.join(".2"));
    write(typequest.join("common/index"));

    let wheels = trash.join("wheels-v6");
    mkdir(wheels.join("pypi"));
    write(wheels.join("pypi/wheel"));

    let sdists = trash.join("sdists-v9");
    mkdir(sdists.join("pypi"));
    mkdir(sdists.join("editable"));
    write(sdists.join("pypi/sdist"));

    let builds = trash.join("builds-v0");
    let build = builds.join(".tmp-native-build");
    mkdir(&build);
    write(build.join("pyvenv.cfg"));

    let trivy = trash.join("db");
    mkdir(&trivy);
    write(trivy.join("trivy.db"));
    write(trivy.join("metadata.json"));

    let candidates = proven_cache_trash_candidates(temp.path());
    let signatures = candidates
        .iter()
        .map(|candidate| candidate.signature.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        signatures,
        BTreeSet::from([
            "npm-cacache",
            "pnpm-store-v11",
            "trivy-database-cache",
            "uv-build-cache",
            "uv-sdist-cache",
            "uv-simple-index-cache",
            "uv-typequest-cache",
            "uv-wheel-cache",
        ])
    );
    assert_eq!(candidates.len(), 8);
    assert!(candidates.iter().all(|candidate| candidate.bytes > 0));

    let snapshot = proven_cache_trash_snapshot(temp.path());
    assert_eq!(snapshot.candidates, candidates);
    assert!(snapshot
        .approval_phrase
        .starts_with("DiskSage cache-trash purge approval "));

    let journal = temp.path().join("cache-trash-purge.jsonl");
    assert_eq!(
        purge_proven_cache_trash(temp.path(), &journal, 17, &snapshot).unwrap_err(),
        DELETE_UNAVAILABLE
    );
    assert!(!journal.exists());

    for path in [npm, pnpm, simple, typequest, wheels, sdists, builds, trivy] {
        assert!(
            path.exists(),
            "read-only proof must not permanently remove {}",
            path.display()
        );
    }
}

#[cfg(unix)]
#[test]
fn ambiguous_cache_lookalikes_and_symlinked_roots_are_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    assert!(proven_cache_trash_candidates(temp.path()).is_empty());

    let trash = trash_root(temp.path());
    mkdir(&trash);

    let symlink_target = temp.path().join("outside-valid-npm-cache");
    mkdir(symlink_target.join("content-v2"));
    mkdir(symlink_target.join("tmp"));
    write(symlink_target.join("content-v2/object"));
    let npm = trash.join("_cacache");
    std::os::unix::fs::symlink(&symlink_target, &npm).unwrap();

    let pnpm = trash.join("v11");
    mkdir(pnpm.join("metadata"));

    let simple = trash.join("simple-v21");
    mkdir(&simple);
    write(simple.join("pypi"));

    let typequest = trash.join("typequest");
    mkdir(typequest.join("common"));

    let wheels = trash.join("wheels-v6");
    mkdir(&wheels);

    let sdists = trash.join("sdists-v9");
    mkdir(sdists.join("pypi"));

    let builds = trash.join("builds-v0");
    let build = builds.join(".tmp-native-build");
    mkdir(&build);
    mkdir(build.join("pyvenv.cfg"));

    let trivy = trash.join("db");
    mkdir(&trivy);
    write(trivy.join("trivy.db"));

    let snapshot = proven_cache_trash_snapshot(temp.path());
    assert!(
        snapshot.candidates.is_empty(),
        "ambiguous structures and symlinked roots must never become deletion candidates"
    );

    let journal = temp.path().join("fail-closed-purge.jsonl");
    assert_eq!(
        purge_proven_cache_trash(temp.path(), &journal, 23, &snapshot).unwrap_err(),
        DELETE_UNAVAILABLE
    );
    assert!(!journal.exists());

    assert!(npm.symlink_metadata().unwrap().file_type().is_symlink());
    assert!(symlink_target.join("content-v2").is_dir());
    assert!(symlink_target.join("tmp").is_dir());
    for path in [pnpm, simple, typequest, wheels, sdists, builds, trivy] {
        assert!(path.exists(), "unproven lookalike must survive: {}", path.display());
    }
}

#[test]
fn cache_trash_snapshot_rejects_tampered_approval_before_any_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let mut snapshot = proven_cache_trash_snapshot(temp.path());
    snapshot.approval_phrase.push('0');

    let journal = temp.path().join("tampered-cache-trash-purge.jsonl");
    assert_eq!(
        purge_proven_cache_trash(temp.path(), &journal, 27, &snapshot).unwrap_err(),
        "cache-trash-confirmation-mismatch"
    );
    assert!(!journal.exists());
}

#[cfg(windows)]
#[test]
fn windows_cache_trash_purge_stays_unavailable_without_handle_bound_authority() {
    let temp = tempfile::tempdir().unwrap();
    let snapshot = proven_cache_trash_snapshot(temp.path());
    assert!(snapshot.candidates.is_empty());

    let journal = temp.path().join("cache-trash-purge.jsonl");
    assert_eq!(
        purge_proven_cache_trash(temp.path(), &journal, 29, &snapshot).unwrap_err(),
        DELETE_UNAVAILABLE
    );
    assert!(!journal.exists());
}
