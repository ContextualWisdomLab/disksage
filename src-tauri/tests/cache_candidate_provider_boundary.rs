use disksage_lib::rules::{cache_candidates, BaseDirs};

#[test]
fn cache_candidates_hide_managed_file_provider_roots() {
    let temporary = tempfile::tempdir().expect("temporary directory");
    let managed_temp = temporary
        .path()
        .join("File Provider Storage")
        .join("cache");
    std::fs::create_dir_all(&managed_temp).expect("managed provider cache fixture");
    std::fs::write(managed_temp.join("provider-state.bin"), b"provider-state")
        .expect("managed provider cache contents");

    let bases = BaseDirs {
        temp: managed_temp,
        local_data: temporary.path().join("local"),
        home: temporary.path().join("home"),
    };

    let candidates = cache_candidates(&bases);

    assert!(
        !candidates.iter().any(|candidate| candidate.id == "os-temp"),
        "managed File Provider storage must not be advertised as an actionable cache candidate"
    );
}

#[cfg(unix)]
#[test]
fn cache_candidates_hide_managed_file_provider_roots_reached_through_symlinked_ancestor() {
    use std::os::unix::fs::symlink;

    let temporary = tempfile::tempdir().expect("temporary directory");
    let managed_parent = temporary.path().join("File Provider Storage");
    let managed_temp = managed_parent.join("cache");
    std::fs::create_dir_all(&managed_temp).expect("managed provider cache fixture");
    std::fs::write(managed_temp.join("provider-state.bin"), b"provider-state")
        .expect("managed provider cache contents");

    let alias = temporary.path().join("cache-alias");
    symlink(&managed_parent, &alias).expect("provider ancestor symlink fixture");
    let aliased_temp = alias.join("cache");

    let bases = BaseDirs {
        temp: aliased_temp,
        local_data: temporary.path().join("local"),
        home: temporary.path().join("home"),
    };

    let candidates = cache_candidates(&bases);

    assert!(
        !candidates.iter().any(|candidate| candidate.id == "os-temp"),
        "a symlinked ancestor must not make managed File Provider storage actionable"
    );
}
