//! Runtime coverage for the filesystem-native local volume evidence contract.
//!
//! These tests exercise the real `statvfs` adapter against an isolated temporary directory while
//! keeping assertions independent of host capacity values. They verify fail-closed input handling,
//! path redaction, integrity validation, and the explicit refusal to attribute physical reclaim.

use disksage_lib::volume_pressure::{
    compare_snapshots, snapshot_volume, validate_comparison, validate_snapshot,
    PhysicalReclaimAttribution,
};

#[test]
fn native_snapshot_is_valid_and_path_redacted() {
    let temp = tempfile::tempdir().expect("isolated filesystem fixture must be created");
    let snapshot = snapshot_volume(temp.path(), 1)
        .expect("an existing temporary directory must produce native volume evidence");

    validate_snapshot(&snapshot).expect("fresh native evidence must validate");
    assert!(snapshot.total_bytes > 0);
    assert!(snapshot.free_bytes <= snapshot.total_bytes);
    assert!(snapshot.available_bytes <= snapshot.free_bytes);
    assert_eq!(snapshot.evidence_fingerprint.len(), 64);

    let encoded = serde_json::to_string(&snapshot).expect("volume evidence must serialize");
    assert!(
        !encoded.contains(temp.path().to_string_lossy().as_ref()),
        "public volume evidence must not disclose the inspected local path"
    );
}

#[test]
fn native_snapshot_rejects_invalid_time_and_missing_path() {
    let temp = tempfile::tempdir().expect("isolated filesystem fixture must be created");
    assert_eq!(
        snapshot_volume(temp.path(), 0).expect_err("zero observation time must fail closed"),
        "local-volume-observed-time-invalid"
    );

    let missing = temp.path().join("definitely-missing-volume-probe");
    assert_eq!(
        snapshot_volume(&missing, 2).expect_err("missing path must fail closed"),
        "local-volume-path-not-found"
    );
}

#[test]
fn native_snapshots_compare_without_physical_reclaim_claim() {
    let temp = tempfile::tempdir().expect("isolated filesystem fixture must be created");
    let before = snapshot_volume(temp.path(), 10)
        .expect("first native volume observation must succeed");
    let after = snapshot_volume(temp.path(), 20)
        .expect("second native volume observation must succeed");

    let comparison = compare_snapshots(&before, &after, Some(0))
        .expect("two valid monotonic observations must compare");
    validate_comparison(&comparison).expect("comparison evidence must validate");
    assert_eq!(comparison.observed_elapsed_ms, 10);
    assert_eq!(comparison.physical_reclaim_bytes, None);
    assert_eq!(
        comparison.physical_reclaim_attribution,
        PhysicalReclaimAttribution::Unproven
    );
    assert_eq!(comparison.evidence_fingerprint.len(), 64);
}
