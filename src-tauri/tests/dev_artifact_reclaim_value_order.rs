#![cfg(unix)]

use disksage_lib::dev_artifacts::find_artifacts;
use std::fs;

fn cargo_target(root: &std::path::Path, name: &str) -> std::path::PathBuf {
    let project = root.join(name);
    fs::create_dir_all(project.join("target")).unwrap();
    fs::write(
        project.join("Cargo.toml"),
        format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n"),
    )
    .unwrap();
    fs::write(project.join("Cargo.lock"), b"version = 4\n").unwrap();
    project.join("target")
}

#[test]
fn development_roots_are_ranked_by_reclaimable_allocation_not_logical_size() {
    let temp = tempfile::tempdir().unwrap();

    let sparse = cargo_target(temp.path(), "sparse");
    let sparse_file = fs::File::create(sparse.join("huge-sparse.bin")).unwrap();
    sparse_file.set_len(128 * 1024 * 1024).unwrap();

    let dense = cargo_target(temp.path(), "dense");
    fs::write(dense.join("dense.bin"), vec![0x5a; 1024 * 1024]).unwrap();

    let found = find_artifacts(temp.path(), 0, u64::MAX);
    assert_eq!(found.len(), 2);

    let sparse = found.iter().find(|item| item.project == "sparse").unwrap();
    let dense = found.iter().find(|item| item.project == "dense").unwrap();
    assert!(sparse.bytes > dense.bytes, "fixture must be logically larger");
    assert!(
        sparse.allocated_bytes < dense.allocated_bytes,
        "fixture must consume fewer local blocks"
    );
    assert_eq!(
        found[0].project, "dense",
        "the buyer-visible reclaim list must rank the larger physical reclaim first"
    );
}
