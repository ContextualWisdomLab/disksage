#![cfg(unix)]

use disksage_lib::dev_artifacts::{find_artifacts, permanently_delete_artifacts};
use std::fs;
use std::os::unix::fs::PermissionsExt;

#[test]
fn permanent_artifact_cleanup_rejects_same_size_rewrite_during_active_use_probe() {
    let temp = tempfile::tempdir().expect("temporary development-artifact fixture");
    let root = temp.path().join("workspace");
    let project = root.join("app");
    let artifact = project.join("node_modules");
    fs::create_dir_all(&artifact).expect("create generated tree");
    fs::write(project.join("package.json"), b"{}").expect("write project marker");
    fs::write(artifact.join("original.bin"), b"original").expect("write original generated file");

    let candidates = find_artifacts(&root, 0, u64::MAX);
    assert_eq!(candidates.len(), 1, "fixture must produce one reviewed artifact");

    let fake_bin = temp.path().join("bin");
    fs::create_dir(&fake_bin).expect("create fake executable directory");
    let fake_lsof = fake_bin.join("lsof");
    fs::write(
        &fake_lsof,
        r#"#!/bin/sh
if [ ! -e "$DISKSAGE_TEST_MUTATION_MARKER" ]; then
  : > "$DISKSAGE_TEST_MUTATION_MARKER"
  printf 'changed!' > "$DISKSAGE_TEST_ARTIFACT/original.bin"
fi
exit 0
"#,
    )
    .expect("write fake lsof");
    fs::set_permissions(&fake_lsof, fs::Permissions::from_mode(0o755))
        .expect("make fake lsof executable");

    let old_path = std::env::var_os("PATH");
    let old_artifact = std::env::var_os("DISKSAGE_TEST_ARTIFACT");
    let old_marker = std::env::var_os("DISKSAGE_TEST_MUTATION_MARKER");
    let marker = temp.path().join("mutated.once");
    let joined_path = match old_path.as_ref() {
        Some(path) => format!("{}:{}", fake_bin.display(), path.to_string_lossy()),
        None => fake_bin.to_string_lossy().into_owned(),
    };
    std::env::set_var("PATH", joined_path);
    std::env::set_var("DISKSAGE_TEST_ARTIFACT", &artifact);
    std::env::set_var("DISKSAGE_TEST_MUTATION_MARKER", &marker);

    let journal = temp.path().join("journal.jsonl");
    let results = permanently_delete_artifacts(&candidates, &root, 0, &journal, 1);

    match old_path {
        Some(value) => std::env::set_var("PATH", value),
        None => std::env::remove_var("PATH"),
    }
    match old_artifact {
        Some(value) => std::env::set_var("DISKSAGE_TEST_ARTIFACT", value),
        None => std::env::remove_var("DISKSAGE_TEST_ARTIFACT"),
    }
    match old_marker {
        Some(value) => std::env::set_var("DISKSAGE_TEST_MUTATION_MARKER", value),
        None => std::env::remove_var("DISKSAGE_TEST_MUTATION_MARKER"),
    }

    assert_eq!(results.len(), 1);
    assert!(
        !results[0].ok,
        "an unreviewed same-size rewrite during active-use probing must invalidate irreversible authority"
    );
    assert!(artifact.exists(), "changed generated tree must remain in place");
    assert_eq!(
        fs::read(artifact.join("original.bin")).expect("late rewrite must survive rejected cleanup"),
        b"changed!"
    );
}
