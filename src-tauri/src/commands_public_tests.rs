//! Deterministic coverage for command-layer pure cores without exposing them as public crate APIs.

use crate::commands::{
    execute_moves_inner, list_roots, load_ontology_from, node_view, parse_move_entry,
    undo_last_moves_inner,
};
use crate::organize::MovePlan;
use crate::scanner::{ScanResult, ScanStats};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

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
        node_view(&result, &root.join("..")).unwrap_err(),
        "path outside scanned root"
    );
    assert_eq!(
        node_view(&result, &temp.path().join("outside")).unwrap_err(),
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
