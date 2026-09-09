use std::fs;
use std::path::Path;

use crate::commands::{
    clean_dev_artifacts_inner, execute_moves_inner, list_cache_candidates_inner,
    list_dev_artifacts_inner, list_roots, load_ontology_from, parse_move_entry, plan_organize_inner,
    recent_operations_inner, undo_last_moves_inner,
};
use crate::dev_artifacts::{DevArtifact, DevArtifactCleanRequest};
use crate::organize::MovePlan;

fn detected(path: &Path, kind: &str, bytes: u64, mtime_ms: u64) -> DevArtifact {
    DevArtifact {
        path: path.to_string_lossy().into_owned(),
        kind: kind.into(),
        logical_bytes: bytes,
        allocated_bytes: bytes,
        age_days: 0,
        object_id: crate::filesystem_object_id(path).unwrap_or_default(),
        mtime_ms,
        active: false,
        symlink: false,
        mountpoint: false,
        stale: true,
        risk: "medium".into(),
        action: "trash".into(),
        reason: "coverage".into(),
    }
}

#[test]
fn cache_listing_wrapper_exercises_real_candidate_discovery() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let cache = home.join(".cache").join("example");
    fs::create_dir_all(&cache).unwrap();
    fs::write(cache.join("artifact.bin"), b"cache-bytes").unwrap();

    let candidates = list_cache_candidates_inner(&home).unwrap();
    assert!(candidates.iter().any(|candidate| candidate.path == cache));
}

#[test]
fn dev_artifact_listing_wrapper_preserves_real_detector_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let target = repo.join("target");
    fs::create_dir_all(&target).unwrap();
    fs::write(repo.join("Cargo.toml"), b"[package]\nname='demo'\nversion='0.1.0'\n").unwrap();
    fs::write(target.join("artifact.bin"), b"artifact").unwrap();

    let entries = list_dev_artifacts_inner(temp.path(), 0, u64::MAX);
    let candidate = entries
        .into_iter()
        .find(|entry| entry.path == target.to_string_lossy())
        .expect("target directory should be detected");
    assert_eq!(candidate.kind, "rust-target");
    assert!(!candidate.object_id.is_empty());
    assert_eq!(candidate.action, "trash");
}

#[test]
fn organize_planning_wrapper_delegates_to_real_ontology_move_planner() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let source = home.join("Downloads").join("picture.png");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, b"png").unwrap();

    let ontology = r#"
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix dm: <https://disksage.app/ontology#> .
dm:Image a owl:Class ; rdfs:label "이미지"@ko ; dm:targetFolder "~/Media/{class}" .
"#;
    let parsed = load_ontology_from(ontology).unwrap();
    let files = vec![crate::dupes::FileEntry {
        path: source.clone(),
        size: 3,
        mtime_ms: 0,
    }];

    let planned = plan_organize_inner(&files, &parsed, &home);
    assert_eq!(planned.len(), 1);
    assert_eq!(planned[0].src, source.to_string_lossy());
    assert!(planned[0].dst.ends_with("Media/Image/picture.png"));
}

#[test]
fn recent_operation_wrapper_reads_real_jsonl_and_bounds_results() {
    let temp = tempfile::tempdir().unwrap();
    let journal = temp.path().join("operations.jsonl");
    fs::write(
        &journal,
        concat!(
            "{\"timestamp_ms\":1,\"op\":\"move\",\"entries\":[\"one\"]}\n",
            "{\"timestamp_ms\":2,\"op\":\"move\",\"entries\":[\"two\"]}\n",
        ),
    )
    .unwrap();

    let entries = recent_operations_inner(&journal, 1).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].timestamp_ms, 2);
    assert_eq!(entries[0].entries, vec!["two"]);
    assert!(recent_operations_inner(&journal, 0).unwrap().is_empty());
}

#[test]
fn clean_dev_artifacts_requires_current_detector_evidence_before_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let target = repo.join("target");
    fs::create_dir_all(&target).unwrap();
    fs::write(repo.join("Cargo.toml"), b"[package]\nname='demo'\nversion='0.1.0'\n").unwrap();
    fs::write(target.join("artifact.bin"), b"stable").unwrap();

    let now_ms = 1_800_000_000_000;
    let current = list_dev_artifacts_inner(temp.path(), now_ms, u64::MAX)
        .into_iter()
        .find(|entry| entry.path == target.to_string_lossy())
        .unwrap();

    let request = DevArtifactCleanRequest::from(&current);
    let mut variants = Vec::new();

    let mut changed = request.clone();
    changed.logical_bytes = changed.logical_bytes.saturating_add(1);
    variants.push(changed);

    let mut changed = request.clone();
    changed.allocated_bytes = changed.allocated_bytes.saturating_add(1);
    variants.push(changed);

    let mut changed = request.clone();
    changed.mtime_ms = changed.mtime_ms.saturating_add(1);
    variants.push(changed);

    let mut changed = request.clone();
    changed.object_id.push_str("-replacement");
    variants.push(changed);

    let mut changed = request.clone();
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
        ..MovePlan::default()
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
        ..MovePlan::default()
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
