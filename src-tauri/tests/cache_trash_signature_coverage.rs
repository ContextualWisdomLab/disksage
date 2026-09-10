use disksage_lib::cache_cleanup::{proven_cache_trash_candidates, purge_proven_cache_trash};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

fn mkdir(path: impl AsRef<Path>) {
    fs::create_dir_all(path).unwrap();
}

fn write(path: impl AsRef<Path>) {
    fs::write(path, b"x").unwrap();
}

#[test]
fn catalog_signatures_are_structurally_proven_before_fixture_trash_is_purged() {
    let temp = tempfile::tempdir().unwrap();
    let trash = temp.path().join(".Trash");
    mkdir(&trash);

    let edge = trash.join("Default");
    mkdir(edge.join("Cache"));
    mkdir(edge.join("Code Cache"));

    let simple = trash.join("simple-v24");
    mkdir(simple.join("pypi"));

    let typequest = trash.join("typequest");
    mkdir(typequest.join("common"));
    mkdir(typequest.join(".2"));

    let wheels = trash.join("wheels-v6");
    mkdir(wheels.join("pypi"));

    let sdists = trash.join("sdists-v9");
    mkdir(sdists.join("pypi"));
    mkdir(sdists.join("editable"));

    let builds = trash.join("builds-v0");
    let build = builds.join(".tmp-native-build");
    mkdir(&build);
    write(build.join("pyvenv.cfg"));

    let trivy = trash.join("db");
    mkdir(&trivy);
    write(trivy.join("trivy.db"));
    write(trivy.join("metadata.json"));

    let fpck = trash.join("fileprovider-fpck");
    let account = fpck.join("75876723-DC8F-4F53-9282-AE20BDB9034C");
    mkdir(&account);
    let database = "75876723-DC8F-4F53-9282-AE20BDB9034C-cache";
    write(account.join(database));
    write(account.join(format!("{database}-wal")));
    write(account.join(format!("{database}-shm")));

    // Exact catalog names are not enough. These malformed lookalikes must survive both planning
    // and the permanent-delete command because their on-disk structure does not prove a cache.
    let malformed_simple = trash.join("simple-v22");
    mkdir(&malformed_simple);
    write(malformed_simple.join("pypi"));
    let malformed_build = trash.join("builds-v0 2");
    mkdir(malformed_build.join(".tmp-native-build"));

    let candidates = proven_cache_trash_candidates(temp.path());
    let signatures = candidates
        .iter()
        .map(|candidate| candidate.signature.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        signatures,
        BTreeSet::from([
            "edge-profile-cache",
            "fileprovider-fpck-temporary-sqlite",
            "trivy-database-cache",
            "uv-build-cache",
            "uv-sdist-cache",
            "uv-simple-index-cache",
            "uv-typequest-cache",
            "uv-wheel-cache",
        ])
    );
    assert_eq!(candidates.len(), 8);

    let journal = temp.path().join("cache-trash-purge.jsonl");
    let results = purge_proven_cache_trash(temp.path(), &journal, 17).unwrap();
    assert_eq!(results.len(), 8);
    assert!(results.iter().all(|result| result.purged && result.error.is_empty()));

    for path in [edge, simple, typequest, wheels, sdists, builds, trivy, fpck] {
        assert!(!path.exists(), "proven fixture must be purged: {}", path.display());
    }
    assert!(malformed_simple.exists());
    assert!(malformed_build.exists());

    let journal_text = fs::read_to_string(journal).unwrap();
    assert_eq!(journal_text.matches("\"outcome\":\"pending\"").count(), 8);
    assert_eq!(journal_text.matches("\"outcome\":\"ok\"").count(), 8);
}