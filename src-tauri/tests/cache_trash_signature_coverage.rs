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

    let npm = trash.join("_cacache 2");
    mkdir(npm.join("content-v2"));
    mkdir(npm.join("tmp"));

    let pnpm = trash.join("v11 01-02-03-004");
    mkdir(pnpm.join("metadata"));
    mkdir(pnpm.join("metadata-full"));

    let edge = trash.join("Default");
    mkdir(edge.join("Cache"));
    mkdir(edge.join("Code Cache"));

    let edge_code_sign = trash.join("code_sign_clone.A1b2C3 3");
    let edge_contents = edge_code_sign.join("Microsoft Edge.app.bundle/Contents");
    mkdir(edge_contents.join("MacOS"));
    mkdir(edge_contents.join("_CodeSignature"));
    write(edge_contents.join("Info.plist"));
    #[cfg(unix)]
    std::os::unix::fs::symlink("Info.plist", edge_contents.join("Info.link")).unwrap();

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

    let git = trash.join("git-v0");
    mkdir(git.join("locks"));
    mkdir(git.join("checkouts"));
    mkdir(git.join("db"));

    let archive = trash.join("archive-v0");
    mkdir(archive.join("A1b2C3d4_E5f6G7h"));

    let trivy = trash.join("db");
    mkdir(&trivy);
    write(trivy.join("trivy.db"));
    write(trivy.join("metadata.json"));

    let cloud_docs = trash.join("com.apple.CloudDocs.iCloudDriveFileProvider");
    let cloud_account = cloud_docs.join("0F876723-DC8F-4F53-9282-AE20BDB9034C");
    mkdir(&cloud_account);
    let cloud_database = "database-2026-09-10.db";
    write(cloud_account.join(cloud_database));
    write(cloud_account.join(format!("{cloud_database}-wal")));
    write(cloud_account.join(format!("{cloud_database}-shm")));

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
    let malformed_archive = trash.join("archive-v0 4");
    mkdir(malformed_archive.join("too-short"));
    let malformed_edge_code_sign = trash.join("code_sign_clone.BAD!23");
    mkdir(malformed_edge_code_sign.join("Microsoft Edge.app.bundle/Contents/MacOS"));

    let candidates = proven_cache_trash_candidates(temp.path());
    let signatures = candidates
        .iter()
        .map(|candidate| candidate.signature.as_str())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        signatures,
        BTreeSet::from([
            "edge-code-sign-clone",
            "edge-profile-cache",
            "fileprovider-cloud-docs-temporary-sqlite",
            "fileprovider-fpck-temporary-sqlite",
            "npm-cacache",
            "pnpm-store-v11",
            "trivy-database-cache",
            "uv-archive-cache",
            "uv-build-cache",
            "uv-git-cache",
            "uv-sdist-cache",
            "uv-simple-index-cache",
            "uv-typequest-cache",
            "uv-wheel-cache",
        ])
    );
    assert_eq!(candidates.len(), 14);

    let journal = temp.path().join("cache-trash-purge.jsonl");
    let results = purge_proven_cache_trash(temp.path(), &journal, 17).unwrap();
    assert_eq!(results.len(), 14);
    assert!(results.iter().all(|result| result.purged && result.error.is_empty()));

    for path in [
        npm,
        pnpm,
        edge,
        edge_code_sign,
        simple,
        typequest,
        wheels,
        sdists,
        builds,
        git,
        archive,
        trivy,
        cloud_docs,
        fpck,
    ] {
        assert!(!path.exists(), "proven fixture must be purged: {}", path.display());
    }
    assert!(malformed_simple.exists());
    assert!(malformed_build.exists());
    assert!(malformed_archive.exists());
    assert!(malformed_edge_code_sign.exists());

    let journal_text = fs::read_to_string(journal).unwrap();
    assert_eq!(journal_text.matches("\"outcome\":\"pending\"").count(), 14);
    assert_eq!(journal_text.matches("\"outcome\":\"ok\"").count(), 14);
}

#[test]
fn ambiguous_cache_lookalikes_and_symlinked_roots_are_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    assert!(proven_cache_trash_candidates(temp.path()).is_empty());

    let trash = temp.path().join(".Trash");
    mkdir(&trash);

    let npm = trash.join("_cacache invalid-collision");
    mkdir(npm.join("content-v2"));
    mkdir(npm.join("tmp"));

    let pnpm = trash.join("v11");
    mkdir(pnpm.join("metadata"));

    let edge = trash.join("Default 8");
    mkdir(edge.join("Cache"));

    let edge_code_sign = trash.join("code_sign_clone.A1b2C3");
    let edge_contents = edge_code_sign.join("Microsoft Edge.app.bundle/Contents");
    mkdir(edge_contents.join("MacOS"));
    mkdir(edge_contents.join("_CodeSignature"));

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
    mkdir(builds.join(".tmp-native-build"));

    let git = trash.join("git-v0");
    mkdir(git.join("locks"));
    mkdir(git.join("checkouts"));

    let archive = trash.join("archive-v0");
    mkdir(&archive);
    write(archive.join("A1b2C3d4_E5f6G7h"));

    let trivy = trash.join("db");
    mkdir(&trivy);
    write(trivy.join("trivy.db"));

    let cloud_docs = trash.join("com.apple.CloudDocs.iCloudDriveFileProvider");
    mkdir(cloud_docs.join("0F876723-DC8F-4F53-9282-AE20BDB9034C"));
    mkdir(cloud_docs.join("1F876723-DC8F-4F53-9282-AE20BDB9034C"));

    let fpck = trash.join("fileprovider-fpck");
    mkdir(fpck.join("not-a-uuid"));

    #[cfg(unix)]
    let symlink_target = {
        let target = temp.path().join("outside-valid-npm-cache");
        mkdir(target.join("content-v2"));
        mkdir(target.join("tmp"));
        std::os::unix::fs::symlink(&target, trash.join("_cacache 7")).unwrap();
        target
    };

    let candidates = proven_cache_trash_candidates(temp.path());
    assert!(candidates.is_empty(), "ambiguous structures must never become deletion candidates");

    let journal = temp.path().join("fail-closed-purge.jsonl");
    assert!(purge_proven_cache_trash(temp.path(), &journal, 23)
        .unwrap()
        .is_empty());
    assert!(!journal.exists());

    for path in [
        npm,
        pnpm,
        edge,
        edge_code_sign,
        simple,
        typequest,
        wheels,
        sdists,
        builds,
        git,
        archive,
        trivy,
        cloud_docs,
        fpck,
    ] {
        assert!(path.exists(), "unproven lookalike must survive: {}", path.display());
    }

    #[cfg(unix)]
    {
        assert!(trash.join("_cacache 7").symlink_metadata().unwrap().file_type().is_symlink());
        assert!(symlink_target.join("content-v2").is_dir());
        assert!(symlink_target.join("tmp").is_dir());
    }
}
