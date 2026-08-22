use disksage_lib::volume_pressure::{
    compare_snapshots, snapshot_volume, validate_comparison, validate_snapshot, ByteChange,
    ByteChangeDirection, LocalVolumePressure, LocalVolumeSnapshot, PhysicalReclaimAttribution,
    LOCAL_VOLUME_COMPARISON_SCHEMA_VERSION, LOCAL_VOLUME_SNAPSHOT_SCHEMA_VERSION,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;

fn json_sha256<T: Serialize>(value: &T) -> String {
    let encoded = serde_json::to_vec(value).unwrap();
    let digest = Sha256::digest(encoded);
    let mut output = String::with_capacity(64);
    for byte in digest {
        write!(&mut output, "{byte:02x}").unwrap();
    }
    output
}

fn resign_snapshot(snapshot: &mut LocalVolumeSnapshot) {
    snapshot.evidence_fingerprint.clear();
    let fingerprint = json_sha256(snapshot);
    snapshot.evidence_fingerprint = fingerprint;
}

fn pressure_for(available_basis_points: u16) -> LocalVolumePressure {
    match available_basis_points {
        0..=500 => LocalVolumePressure::Critical,
        501..=1_000 => LocalVolumePressure::High,
        1_001..=2_000 => LocalVolumePressure::Elevated,
        _ => LocalVolumePressure::Normal,
    }
}

fn synthetic_snapshot(
    template: &LocalVolumeSnapshot,
    total_bytes: u64,
    free_bytes: u64,
    available_bytes: u64,
    observed_at_ms: u64,
) -> LocalVolumeSnapshot {
    assert!(total_bytes > 0);
    assert!(free_bytes <= total_bytes);
    assert!(available_bytes <= free_bytes);
    assert!(observed_at_ms > 0);

    let available_basis_points =
        ((u128::from(available_bytes) * 10_000) / u128::from(total_bytes)) as u16;
    let mut snapshot = template.clone();
    snapshot.schema_version = LOCAL_VOLUME_SNAPSHOT_SCHEMA_VERSION;
    snapshot.observed_at_ms = observed_at_ms;
    snapshot.total_bytes = total_bytes;
    snapshot.free_bytes = free_bytes;
    snapshot.available_bytes = available_bytes;
    snapshot.used_bytes = total_bytes - free_bytes;
    snapshot.available_basis_points = available_basis_points;
    snapshot.pressure = pressure_for(available_basis_points);
    resign_snapshot(&mut snapshot);
    validate_snapshot(&snapshot).unwrap();
    snapshot
}

fn tamper_lower_hex(value: &str) -> String {
    let mut bytes = value.as_bytes().to_vec();
    bytes[0] = if bytes[0] == b'0' { b'1' } else { b'0' };
    String::from_utf8(bytes).unwrap()
}

#[test]
fn snapshot_admission_and_integrity_validation_fail_closed() {
    let temp = tempfile::tempdir().unwrap();

    assert_eq!(
        snapshot_volume(temp.path(), 0).unwrap_err(),
        "local-volume-observed-time-invalid"
    );

    #[cfg(unix)]
    assert_eq!(
        snapshot_volume(&temp.path().join("missing-volume-path"), 1).unwrap_err(),
        "local-volume-path-not-found"
    );

    let live = snapshot_volume(temp.path(), 1).unwrap();
    assert!(validate_snapshot(&live).is_ok());

    let mut changed = live.clone();
    changed.schema_version = LOCAL_VOLUME_SNAPSHOT_SCHEMA_VERSION + 1;
    assert_eq!(
        validate_snapshot(&changed).unwrap_err(),
        "local-volume-snapshot-version-invalid"
    );

    let mut changed = live.clone();
    changed.observed_at_ms = 0;
    assert_eq!(
        validate_snapshot(&changed).unwrap_err(),
        "local-volume-snapshot-shape-invalid"
    );

    let mut changed = live.clone();
    changed.total_bytes = 0;
    assert_eq!(
        validate_snapshot(&changed).unwrap_err(),
        "local-volume-snapshot-shape-invalid"
    );

    let mut changed = live.clone();
    changed.free_bytes = changed.total_bytes + 1;
    assert_eq!(
        validate_snapshot(&changed).unwrap_err(),
        "local-volume-snapshot-shape-invalid"
    );

    let mut changed = live.clone();
    changed.available_bytes = changed.free_bytes + 1;
    assert_eq!(
        validate_snapshot(&changed).unwrap_err(),
        "local-volume-snapshot-shape-invalid"
    );

    let mut changed = live.clone();
    changed.allocation_granularity_bytes = 0;
    assert_eq!(
        validate_snapshot(&changed).unwrap_err(),
        "local-volume-snapshot-shape-invalid"
    );

    let mut changed = live.clone();
    changed.used_bytes = changed.used_bytes.saturating_add(1);
    assert_eq!(
        validate_snapshot(&changed).unwrap_err(),
        "local-volume-snapshot-shape-invalid"
    );

    let mut changed = live.clone();
    changed.available_basis_points ^= 1;
    assert_eq!(
        validate_snapshot(&changed).unwrap_err(),
        "local-volume-snapshot-derived-fields-invalid"
    );

    let mut changed = live.clone();
    changed.pressure = if changed.pressure == LocalVolumePressure::Critical {
        LocalVolumePressure::Normal
    } else {
        LocalVolumePressure::Critical
    };
    assert_eq!(
        validate_snapshot(&changed).unwrap_err(),
        "local-volume-snapshot-derived-fields-invalid"
    );

    let mut changed = live.clone();
    changed.limitations.push("invented-limitation".into());
    assert_eq!(
        validate_snapshot(&changed).unwrap_err(),
        "local-volume-snapshot-derived-fields-invalid"
    );

    let mut changed = live.clone();
    changed.evidence_fingerprint = "A".repeat(64);
    assert_eq!(
        validate_snapshot(&changed).unwrap_err(),
        "local-volume-snapshot-fingerprint-invalid"
    );

    let mut changed = live.clone();
    changed.evidence_fingerprint = tamper_lower_hex(&changed.evidence_fingerprint);
    assert_eq!(
        validate_snapshot(&changed).unwrap_err(),
        "local-volume-snapshot-fingerprint-invalid"
    );
}

#[test]
fn comparison_covers_increase_decrease_unchanged_and_capacity_change() {
    let temp = tempfile::tempdir().unwrap();
    let template = snapshot_volume(temp.path(), 1).unwrap();
    let before = synthetic_snapshot(&template, 10_000, 4_000, 3_000, 10);

    let unchanged = synthetic_snapshot(&template, 10_000, 4_000, 3_000, 20);
    let comparison = compare_snapshots(&before, &unchanged, None).unwrap();
    assert_eq!(
        comparison.available_change,
        ByteChange {
            direction: ByteChangeDirection::Unchanged,
            bytes: 0,
        }
    );
    assert_eq!(comparison.free_change.direction, ByteChangeDirection::Unchanged);
    assert_eq!(
        comparison.reason_codes,
        vec!["concurrent-filesystem-activity-unattributed".to_string()]
    );
    assert_eq!(comparison.physical_reclaim_bytes, None);
    assert_eq!(
        comparison.physical_reclaim_attribution,
        PhysicalReclaimAttribution::Unproven
    );
    validate_comparison(&comparison).unwrap();

    let increased = synthetic_snapshot(&template, 10_000, 5_000, 4_500, 20);
    let comparison = compare_snapshots(&before, &increased, Some(0)).unwrap();
    assert_eq!(
        comparison.available_change,
        ByteChange {
            direction: ByteChangeDirection::Increased,
            bytes: 1_500,
        }
    );
    assert_eq!(comparison.free_change.bytes, 1_000);
    assert!(comparison
        .reason_codes
        .contains(&"logical-removal-does-not-prove-physical-reclaim".into()));
    assert!(!comparison
        .reason_codes
        .contains(&"available-space-decreased-despite-logical-removal".into()));
    validate_comparison(&comparison).unwrap();

    let decreased = synthetic_snapshot(&template, 10_000, 2_000, 1_500, 20);
    let comparison = compare_snapshots(&before, &decreased, Some(1_000)).unwrap();
    assert_eq!(
        comparison.available_change,
        ByteChange {
            direction: ByteChangeDirection::Decreased,
            bytes: 1_500,
        }
    );
    assert_eq!(comparison.free_change.bytes, 2_000);
    assert!(comparison
        .reason_codes
        .contains(&"available-space-decreased-despite-logical-removal".into()));
    validate_comparison(&comparison).unwrap();

    let capacity_changed = synthetic_snapshot(&template, 12_000, 4_000, 3_000, 20);
    let comparison = compare_snapshots(&before, &capacity_changed, None).unwrap();
    assert!(!comparison.total_bytes_stable);
    assert!(comparison
        .reason_codes
        .contains(&"volume-capacity-changed".into()));
    validate_comparison(&comparison).unwrap();
}

#[test]
fn comparison_validation_rejects_schema_shape_claim_and_fingerprint_tampering() {
    let temp = tempfile::tempdir().unwrap();
    let template = snapshot_volume(temp.path(), 1).unwrap();
    let before = synthetic_snapshot(&template, 10_000, 4_000, 3_000, 10);
    let after = synthetic_snapshot(&template, 10_000, 5_000, 4_500, 20);
    let valid = compare_snapshots(&before, &after, None).unwrap();
    assert_eq!(valid.schema_version, LOCAL_VOLUME_COMPARISON_SCHEMA_VERSION);

    let mut changed = valid.clone();
    changed.schema_version = LOCAL_VOLUME_COMPARISON_SCHEMA_VERSION + 1;
    assert_eq!(
        validate_comparison(&changed).unwrap_err(),
        "local-volume-comparison-version-invalid"
    );

    let mut changed = valid.clone();
    changed.before.schema_version = LOCAL_VOLUME_SNAPSHOT_SCHEMA_VERSION + 1;
    assert_eq!(
        validate_comparison(&changed).unwrap_err(),
        "local-volume-snapshot-version-invalid"
    );

    let mut changed = valid.clone();
    changed.after = synthetic_snapshot(&template, 10_000, 5_000, 4_500, 5);
    assert_eq!(
        validate_comparison(&changed).unwrap_err(),
        "local-volume-comparison-shape-invalid"
    );

    let mut changed = valid.clone();
    changed.observed_elapsed_ms += 1;
    assert_eq!(
        validate_comparison(&changed).unwrap_err(),
        "local-volume-comparison-shape-invalid"
    );

    let mut changed = valid.clone();
    changed.total_bytes_stable = false;
    assert_eq!(
        validate_comparison(&changed).unwrap_err(),
        "local-volume-comparison-shape-invalid"
    );

    let mut changed = valid.clone();
    changed.physical_reclaim_bytes = Some(1);
    assert_eq!(
        validate_comparison(&changed).unwrap_err(),
        "local-volume-comparison-shape-invalid"
    );

    let mut changed = valid.clone();
    changed.available_change.bytes += 1;
    assert_eq!(
        validate_comparison(&changed).unwrap_err(),
        "local-volume-comparison-shape-invalid"
    );

    let mut changed = valid.clone();
    changed.free_change.direction = ByteChangeDirection::Decreased;
    assert_eq!(
        validate_comparison(&changed).unwrap_err(),
        "local-volume-comparison-shape-invalid"
    );

    let mut changed = valid.clone();
    changed.logical_removed_bytes = Some(1);
    assert_eq!(
        validate_comparison(&changed).unwrap_err(),
        "local-volume-comparison-shape-invalid"
    );

    let mut changed = valid.clone();
    changed.reason_codes.push("invented-reason".into());
    assert_eq!(
        validate_comparison(&changed).unwrap_err(),
        "local-volume-comparison-shape-invalid"
    );

    let mut changed = valid.clone();
    changed.evidence_fingerprint = "A".repeat(64);
    assert_eq!(
        validate_comparison(&changed).unwrap_err(),
        "local-volume-comparison-fingerprint-invalid"
    );

    let mut changed = valid;
    changed.evidence_fingerprint = tamper_lower_hex(&changed.evidence_fingerprint);
    assert_eq!(
        validate_comparison(&changed).unwrap_err(),
        "local-volume-comparison-fingerprint-invalid"
    );
}
