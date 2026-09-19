use disksage::dev_artifacts::find_artifacts;
use std::fs;

fn generated_root(project: &std::path::Path, name: &str) -> std::path::PathBuf {
    let root = project.join(name);
    fs::create_dir_all(&root).expect("create generated root");
    fs::write(root.join("payload.bin"), vec![0_u8; 4096]).expect("write generated payload");
    root
}

#[test]
fn cargo_target_requires_manifest_and_lockfile_rebuild_authority() {
    let temp = tempfile::tempdir().expect("create fixture root");
    let project = temp.path().join("cargo-app");
    fs::create_dir_all(&project).expect("create cargo project");
    fs::write(project.join("Cargo.toml"), b"[package]\nname='fixture'\nversion='0.1.0'\n")
        .expect("write Cargo manifest");
    let target = generated_root(&project, "target");

    let without_lock = find_artifacts(temp.path(), 0, u64::MAX);
    assert!(
        !without_lock.iter().any(|artifact| artifact.path == target.to_string_lossy()),
        "Cargo.toml alone is not rebuild authority for deleting target"
    );

    fs::write(project.join("Cargo.lock"), b"version = 4\n").expect("write Cargo lockfile");
    let with_lock = find_artifacts(temp.path(), 0, u64::MAX);
    assert!(
        with_lock.iter().any(|artifact| artifact.path == target.to_string_lossy()),
        "Cargo.toml + Cargo.lock must admit the generated target"
    );
}

#[test]
fn javascript_generated_roots_require_package_manifest_and_recognized_lockfile() {
    let temp = tempfile::tempdir().expect("create fixture root");
    let project = temp.path().join("web-app");
    fs::create_dir_all(&project).expect("create JavaScript project");
    fs::write(project.join("package.json"), br#"{"name":"fixture"}"#)
        .expect("write package manifest");
    let node_modules = generated_root(&project, "node_modules");
    let next = generated_root(&project, ".next");
    let electron = generated_root(&project, "dist-electron");

    let without_lock = find_artifacts(temp.path(), 0, u64::MAX);
    for path in [&node_modules, &next, &electron] {
        assert!(
            !without_lock.iter().any(|artifact| artifact.path == path.to_string_lossy()),
            "package.json alone is not rebuild authority for deleting {}",
            path.display()
        );
    }

    fs::write(project.join("pnpm-lock.yaml"), b"lockfileVersion: '9.0'\n")
        .expect("write recognized lockfile");
    let with_lock = find_artifacts(temp.path(), 0, u64::MAX);
    for path in [&node_modules, &next, &electron] {
        assert!(
            with_lock.iter().any(|artifact| artifact.path == path.to_string_lossy()),
            "package manifest + recognized lockfile must admit {}",
            path.display()
        );
    }
}

#[test]
fn native_cargo_cache_tag_remains_independent_of_project_lockfiles() {
    let temp = tempfile::tempdir().expect("create fixture root");
    let cache = temp.path().join("shared-target-cache");
    fs::create_dir_all(cache.join("debug")).expect("create cache layout");
    fs::write(
        cache.join("CACHEDIR.TAG"),
        "Signature: 8a477f597d28d172789f06886806bc55\n# This file is a cache directory tag created by cargo.\n",
    )
    .expect("write native cargo cache tag");
    fs::write(cache.join(".rustc_info.json"), b"{}").expect("write rustc cache metadata");
    fs::write(cache.join("debug/payload.bin"), vec![0_u8; 4096]).expect("write cache payload");

    let found = find_artifacts(temp.path(), 0, u64::MAX);
    assert!(
        found.iter().any(|artifact| artifact.kind == "cargo-target-cache" && artifact.path == cache.to_string_lossy()),
        "native Cargo cache authority must remain independently discoverable"
    );
}
