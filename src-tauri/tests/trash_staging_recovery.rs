#![cfg(target_os = "linux")]

use disksage_lib::dev_artifacts::{clean_artifacts, find_artifacts};
use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

struct LinuxTrashFixtureGuard {
    original_path: PathBuf,
    preexisting_ids: HashSet<OsString>,
    cleaned: bool,
}

impl LinuxTrashFixtureGuard {
    fn new(original_path: PathBuf) -> Self {
        let preexisting_ids = trash::os_limited::list()
            .expect("snapshot Linux Trash before destructive fixture")
            .into_iter()
            .map(|item| item.id)
            .collect();
        Self {
            original_path,
            preexisting_ids,
            cleaned: false,
        }
    }

    fn matching_items(&self) -> Result<Vec<trash::TrashItem>, trash::Error> {
        Ok(trash::os_limited::list()?
            .into_iter()
            .filter(|item| {
                item.original_path() == self.original_path
                    && !self.preexisting_ids.contains(&item.id)
            })
            .collect())
    }

    fn matching_count(&self) -> usize {
        self.matching_items().expect("list Linux Trash").len()
    }

    fn cleanup(&mut self) -> usize {
        let items = self.matching_items().expect("list Linux Trash");
        let count = items.len();
        if !items.is_empty() {
            trash::os_limited::purge_all(items).expect("purge owned Linux Trash fixture");
        }
        self.cleaned = true;
        count
    }
}

impl Drop for LinuxTrashFixtureGuard {
    fn drop(&mut self) {
        if !self.cleaned {
            if let Ok(items) = self.matching_items() {
                if !items.is_empty() {
                    let _ = trash::os_limited::purge_all(items);
                }
            }
        }
    }
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
    let fixture = tempfile::tempdir().expect("create filesystem fixture");
    let victim_name = format!(
        "disksage-staging-recovery-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock after unix epoch")
            .as_nanos()
    );
    let extensions = fixture.path().join(".vscode/extensions");
    let victim = extensions.join(&victim_name);
    std::fs::create_dir_all(&victim).expect("create reviewed extension directory");
    std::fs::write(victim.join("package.json"), b"{}")
        .expect("write reviewed extension manifest");
    std::fs::write(victim.join("payload.bin"), b"reviewed-object")
        .expect("write reviewed payload");
    std::fs::write(
        extensions.join(".obsolete"),
        format!("{{\"{victim_name}\":true}}"),
    )
    .expect("mark fixture as obsolete extension");

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock after unix epoch")
        .as_millis() as u64;
    let candidates = find_artifacts(fixture.path(), 0, now_ms);
    let candidate = candidates
        .iter()
        .find(|candidate| candidate.path == victim.to_string_lossy())
        .expect("public inventory must discover the reviewed obsolete extension");
    assert!(candidate.scan_complete, "destructive acceptance requires a complete inventory");
    assert_eq!(candidate.skipped, 0, "destructive acceptance cannot skip inventory entries");
    let mut trash_guard = LinuxTrashFixtureGuard::new(victim.clone());
    let journal = fixture.path().join("journal.jsonl");

    let results = clean_artifacts(std::slice::from_ref(candidate), fixture.path(), 0, &journal, now_ms);
    assert_eq!(results.len(), 1, "one reviewed candidate must yield one cleanup result");
    assert!(
        results[0].ok,
        "public identity-bound Linux Trash cleanup failed: {}",
        results[0].error
    );

    assert!(!victim.exists(), "the reviewed object must have moved to Linux Trash");
    assert_eq!(
        trash_guard.matching_count(),
        1,
        "the newly created Trash item for the reviewed original path must be unique"
    );
    assert!(
        std::fs::read_dir(&extensions)
            .expect("read reviewed candidate parent")
            .all(|entry| !entry
                .expect("read candidate-parent entry")
                .file_name()
                .to_string_lossy()
                .starts_with(".disksage-trash-")),
        "successful Linux Trash must not leave a DiskSage staging directory"
    );
    assert_eq!(
        journal_outcomes(&journal),
        ["pending".to_string(), "ok".to_string()],
        "the public cleanup boundary must durably record pending then terminal success"
    );
    assert_eq!(trash_guard.cleanup(), 1, "exactly one owned Linux Trash fixture must be cleaned");
}
