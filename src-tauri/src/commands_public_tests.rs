//! Deterministic coverage for command-layer pure cores without exposing them as public crate APIs.

use crate::commands::{
    clean_dev_artifacts_inner, clean_paths_inner, execute_moves_inner, list_roots,
    load_ontology_from, node_view, parse_move_entry, undo_last_moves_inner, AppState, CleanResult,
    EntryView, NodeView,
};
use crate::organize::MovePlan;
use crate::scanner::{ScanResult, ScanStats};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

fn result_for(root: &Path, dir_sizes: HashMap<PathBuf, u64>) -> ScanResult {
    ScanResult {
        root: root.to_path_buf(),
        dir_sizes,
        top_files: Vec::new(),
        stats: ScanStats::default(),
        cancelled: false,
    }
}

#[test]
fn node_view_rejects_parent_outside_and_missing_paths() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    fs::create_dir(&root).unwrap();
    let result = result_for(&root, HashMap::new());

    assert_eq!(
        node_view(&result, &root.join(".."))
            .err()
            .expect("parent traversal must fail"),
        "path outside scanned root"
    );
    assert_eq!(
        node_view(&result, &temp.path().join("outside"))
            .err()
            .expect("outside path must fail"),
        "path outside scanned root"
    );
    assert!(node_view(&result, &root.join("missing")).is_err());
}

#[test]
fn node_view_lists_files_and_directories_by_descending_size() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let directory = root.join("directory");
    let file = root.join("file.bin");
    fs::create_dir_all(&directory).unwrap();
    fs::write(&file, [1_u8, 2, 3, 4]).unwrap();

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&file, root.join("file-link")).unwrap();
    }

    let mut sizes = HashMap::new();
    sizes.insert(root.clone(), 10);
    sizes.insert(directory.clone(), 6);
    let view = node_view(&result_for(&root, sizes), &root).unwrap();

    assert_eq!(view.path, root.to_string_lossy());
    assert_eq!(view.size, 10);
    assert_eq!(view.entries.len(), 2);
    assert_eq!(view.entries[0].name, "directory");
    assert!(view.entries[0].is_dir);
    assert_eq!(view.entries[0].size, 6);
    assert_eq!(view.entries[1].name, "file.bin");
    assert!(!view.entries[1].is_dir);
    assert_eq!(view.entries[1].size, 4);
}

#[test]
fn node_view_defaults_unmeasured_directory_sizes_without_inventing_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("root");
    let unmeasured = root.join("unmeasured");
    fs::create_dir_all(&unmeasured).unwrap();

    let view = node_view(&result_for(&root, HashMap::new()), &root).unwrap();

    assert_eq!(view.size, 0);
    assert_eq!(view.entries.len(), 1);
    assert_eq!(view.entries[0].name, "unmeasured");
    assert!(view.entries[0].is_dir);
    assert_eq!(view.entries[0].size, 0);
}

#[test]
fn command_state_defaults_and_serializable_views_are_covered() {
    let state = AppState::default();
    assert!(state.result.lock().unwrap().is_none());
    assert!(!state.cancel.load(Ordering::SeqCst));
    assert!(!state.scanning.load(Ordering::SeqCst));
    assert!(state.cloud_review.lock().is_ok());

    let node = NodeView {
        path: "/tmp/example".into(),
        size: 7,
        entries: vec![EntryView {
            name: "file.bin".into(),
            path: "/tmp/example/file.bin".into(),
            size: 7,
            is_dir: false,
        }],
    };
    let node_json = serde_json::to_value(&node).unwrap();
    assert_eq!(node_json["path"], "/tmp/example");
    assert_eq!(node_json["size"], 7);
    assert_eq!(node_json["entries"][0]["name"], "file.bin");
    assert_eq!(node_json["entries"][0]["path"], "/tmp/example/file.bin");
    assert_eq!(node_json["entries"][0]["size"], 7);
    assert_eq!(node_json["entries"][0]["is_dir"], false);

    let clean = CleanResult {
        path: "/tmp/example/file.bin".into(),
        ok: false,
        error: "blocked".into(),
    };
    let clean_json = serde_json::to_value(&clean).unwrap();
    assert_eq!(clean_json["path"], "/tmp/example/file.bin");
    assert_eq!(clean_json["ok"], false);
    assert_eq!(clean_json["error"], "blocked");
}

