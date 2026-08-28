use disksage_lib::dev_artifacts::find_artifacts;
use std::fs;

#[test]
fn bare_tox_section_does_not_authorize_setup_cfg_cache_discovery() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join("setup.cfg"), "[tox]\nlegacy = true\n").unwrap();
    let tox = tmp.path().join(".tox");
    fs::create_dir(&tox).unwrap();
    fs::write(tox.join("cache.bin"), b"cache").unwrap();

    let artifacts = find_artifacts(tmp.path(), 0, u64::MAX);

    assert!(
        artifacts.iter().all(|artifact| artifact.kind != ".tox"),
        "only the standard setup.cfg [tox:tox] section may authorize .tox discovery"
    );
}

#[test]
fn rejected_python_314_environment_is_not_descended_for_nested_cache_candidates() {
    let tmp = tempfile::tempdir().unwrap();
    fs::write(tmp.path().join(".git"), "gitdir: /private/fixture\n").unwrap();
    let environment = tmp.path().join(".venv314");
    let nested_cache = environment.join(".mypy_cache");
    fs::create_dir_all(&nested_cache).unwrap();
    fs::write(environment.join("pyvenv.cfg"), "version = 3.13.9\n").unwrap();
    fs::write(nested_cache.join("cache.bin"), b"cache").unwrap();

    let artifacts = find_artifacts(tmp.path(), 0, u64::MAX);

    assert!(
        artifacts.is_empty(),
        "a marker-qualified .venv314 that fails its Python 3.14 proof must be pruned from discovery"
    );
}

#[test]
fn markerless_python_314_environment_is_not_descended_for_nested_cache_candidates() {
    let tmp = tempfile::tempdir().unwrap();
    let environment = tmp.path().join(".venv314");
    let nested_cache = environment.join(".mypy_cache");
    fs::create_dir_all(&nested_cache).unwrap();
    fs::write(environment.join("pyvenv.cfg"), "version = 3.14.1\n").unwrap();
    fs::write(nested_cache.join("cache.bin"), b"cache").unwrap();

    let artifacts = find_artifacts(tmp.path(), 0, u64::MAX);

    assert!(
        artifacts.is_empty(),
        "an unowned .venv314 must be pruned instead of lending authority to marker-free nested caches"
    );
}
