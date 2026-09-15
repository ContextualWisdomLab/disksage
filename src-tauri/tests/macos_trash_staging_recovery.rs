#![cfg(target_os = "macos")]

use disksage_lib::dev_artifacts::{clean_artifacts, find_artifacts};
use disksage_lib::filesystem_object_id;
use std::path::{Path, PathBuf};

struct TrashedFixtureGuard {
    expected_object_id: String,
    trash_dir: PathBuf,
    cleaned: bool,
}

impl TrashedFixtureGuard {
    fn new(expected_object_id: String, home: &Path) -> Self {
        Self {
            expected_object_id,
            trash_dir: home.join(".Trash"),
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

fn journal_outcomes(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .expect("read recovery journal")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .expect("parse recovery receipt")
                .get("outcome")
                .and_then(serde_json::Value::as_str)
                .expect("recovery receipt outcome")
                .to_owned()
        })
        .collect()
}

#[test]
fn successful_identity_bound_trash_leaves_no_private_staging_directory() {
    let home = PathBuf::from(
        std::env::var_os("HOME").expect("macOS test runner HOME must be available"),
    );
    let fixture = tempfile::tempdir_in(&home).expect("create filesystem fixture on macOS home volume");
    let project = fixture.path().join("project");
    let victim = project.join(".codegraph");
    std::fs::create_dir_all(&victim).expect("create reviewed regenerable directory");
    std::fs::write(victim.join("payload.bin"), b"reviewed-object")
        .expect("write reviewed payload");

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock after unix epoch")
        .as_millis() as u64;
    let candidates = find_artifacts(fixture.path(), 0, now_ms);
    let candidate = candidates
        .iter()
        .find(|candidate| candidate.path == victim.to_string_lossy())
        .expect("public inventory must discover the reviewed .codegraph candidate");
    assert!(candidate.scan_complete, "destructive acceptance requires a complete inventory");
    assert_eq!(candidate.skipped, 0, "destructive acceptance cannot skip inventory entries");
    let mut trash_guard = TrashedFixtureGuard::new(candidate.object_id.clone(), &home);
    let journal = fixture.path().join("journal.jsonl");

    let results = clean_artifacts(std::slice::from_ref(candidate), fixture.path(), 0, &journal, now_ms);
    assert_eq!(results.len(), 1, "one reviewed candidate must yield one cleanup result");
    assert!(
        results[0].ok,
        "public identity-bound macOS Trash cleanup failed: {}",
        results[0].error
    );

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
        std::fs::read_dir(&project)
            .expect("read reviewed candidate parent")
            .all(|entry| !entry
                .expect("read candidate-parent entry")
                .file_name()
                .to_string_lossy()
                .starts_with(".disksage-trash-")),
        "successful macOS Trash must not leave a DiskSage staging directory"
    );
    assert_eq!(
        journal_outcomes(&journal),
        ["pending".to_string(), "ok".to_string()],
        "the public cleanup boundary must durably record pending then terminal success"
    );

    assert_eq!(
        trash_guard
            .cleanup_exact_fixture()
            .expect("purge only the exact owned macOS Trash fixture"),
        1,
        "exactly one owned macOS Trash fixture must be cleaned"
    );
}