#[test]
fn clean_paths_fail_closed_before_mutation_when_journaling_is_unavailable() {
    let temp = tempfile::tempdir().unwrap();
    let file = temp.path().join("file.bin");
    let directory = temp.path().join("directory");
    let nested = directory.join("nested.bin");
    let missing = temp.path().join("missing.bin");
    fs::write(&file, [1_u8, 2, 3, 4]).unwrap();
    fs::create_dir(&directory).unwrap();
    fs::write(&nested, [5_u8, 6, 7]).unwrap();

    // Passing an existing directory as the journal file makes OpenOptions fail before
    // trash::delete can run. This exercises regular-file, recursive-directory, and missing-file
    // accounting while proving the command core keeps every real target intact when its audit
    // journal cannot be written.
    let results = clean_paths_inner(
        &[file.clone(), directory.clone(), missing.clone()],
        temp.path(),
        99,
    );

    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|result| !result.ok));
    assert!(results.iter().all(|result| !result.error.is_empty()));
    assert_eq!(results[0].path, file.to_string_lossy());
    assert_eq!(results[1].path, directory.to_string_lossy());
    assert_eq!(results[2].path, missing.to_string_lossy());
    assert!(file.exists());
    assert!(directory.exists());
    assert!(nested.exists());
    assert!(!missing.exists());

    #[cfg(unix)]
    {
        // A filesystem root is rejected by the final safety guard without touching the journal.
        let protected = clean_paths_inner(&[PathBuf::from("/")], temp.path(), 100);
        assert_eq!(protected.len(), 1);
        assert!(!protected[0].ok);
        assert!(!protected[0].error.is_empty());
    }
}

#[test]
fn developer_artifact_cleanup_rejects_stale_manifest_and_preserves_current_object_on_journal_failure() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("app");
    let target = project.join("target");
    fs::create_dir_all(&target).unwrap();
    fs::write(project.join("Cargo.toml"), b"[package]\nname = \"coverage-fixture\"\n").unwrap();
    fs::write(target.join("artifact.bin"), b"preserve-me").unwrap();

    let mut requests = crate::dev_artifacts::find_artifacts(temp.path(), 0, u64::MAX);
    assert_eq!(requests.len(), 1);
    let current = requests.pop().unwrap();
    assert!(current.scan_complete);
    assert!(!current.object_id.is_empty());

    let mut stale = current.clone();
    stale.fingerprint.push('0');
    let stale_result = clean_dev_artifacts_inner(
        &[stale],
        temp.path(),
        0,
        &temp.path().join("missing-journal-parent").join("operations.jsonl"),
        u64::MAX,
    );
    assert_eq!(stale_result.len(), 1);
    assert!(!stale_result[0].ok);
    assert!(stale_result[0].error.contains("다시 스캔"));
    assert!(target.exists());

    // A current manifest reaches the identity-bound recycle authority. Pointing its audit journal
    // at a nonexistent parent makes journaling fail before the staged object can be renamed, so
    // the fixture proves the matching/error arm without relying on the host trash provider.
    let current_result = clean_dev_artifacts_inner(
        &[current],
        temp.path(),
        0,
        &temp.path().join("missing-journal-parent").join("operations.jsonl"),
        u64::MAX,
    );
    assert_eq!(current_result.len(), 1);
    assert!(!current_result[0].ok);
    assert!(!current_result[0].error.is_empty());
    assert!(target.exists());
    assert_eq!(fs::read(target.join("artifact.bin")).unwrap(), b"preserve-me");
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn developer_artifact_cleanup_recycles_current_identity_and_records_success() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("identity-success-project");
    let target = project.join("target");
    fs::create_dir_all(&target).unwrap();
    fs::write(
        project.join("Cargo.toml"),
        b"[package]\nname = \"identity-success-fixture\"\n",
    )
    .unwrap();
    fs::write(target.join("artifact.bin"), b"recycle-me").unwrap();

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let mut discovered = crate::dev_artifacts::find_artifacts(temp.path(), 0, now_ms);
    assert_eq!(discovered.len(), 1);
    let current = discovered.pop().unwrap();
    assert!(current.scan_complete);
    assert!(!current.object_id.is_empty());

    let journal = temp.path().join("operations.jsonl");
    let result = clean_dev_artifacts_inner(
        &[current],
        temp.path(),
        0,
        &journal,
        now_ms,
    );

    assert_eq!(result.len(), 1);
    assert!(result[0].ok, "{}", result[0].error);
    assert!(result[0].error.is_empty());
    assert!(!target.exists(), "reviewed artifact must leave its source path");

    let recent = crate::safety::journal_recent(&journal, 10);
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].outcome, "ok");
    assert_eq!(recent[1].outcome, "pending");

    // The identity-bound trash authority first moves the reviewed `target` into a private sibling
    // staging directory. Match both that original parent and the item name so this cleanup can
    // never purge unrelated user trash that happens to contain a directory named `target`.
    let items: Vec<_> = trash::os_limited::list()
        .unwrap()
        .into_iter()
        .filter(|item| item.name == std::ffi::OsStr::new("target"))
        .filter(|item| item.original_parent.starts_with(&project))
        .collect();
    assert_eq!(items.len(), 1, "exact staged fixture must be present in trash");
    trash::os_limited::purge_all(items).unwrap();
}

