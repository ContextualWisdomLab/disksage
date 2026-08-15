//! Public edge coverage for bounded local-cloud inventory without provider credentials.
//!
//! The fixtures use only temporary local filesystem objects. They exercise serialization,
//! root admission, checkpoint delivery, special-file handling, issue truncation, and bounded
//! candidate projection without opening file contents through DiskSage or authorizing eviction.

#![cfg(unix)]

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::cloud_local_inventory::{
    inventory_cloud_local_allocations, inventory_cloud_local_allocations_with_checkpoints,
    CloudLocalAllocationCandidate, CloudLocalAllocationInventory, CloudLocalInventoryIssue,
    CloudLocalInventoryOptions,
};
use std::fs::File;
use std::io::Write;
use std::os::unix::fs::symlink;
use std::os::unix::net::UnixListener;
use std::path::Path;

fn root(path: &Path) -> CloudRoot {
    CloudRoot {
        id: "icloud:public-edge-coverage".into(),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        label: "iCloud public edge coverage".into(),
        path: path.to_string_lossy().into_owned(),
        readable: true,
        access_issue: None,
    }
}

fn options() -> CloudLocalInventoryOptions {
    CloudLocalInventoryOptions {
        min_allocated_bytes: 1,
        max_entries: 100,
        max_results: 10,
        max_depth: 4,
        max_duration_ms: 10_000,
        max_issues: 10,
    }
}

fn write_allocated(path: &Path, size: usize) {
    let mut file = File::create(path).unwrap();
    file.write_all(&vec![0x5a; size]).unwrap();
    file.sync_all().unwrap();
}

fn add_unknown_field(value: &mut serde_json::Value) {
    value
        .as_object_mut()
        .expect("serialized public record must be a JSON object")
        .insert("unexpected".into(), serde_json::Value::Bool(true));
}

#[test]
fn public_inventory_records_round_trip_and_reject_unknown_fields() {
    let candidate = CloudLocalAllocationCandidate {
        path: "/Cloud/large.bin".into(),
        logical_bytes: 8_192,
        allocated_bytes: 12_288,
        filesystem_created_ms: Some(100),
        filesystem_modified_ms: Some(200),
        allocation_evidence: "filesystem:st-blocks-512".into(),
        content_opened: false,
        embedded_metadata_inspected: false,
        provider_sync_attested: false,
        eviction_blockers: vec![
            "provider-sync-unverified".into(),
            "human-eviction-approval-required".into(),
        ],
    };
    let issue = CloudLocalInventoryIssue {
        relative_scope: Some("socket".into()),
        kind: "unsupported-entry-type".into(),
        reason: "policy-not-file-or-directory".into(),
    };
    let inventory = CloudLocalAllocationInventory {
        version: 2,
        cloud_root_id: "icloud:public-edge-coverage".into(),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        cloud_root: "/Cloud".into(),
        observed_at_ms: 300,
        options: options(),
        visited_entries: 2,
        visited_files: 1,
        visited_directories: 0,
        skipped_entries: 1,
        issues: vec![issue.clone()],
        issues_truncated: false,
        allocated_candidate_bytes: candidate.allocated_bytes,
        candidates: vec![candidate.clone()],
        results_truncated: false,
        evidence_complete: false,
        stop_reasons: Vec::new(),
        notices: vec!["inventory-incomplete".into()],
    };

    let encoded = serde_json::to_value(&inventory).unwrap();
    let decoded: CloudLocalAllocationInventory = serde_json::from_value(encoded).unwrap();
    assert_eq!(decoded, inventory);

    let mut candidate_with_unknown = serde_json::to_value(&candidate).unwrap();
    add_unknown_field(&mut candidate_with_unknown);
    assert!(serde_json::from_value::<CloudLocalAllocationCandidate>(candidate_with_unknown).is_err());

    let mut issue_with_unknown = serde_json::to_value(&issue).unwrap();
    add_unknown_field(&mut issue_with_unknown);
    assert!(serde_json::from_value::<CloudLocalInventoryIssue>(issue_with_unknown).is_err());

    let mut inventory_with_unknown = serde_json::to_value(&inventory).unwrap();
    add_unknown_field(&mut inventory_with_unknown);
    assert!(serde_json::from_value::<CloudLocalAllocationInventory>(inventory_with_unknown).is_err());
}

