#![cfg(target_os = "macos")]

use disksage_lib::safety::{filesystem_object_id, journal_recent, trash_delete_if_identity};
use std::path::{Path, PathBuf};

struct TrashedFixtureGuard {
    expected_object_id: String,
    trash_dir: PathBuf,
    cleaned: bool,
}

impl TrashedFixtureGuard {
    fn new(expected_object_id: String) -> Self {
        let home = std::env::var_os("HOME").expect("macOS test runner HOME must be available");
        Self {
            expected_object_id,
            trash_dir: PathBuf::from(home).join(".Trash"),
            cleaned: false,
        }
    }

    fn matching_items(&self) -> Result<Vec<PathBuf>, String> {
        let entries = match std::fs::read_dir(&self.trash_dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(format!(
                    "read macOS Trash {}: {error}",
                    self.trash_dir.display()
                ))
            }
        };

        let mut matches = Vec::new();
        for entry in entries {
            let path = entry
                .map_err(|error| format!("read macOS Trash entry: {error}"))?
                .path();
            if filesystem_object_id(&path).ok().as_deref() == Some(self.expected_object_id.as_str()) {
                matches.push(path);
            }
        }
        Ok(matches)
    }

    fn cleanup_exact_fixture(&mut self) -> Result<usize, String> {
        let matches = self.matching_items()?;
        for path in &matches {
            remove_exact_fixture_directory(path)?;
        }
        self.cleaned = true;
        Ok(matches.len())
    }
}

impl Drop for TrashedFixtureGuard {
    fn drop(&mut self) {
        if !self.cleaned {
            let _ = self.cleanup_exact_fixture();
        }
    }
}

fn remove_exact_fixture_directory(path: &Path) -> Result<(), String> {
    let mut entries = std::fs::read_dir(path)
        .map_err(|error| format!("read owned Trash fixture {}: {error}", path.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("read owned Trash fixture entry: {error}"))?;

    if entries.len() != 1 || entries[0].file_name() != "payload.bin" {
        return Err(format!(
            "refuse to purge unexpected contents from owned Trash fixture {}",
            path.display()
        ));
    }

    let payload = entries.pop().expect("single fixture payload").path();
    let metadata = std::fs::symlink_metadata(&payload)
        .map_err(|error| format!("stat owned Trash fixture payload: {error}"))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(format!(
            "refuse to purge non-regular owned Trash fixture payload {}",
            payload.display()
        ));
    }

    std::fs::remove_file(&payload)
        .map_err(|error| format!("remove owned Trash fixture payload: {error}"))?;
    std::fs::remove_dir(path)
        .map_err(|error| format!("remove owned Trash fixture directory: {error}"))
}

#[test]
fn successful_identity_bound_trash_leaves_no_private_staging_directory() {
    let fixture = tempfile::tempdir().expect("create filesystem fixture");
    let victim_name = format!(
        "disksage-macos-staging-recovery-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock after unix epoch")
            .as_nanos()
    );
    let victim = fixture.path().join(&victim_name);
    std::fs::create_dir(&victim).expect("create reviewed directory");
    std::fs::write(victim.join("payload.bin"), b"reviewed-object")
        .expect("write reviewed payload");

    let expected_object_id = filesystem_object_id(&victim).expect("capture reviewed object identity");
    let mut trash_guard = TrashedFixtureGuard::new(expected_object_id.clone());
    let journal = fixture.path().join("journal.jsonl");

    trash_delete_if_identity(&victim, &expected_object_id, 15, &journal, 1)
        .expect("identity-bound macOS Trash mutation should succeed");

    assert!(!victim.exists(), "the reviewed object must have moved to macOS Trash");
    let trashed = trash_guard
        .matching_items()
        .expect("locate the exact reviewed filesystem object in macOS Trash");
    assert_eq!(
        trashed.len(),
        1,
        "the exact reviewed filesystem object must be present once in macOS Trash"
    );

    assert!(
        std::fs::read_dir(fixture.path())
            .expect("read fixture parent")
            .all(|entry| !entry
                .expect("read fixture entry")
                .file_name()
                .to_string_lossy()
                .starts_with(".disksage-trash-")),
        "successful macOS Trash must not leave a DiskSage staging directory"
    );

    let entries = journal_recent(&journal, 2);
    assert_eq!(entries.len(), 2, "pending and terminal receipts are required");
    assert_eq!(entries[0].outcome, "ok");
    assert_eq!(entries[1].outcome, "pending");

    assert_eq!(
        trash_guard
            .cleanup_exact_fixture()
            .expect("purge only the exact owned macOS Trash fixture"),
        1,
        "exactly one owned macOS Trash fixture must be cleaned"
    );
}