#[test]
fn developer_artifact_cleanup_rejects_each_mutable_request_identity_field() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("app");
    let target = project.join("target");
    fs::create_dir_all(&target).unwrap();
    fs::write(project.join("Cargo.toml"), b"[package]\nname = \"branch-coverage-fixture\"\n").unwrap();
    fs::write(target.join("artifact.bin"), b"stable").unwrap();

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let mut discovered = crate::dev_artifacts::find_artifacts(temp.path(), 0, now_ms);
    assert_eq!(discovered.len(), 1);
    let current = discovered.pop().unwrap();
    assert!(current.scan_complete);
    assert_eq!(current.skipped, 0);
    assert!(!current.object_id.is_empty());

    let mut variants = Vec::new();

    let mut changed = current.clone();
    changed.path.push_str("-replacement");
    variants.push(changed);

    let mut changed = current.clone();
    changed.kind.push_str("-replacement");
    variants.push(changed);

    let mut changed = current.clone();
    changed.project.push_str("-replacement");
    variants.push(changed);

    let mut changed = current.clone();
    changed.bytes = changed.bytes.saturating_add(1);
    variants.push(changed);

    let mut changed = current.clone();
    changed.files = changed.files.saturating_add(1);
    variants.push(changed);

    let mut changed = current.clone();
    changed.skipped = 1;
    variants.push(changed);

    let mut changed = current.clone();
    changed.scan_complete = false;
    variants.push(changed);

    let mut changed = current.clone();
    changed.fingerprint.push('0');
    variants.push(changed);

    let mut changed = current.clone();
    changed.object_id.clear();
    variants.push(changed);

    let mut changed = current.clone();
    changed.object_id.push_str("-replacement");
    variants.push(changed);

    let mut changed = current.clone();
    changed.age_days = changed.age_days.saturating_add(1);
    variants.push(changed);

    for request in variants {
        let result = clean_dev_artifacts_inner(
            &[request],
            temp.path(),
            0,
            &temp.path().join("unused-journal.jsonl"),
            now_ms,
        );
        assert_eq!(result.len(), 1);
        assert!(!result[0].ok);
        assert!(result[0].error.contains("다시 스캔"));
        assert!(target.exists());
        assert_eq!(fs::read(target.join("artifact.bin")).unwrap(), b"stable");
    }
}

#[test]
fn move_execution_journaling_and_undo_form_one_reversible_flow() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source.txt");
    let destination = temp.path().join("destination.txt");
    let journal = temp.path().join("operations.jsonl");
    fs::write(&source, b"reversible").unwrap();

    let plan = MovePlan {
        src: source.to_string_lossy().into_owned(),
        dst: destination.to_string_lossy().into_owned(),
        class_id: "test-class".into(),
    };
    let executed = execute_moves_inner(std::slice::from_ref(&plan), &journal, 100);
    assert_eq!(executed.len(), 1);
    assert!(executed[0].ok, "{}", executed[0].error);
    assert!(!source.exists());
    assert!(destination.exists());
    assert_eq!(
        parse_move_entry(&format!("{} -> {}", source.display(), destination.display())),
        Some((
            source.to_string_lossy().into_owned(),
            destination.to_string_lossy().into_owned()
        ))
    );
    assert_eq!(parse_move_entry("missing separator"), None);

    let undone = undo_last_moves_inner(1, &journal, 101);
    assert_eq!(undone.len(), 1);
    assert!(undone[0].ok, "{}", undone[0].error);
    assert!(source.exists());
    assert!(!destination.exists());
    assert!(undo_last_moves_inner(0, &journal, 102).is_empty());

    let missing = MovePlan {
        src: temp.path().join("missing.txt").to_string_lossy().into_owned(),
        dst: temp.path().join("never-created.txt").to_string_lossy().into_owned(),
        class_id: "test-class".into(),
    };
    let failed = execute_moves_inner(&[missing], &journal, 103);
    assert_eq!(failed.len(), 1);
    assert!(!failed[0].ok);
    assert!(!failed[0].error.is_empty());
}

#[test]
fn roots_and_ontology_wrappers_reach_their_real_pure_implementations() {
    let roots = list_roots();
    #[cfg(not(windows))]
    {
        assert_eq!(roots.first().map(String::as_str), Some("/"));
        if let Ok(home) = std::env::var("HOME") {
            assert!(roots.iter().any(|root| root == &home));
        }
    }
    #[cfg(windows)]
    {
        assert!(roots
            .iter()
            .all(|root| root.len() == 3 && root.ends_with(":\\")));
    }

    let ontology = load_ontology_from(include_str!("../resources/ontology/default.ttl")).unwrap();
    assert!(!ontology.classes.is_empty());
    assert!(load_ontology_from("this is not Turtle").is_err());
}