#[test]
fn inventory_rejects_unreadable_missing_file_and_symlink_roots() {
    let temp = tempfile::tempdir().unwrap();

    let mut unreadable = root(temp.path());
    unreadable.readable = false;
    unreadable.access_issue = Some("permission-denied".into());
    assert!(inventory_cloud_local_allocations(&unreadable, options(), 1).is_err());

    let missing = temp.path().join("missing-root");
    assert_eq!(
        inventory_cloud_local_allocations(&root(&missing), options(), 2).unwrap_err(),
        "cloud-local-inventory-root-metadata-unavailable"
    );

    let regular_file = temp.path().join("regular-file-root");
    std::fs::write(&regular_file, b"not a directory").unwrap();
    assert_eq!(
        inventory_cloud_local_allocations(&root(&regular_file), options(), 3).unwrap_err(),
        "cloud-local-inventory-root-not-real-directory"
    );

    let symlink_root = temp.path().join("symlink-root");
    symlink(temp.path(), &symlink_root).unwrap();
    assert_eq!(
        inventory_cloud_local_allocations(&root(&symlink_root), options(), 4).unwrap_err(),
        "cloud-local-inventory-root-not-real-directory"
    );
}

#[test]
fn checkpoint_delivery_is_nonterminal_and_sink_failures_propagate() {
    let temp = tempfile::tempdir().unwrap();
    let root = root(temp.path());

    let error = inventory_cloud_local_allocations_with_checkpoints(
        &root,
        options(),
        10,
        |_| Err("checkpoint-sink-unavailable".into()),
    )
    .unwrap_err();
    assert_eq!(error, "checkpoint-sink-unavailable");

    let mut checkpoints = Vec::new();
    let report = inventory_cloud_local_allocations_with_checkpoints(
        &root,
        options(),
        11,
        |checkpoint| {
            checkpoints.push(checkpoint.clone());
            Ok(())
        },
    )
    .unwrap();

    assert!(report.evidence_complete);
    assert!(report.stop_reasons.is_empty());
    assert!(!checkpoints.is_empty());
    assert!(checkpoints.iter().all(|checkpoint| {
        !checkpoint.evidence_complete
            && checkpoint
                .notices
                .iter()
                .any(|notice| notice == "inventory-checkpoint-not-terminal")
    }));
}

#[test]
fn symlink_and_socket_issues_are_bounded_without_content_access() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target.bin");
    let link = temp.path().join("target-link");
    let socket_path = temp.path().join("inventory.sock");
    std::fs::write(&target, b"target").unwrap();
    symlink(&target, &link).unwrap();
    let _socket = UnixListener::bind(&socket_path).unwrap();

    let full = inventory_cloud_local_allocations(&root(temp.path()), options(), 20).unwrap();
    assert_eq!(full.skipped_entries, 2);
    assert!(!full.evidence_complete);
    assert!(full.issues.iter().any(|issue| {
        issue.relative_scope.as_deref() == Some("target-link")
            && issue.kind == "symlink-skipped"
            && issue.reason == "policy-not-followed"
    }));
    assert!(full.issues.iter().any(|issue| {
        issue.relative_scope.as_deref() == Some("inventory.sock")
            && issue.kind == "unsupported-entry-type"
            && issue.reason == "policy-not-file-or-directory"
    }));
    assert!(full.candidates.iter().all(|candidate| {
        !candidate.content_opened
            && !candidate.embedded_metadata_inspected
            && !candidate.provider_sync_attested
    }));

    let mut truncated_options = options();
    truncated_options.max_issues = 1;
    let truncated = inventory_cloud_local_allocations(
        &root(temp.path()),
        truncated_options,
        21,
    )
    .unwrap();
    assert_eq!(truncated.skipped_entries, 2);
    assert_eq!(truncated.issues.len(), 1);
    assert!(truncated.issues_truncated);
    assert!(!truncated.evidence_complete);
    assert!(truncated
        .notices
        .iter()
        .any(|notice| notice == "inventory-issues-truncated"));
    assert!(truncated
        .notices
        .iter()
        .any(|notice| notice == "inventory-incomplete"));
}

#[test]
fn candidate_projection_sorts_by_allocation_and_marks_result_truncation() {
    let temp = tempfile::tempdir().unwrap();
    write_allocated(&temp.path().join("small.bin"), 4_096);
    write_allocated(&temp.path().join("large.bin"), 16_384);

    let mut bounded = options();
    bounded.max_results = 1;
    let report = inventory_cloud_local_allocations(&root(temp.path()), bounded, 30).unwrap();

    assert_eq!(report.visited_files, 2);
    assert!(report.evidence_complete);
    assert!(report.allocated_candidate_bytes > 0);
    assert_eq!(report.candidates.len(), 1);
    assert!(report.results_truncated);
    assert!(report.candidates[0].path.ends_with("large.bin"));
    assert!(report
        .notices
        .iter()
        .any(|notice| notice == "candidate-output-truncated"));
}
