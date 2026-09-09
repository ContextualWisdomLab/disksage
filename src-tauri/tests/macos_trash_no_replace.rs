#![cfg(target_os = "macos")]

use disksage_lib::dev_artifacts::{clean_artifacts, find_artifacts};
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs as unix_fs;
use std::path::{Path, PathBuf};

struct HomeGuard {
    original: Option<OsString>,
}

impl HomeGuard {
    fn replace(home: &Path) -> Self {
        let original = std::env::var_os("HOME");
        std::env::set_var("HOME", home);
        Self { original }
    }
}

impl Drop for HomeGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
    }
}

fn create_node_project(root: &Path) -> PathBuf {
    let project = root.join("webapp");
    let artifact = project.join("node_modules");
    fs::create_dir_all(&artifact).unwrap();
    fs::write(project.join("package.json"), b"{}").unwrap();
    fs::write(artifact.join("payload.bin"), b"reviewed-artifact").unwrap();
    artifact
}

#[test]
fn macos_trash_move_does_not_replace_a_dangling_destination_entry() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    let trash = home.join(".Trash");
    let work = tmp.path().join("work");
    fs::create_dir_all(&trash).unwrap();
    fs::create_dir_all(&work).unwrap();
    let _home_guard = HomeGuard::replace(&home);

    let artifact = create_node_project(&work);
    let now_ms = 1_777_777_777_u64;
    let planned = find_artifacts(&work, 0, now_ms);
    assert_eq!(planned.len(), 1, "the node_modules fixture must be reviewable");
    assert_eq!(Path::new(&planned[0].path), artifact.as_path());

    let collision = trash.join(format!(
        "disksage-{now_ms}-{}-0-node_modules",
        std::process::id()
    ));
    let missing_target = trash.join("incumbent-target-that-does-not-exist");
    unix_fs::symlink(&missing_target, &collision).unwrap();
    assert!(
        !collision.exists(),
        "a dangling destination is intentionally invisible to Path::exists"
    );
    assert!(
        fs::symlink_metadata(&collision)
            .unwrap()
            .file_type()
            .is_symlink(),
        "the incumbent directory entry must exist before cleanup"
    );

    let journal = tmp.path().join("journal.jsonl");
    let results = clean_artifacts(&planned, &work, 0, &journal, now_ms);
    assert_eq!(results.len(), 1);
    assert!(results[0].ok, "cleanup should retry a colliding trash name");

    let collision_metadata = fs::symlink_metadata(&collision)
        .expect("the pre-existing trash entry must not be replaced");
    assert!(
        collision_metadata.file_type().is_symlink(),
        "exclusive trash placement must preserve the incumbent directory entry"
    );
    assert_eq!(
        fs::read_link(&collision).unwrap(),
        missing_target,
        "the incumbent symlink target must remain unchanged"
    );

    let retried_destination = trash.join(format!(
        "disksage-{now_ms}-{}-1-node_modules",
        std::process::id()
    ));
    assert!(
        retried_destination.is_dir(),
        "EEXIST must advance to a fresh trash destination rather than overwrite"
    );
    assert!(
        !artifact.exists(),
        "the reviewed artifact should still complete its safe trash move"
    );
}
