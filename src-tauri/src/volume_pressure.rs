use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::io;
use std::io::Write as _;
use std::path::{Path, PathBuf};

pub const LOCAL_VOLUME_SNAPSHOT_SCHEMA_VERSION: u32 = 1;
pub const LOCAL_VOLUME_COMPARISON_SCHEMA_VERSION: u32 = 1;
pub const LOCAL_VOLUME_EVIDENCE_DIRECTORY: &str = "volume-pressure-evidence";
const MAX_PERSISTED_SNAPSHOTS: usize = 128;
const MAX_PERSISTED_SNAPSHOT_BYTES: usize = 64 * 1024;
/// Keep enough local space for File Provider staging and filesystem metadata while copying one
/// candidate. This is separate from remote cloud capacity and is checked again at the mutation
/// boundary.
pub const LOCAL_COPY_RESERVE_BYTES: u64 = 1024 * 1024 * 1024;

const LIMITATIONS: [&str; 3] = [
    "shared-filesystem-concurrency-unattributed",
    "logical-removal-does-not-prove-physical-reclaim",
    "filesystem-snapshot-is-not-cloud-provider-capacity",
];

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LocalVolumePressure {
    Normal,
    Elevated,
    High,
    Critical,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LocalVolumeEvidenceKind {
    FilesystemNativeStatvfs,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocalVolumeSnapshot {
    pub schema_version: u32,
    pub observed_at_ms: u64,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
    pub available_basis_points: u16,
    pub allocation_granularity_bytes: u64,
    pub pressure: LocalVolumePressure,
    pub evidence_kind: LocalVolumeEvidenceKind,
    pub limitations: Vec<String>,
    pub evidence_fingerprint: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ByteChangeDirection {
    Increased,
    Decreased,
    Unchanged,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ByteChange {
    pub direction: ByteChangeDirection,
    pub bytes: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PhysicalReclaimAttribution {
    Unproven,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocalVolumeComparison {
    pub schema_version: u32,
    pub before: LocalVolumeSnapshot,
    pub after: LocalVolumeSnapshot,
    pub observed_elapsed_ms: u64,
    pub total_bytes_stable: bool,
    pub available_change: ByteChange,
    pub free_change: ByteChange,
    pub logical_removed_bytes: Option<u64>,
    pub physical_reclaim_bytes: Option<u64>,
    pub physical_reclaim_attribution: PhysicalReclaimAttribution,
    pub reason_codes: Vec<String>,
    pub evidence_fingerprint: String,
}

pub fn snapshot_volume(path: &Path, observed_at_ms: u64) -> Result<LocalVolumeSnapshot, String> {
    if observed_at_ms == 0 {
        return Err("local-volume-observed-time-invalid".into());
    }
    let stats = fs4::statvfs(path).map_err(|error| statvfs_error_reason(error.kind()))?;
    snapshot_from_stats(
        stats.total_space(),
        stats.free_space(),
        stats.available_space(),
        stats.allocation_granularity(),
        observed_at_ms,
    )
}

/// Persist one path-free capacity observation for later incident comparison.
///
/// The record is create-only, read-only, fsynced, and named by its observation time and content
/// fingerprint. Retention is bounded and only files matching DiskSage's own record shape are
/// pruned; provider databases, user files, and unrelated app-data entries are never touched.
pub fn write_snapshot_evidence(
    app_data_dir: &Path,
    snapshot: &LocalVolumeSnapshot,
) -> Result<PathBuf, String> {
    validate_snapshot(snapshot)?;
    let directory = snapshot_evidence_directory(app_data_dir)?;
    let path = directory.join(format!(
        "{:020}-{}.json",
        snapshot.observed_at_ms, snapshot.evidence_fingerprint
    ));
    let encoded = serde_json::to_vec_pretty(snapshot)
        .map_err(|_| "local-volume-evidence-encode-failed".to_string())?;
    if encoded.len() > MAX_PERSISTED_SNAPSHOT_BYTES {
        return Err("local-volume-evidence-too-large".into());
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o400);
    }
    let mut file = options
        .open(&path)
        .map_err(|_| "local-volume-evidence-create-failed".to_string())?;
    let result = (|| -> Result<(), String> {
        file.write_all(&encoded)
            .and_then(|_| file.sync_all())
            .map_err(|_| "local-volume-evidence-write-failed".to_string())?;
        #[cfg(not(unix))]
        file.set_len(encoded.len() as u64)
            .map_err(|_| "local-volume-evidence-write-failed".to_string())?;
        #[cfg(unix)]
        std::fs::File::open(&directory)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| "local-volume-evidence-directory-sync-failed".to_string())?;
        Ok(())
    })();
    if let Err(error) = result {
        drop(file);
        let _ = std::fs::remove_file(&path);
        return Err(error);
    }
    prune_snapshot_evidence(&directory)?;
    Ok(path)
}

fn snapshot_evidence_directory(app_data_dir: &Path) -> Result<PathBuf, String> {
    if !app_data_dir.is_absolute()
        || app_data_dir
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("local-volume-evidence-parent-invalid".into());
    }
    std::fs::create_dir_all(app_data_dir)
        .map_err(|_| "local-volume-evidence-parent-create-failed".to_string())?;
    let parent = std::fs::symlink_metadata(app_data_dir)
        .map_err(|_| "local-volume-evidence-parent-unavailable".to_string())?;
    if parent.file_type().is_symlink() || !parent.is_dir() {
        return Err("local-volume-evidence-parent-unsafe".into());
    }
    let directory = app_data_dir.join(LOCAL_VOLUME_EVIDENCE_DIRECTORY);
    std::fs::create_dir_all(&directory)
        .map_err(|_| "local-volume-evidence-directory-create-failed".to_string())?;
    let metadata = std::fs::symlink_metadata(&directory)
        .map_err(|_| "local-volume-evidence-directory-unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("local-volume-evidence-directory-unsafe".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "local-volume-evidence-directory-permissions-failed".to_string())?;
    }
    Ok(directory)
}

fn is_snapshot_record_name(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".json") else {
        return false;
    };
    let Some((timestamp, fingerprint)) = stem.split_once('-') else {
        return false;
    };
    timestamp.len() == 20
        && timestamp.bytes().all(|byte| byte.is_ascii_digit())
        && is_lower_hex_64(fingerprint)
}

fn prune_snapshot_evidence(directory: &Path) -> Result<(), String> {
    let mut records = std::fs::read_dir(directory)
        .map_err(|_| "local-volume-evidence-directory-read-failed".to_string())?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            is_snapshot_record_name(&name).then_some((name, entry.path()))
        })
        .collect::<Vec<_>>();
    records.sort_by(|left, right| left.0.cmp(&right.0));
    while records.len() > MAX_PERSISTED_SNAPSHOTS {
        let (_, path) = records.remove(0);
        std::fs::remove_file(path).map_err(|_| "local-volume-evidence-retention-failed")?;
    }
    Ok(())
}

pub fn has_copy_headroom(available_bytes: u64, candidate_bytes: u64) -> bool {
    candidate_bytes
        .checked_add(LOCAL_COPY_RESERVE_BYTES)
        .is_some_and(|required| available_bytes >= required)
}

pub fn validate_snapshot(snapshot: &LocalVolumeSnapshot) -> Result<(), String> {
    if snapshot.schema_version != LOCAL_VOLUME_SNAPSHOT_SCHEMA_VERSION {
        return Err("local-volume-snapshot-version-invalid".into());
    }
    if snapshot.observed_at_ms == 0
        || snapshot.total_bytes == 0
        || snapshot.free_bytes > snapshot.total_bytes
        || snapshot.available_bytes > snapshot.free_bytes
        || snapshot.allocation_granularity_bytes == 0
        || snapshot.used_bytes != snapshot.total_bytes - snapshot.free_bytes
    {
        return Err("local-volume-snapshot-shape-invalid".into());
    }
    let expected_basis_points =
        available_basis_points(snapshot.total_bytes, snapshot.available_bytes);
    if snapshot.available_basis_points != expected_basis_points
        || snapshot.pressure != pressure_for(expected_basis_points)
        || snapshot.evidence_kind != LocalVolumeEvidenceKind::FilesystemNativeStatvfs
        || snapshot.limitations != limitation_codes()
    {
        return Err("local-volume-snapshot-derived-fields-invalid".into());
    }
    if !is_lower_hex_64(&snapshot.evidence_fingerprint)
        || snapshot.evidence_fingerprint != snapshot_fingerprint(snapshot)?
    {
        return Err("local-volume-snapshot-fingerprint-invalid".into());
    }
    Ok(())
}

pub fn compare_snapshots(
    before: &LocalVolumeSnapshot,
    after: &LocalVolumeSnapshot,
    logical_removed_bytes: Option<u64>,
) -> Result<LocalVolumeComparison, String> {
    validate_snapshot(before)?;
    validate_snapshot(after)?;
    if after.observed_at_ms < before.observed_at_ms {
        return Err("local-volume-comparison-time-invalid".into());
    }

    let total_bytes_stable = before.total_bytes == after.total_bytes;
    let available_change = byte_change(before.available_bytes, after.available_bytes);
    let reason_codes =
        comparison_reason_codes(total_bytes_stable, available_change, logical_removed_bytes);

    let mut comparison = LocalVolumeComparison {
        schema_version: LOCAL_VOLUME_COMPARISON_SCHEMA_VERSION,
        before: before.clone(),
        after: after.clone(),
        observed_elapsed_ms: after.observed_at_ms - before.observed_at_ms,
        total_bytes_stable,
        available_change,
        free_change: byte_change(before.free_bytes, after.free_bytes),
        logical_removed_bytes,
        physical_reclaim_bytes: None,
        physical_reclaim_attribution: PhysicalReclaimAttribution::Unproven,
        reason_codes,
        evidence_fingerprint: String::new(),
    };
    comparison.evidence_fingerprint = comparison_fingerprint(&comparison)?;
    validate_comparison(&comparison)?;
    Ok(comparison)
}

pub fn validate_comparison(comparison: &LocalVolumeComparison) -> Result<(), String> {
    if comparison.schema_version != LOCAL_VOLUME_COMPARISON_SCHEMA_VERSION {
        return Err("local-volume-comparison-version-invalid".into());
    }
    validate_snapshot(&comparison.before)?;
    validate_snapshot(&comparison.after)?;
    if comparison.after.observed_at_ms < comparison.before.observed_at_ms
        || comparison.observed_elapsed_ms
            != comparison.after.observed_at_ms - comparison.before.observed_at_ms
        || comparison.total_bytes_stable
            != (comparison.before.total_bytes == comparison.after.total_bytes)
        || comparison.physical_reclaim_bytes.is_some()
        || comparison.physical_reclaim_attribution != PhysicalReclaimAttribution::Unproven
        || comparison.available_change
            != byte_change(
                comparison.before.available_bytes,
                comparison.after.available_bytes,
            )
        || comparison.free_change
            != byte_change(comparison.before.free_bytes, comparison.after.free_bytes)
        || comparison.reason_codes
            != comparison_reason_codes(
                comparison.total_bytes_stable,
                comparison.available_change,
                comparison.logical_removed_bytes,
            )
    {
        return Err("local-volume-comparison-shape-invalid".into());
    }
    if !is_lower_hex_64(&comparison.evidence_fingerprint)
        || comparison.evidence_fingerprint != comparison_fingerprint(comparison)?
    {
        return Err("local-volume-comparison-fingerprint-invalid".into());
    }
    Ok(())
}

fn snapshot_from_stats(
    total_bytes: u64,
    free_bytes: u64,
    available_bytes: u64,
    allocation_granularity_bytes: u64,
    observed_at_ms: u64,
) -> Result<LocalVolumeSnapshot, String> {
    if observed_at_ms == 0
        || total_bytes == 0
        || free_bytes > total_bytes
        || available_bytes > free_bytes
        || allocation_granularity_bytes == 0
    {
        return Err("local-volume-native-stats-invalid".into());
    }
    let available_basis_points = available_basis_points(total_bytes, available_bytes);
    let mut snapshot = LocalVolumeSnapshot {
        schema_version: LOCAL_VOLUME_SNAPSHOT_SCHEMA_VERSION,
        observed_at_ms,
        total_bytes,
        free_bytes,
        available_bytes,
        used_bytes: total_bytes - free_bytes,
        available_basis_points,
        allocation_granularity_bytes,
        pressure: pressure_for(available_basis_points),
        evidence_kind: LocalVolumeEvidenceKind::FilesystemNativeStatvfs,
        limitations: limitation_codes(),
        evidence_fingerprint: String::new(),
    };
    snapshot.evidence_fingerprint = snapshot_fingerprint(&snapshot)?;
    validate_snapshot(&snapshot)?;
    Ok(snapshot)
}

fn available_basis_points(total_bytes: u64, available_bytes: u64) -> u16 {
    ((u128::from(available_bytes) * 10_000) / u128::from(total_bytes)) as u16
}

fn pressure_for(available_basis_points: u16) -> LocalVolumePressure {
    match available_basis_points {
        0..=500 => LocalVolumePressure::Critical,
        501..=1_000 => LocalVolumePressure::High,
        1_001..=2_000 => LocalVolumePressure::Elevated,
        _ => LocalVolumePressure::Normal,
    }
}

fn byte_change(before: u64, after: u64) -> ByteChange {
    match after.cmp(&before) {
        std::cmp::Ordering::Greater => ByteChange {
            direction: ByteChangeDirection::Increased,
            bytes: after - before,
        },
        std::cmp::Ordering::Less => ByteChange {
            direction: ByteChangeDirection::Decreased,
            bytes: before - after,
        },
        std::cmp::Ordering::Equal => ByteChange {
            direction: ByteChangeDirection::Unchanged,
            bytes: 0,
        },
    }
}

fn comparison_reason_codes(
    total_bytes_stable: bool,
    available_change: ByteChange,
    logical_removed_bytes: Option<u64>,
) -> Vec<String> {
    let mut reason_codes = vec!["concurrent-filesystem-activity-unattributed".to_string()];
    if logical_removed_bytes.is_some() {
        reason_codes.push("logical-removal-does-not-prove-physical-reclaim".to_string());
    }
    if logical_removed_bytes.is_some_and(|bytes| bytes > 0)
        && available_change.direction == ByteChangeDirection::Decreased
    {
        reason_codes.push("available-space-decreased-despite-logical-removal".to_string());
    }
    if !total_bytes_stable {
        reason_codes.push("volume-capacity-changed".to_string());
    }
    reason_codes
}

fn limitation_codes() -> Vec<String> {
    LIMITATIONS
        .iter()
        .map(|value| (*value).to_string())
        .collect()
}

fn statvfs_error_reason(kind: io::ErrorKind) -> String {
    match kind {
        io::ErrorKind::NotFound => "local-volume-path-not-found",
        io::ErrorKind::PermissionDenied => "local-volume-path-permission-denied",
        _ => "local-volume-statvfs-unavailable",
    }
    .into()
}

fn snapshot_fingerprint(snapshot: &LocalVolumeSnapshot) -> Result<String, String> {
    let mut payload = snapshot.clone();
    payload.evidence_fingerprint.clear();
    json_fingerprint(&payload)
}

fn comparison_fingerprint(comparison: &LocalVolumeComparison) -> Result<String, String> {
    let mut payload = comparison.clone();
    payload.evidence_fingerprint.clear();
    json_fingerprint(&payload)
}

fn json_fingerprint<T: Serialize>(value: &T) -> Result<String, String> {
    let encoded =
        serde_json::to_vec(value).map_err(|_| "local-volume-fingerprint-encode-failed")?;
    let digest = Sha256::digest(encoded);
    let mut output = String::with_capacity(64);
    for byte in digest {
        write!(&mut output, "{byte:02x}").map_err(|_| "local-volume-fingerprint-encode-failed")?;
    }
    Ok(output)
}

fn is_lower_hex_64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(
        total_bytes: u64,
        free_bytes: u64,
        available_bytes: u64,
        observed_at_ms: u64,
    ) -> LocalVolumeSnapshot {
        snapshot_from_stats(
            total_bytes,
            free_bytes,
            available_bytes,
            4_096,
            observed_at_ms,
        )
        .unwrap()
    }

    #[test]
    fn pressure_uses_available_space_not_logical_free_space() {
        let high = snapshot(1_000, 300, 75, 1);
        assert_eq!(high.used_bytes, 700);
        assert_eq!(high.available_basis_points, 750);
        assert_eq!(high.pressure, LocalVolumePressure::High);
        assert!(validate_snapshot(&high).is_ok());

        assert_eq!(
            snapshot(1_000, 100, 50, 1).pressure,
            LocalVolumePressure::Critical
        );
        assert_eq!(
            snapshot(1_000, 200, 100, 1).pressure,
            LocalVolumePressure::High
        );
        assert_eq!(
            snapshot(1_000, 300, 200, 1).pressure,
            LocalVolumePressure::Elevated
        );
        assert_eq!(
            snapshot(1_000, 400, 201, 1).pressure,
            LocalVolumePressure::Normal
        );
    }

    #[test]
    fn copy_headroom_requires_candidate_and_reserve_without_overflow() {
        assert!(has_copy_headroom(LOCAL_COPY_RESERVE_BYTES + 10, 10));
        assert!(!has_copy_headroom(LOCAL_COPY_RESERVE_BYTES + 9, 10));
        assert!(!has_copy_headroom(u64::MAX, u64::MAX));
    }

    #[test]
    fn snapshot_is_path_redacted_and_integrity_bound() {
        let mut value = snapshot(10_000, 2_000, 1_500, 10);
        let encoded = serde_json::to_string(&value).unwrap();
        assert!(!encoded.contains("path"));
        assert!(!encoded.contains("mount"));
        assert_eq!(value.evidence_fingerprint.len(), 64);

        value.available_bytes -= 1;
        assert_eq!(
            validate_snapshot(&value).unwrap_err(),
            "local-volume-snapshot-derived-fields-invalid"
        );
    }

    #[test]
    fn comparison_never_claims_physical_reclaim_attribution() {
        let before = snapshot(10_000, 2_000, 1_500, 10);
        let after = snapshot(10_000, 1_000, 500, 20);
        let comparison = compare_snapshots(&before, &after, Some(3_000)).unwrap();
        assert_eq!(
            comparison.available_change,
            ByteChange {
                direction: ByteChangeDirection::Decreased,
                bytes: 1_000,
            }
        );
        assert_eq!(comparison.physical_reclaim_bytes, None);
        assert_eq!(
            comparison.physical_reclaim_attribution,
            PhysicalReclaimAttribution::Unproven
        );
        assert!(comparison
            .reason_codes
            .contains(&"available-space-decreased-despite-logical-removal".into()));
        assert!(validate_comparison(&comparison).is_ok());
    }

    #[test]
    fn comparison_rejects_time_reversal_and_tampering() {
        let before = snapshot(10_000, 2_000, 1_500, 20);
        let after = snapshot(10_000, 3_000, 2_500, 10);
        assert_eq!(
            compare_snapshots(&before, &after, None).unwrap_err(),
            "local-volume-comparison-time-invalid"
        );

        let after = snapshot(10_000, 3_000, 2_500, 30);
        let mut comparison = compare_snapshots(&before, &after, None).unwrap();
        comparison.after.available_bytes += 1;
        assert_eq!(
            validate_comparison(&comparison).unwrap_err(),
            "local-volume-snapshot-derived-fields-invalid"
        );
    }

    #[test]
    fn comparison_rejects_derived_field_and_reason_tampering() {
        let before = snapshot(10_000, 2_000, 1_500, 10);
        let after = snapshot(10_000, 3_000, 2_500, 20);

        let mut changed = compare_snapshots(&before, &after, None).unwrap();
        changed.available_change.bytes += 1;
        assert_eq!(
            validate_comparison(&changed).unwrap_err(),
            "local-volume-comparison-shape-invalid"
        );

        let mut changed = compare_snapshots(&before, &after, None).unwrap();
        changed.reason_codes.push("invented-reason".into());
        assert_eq!(
            validate_comparison(&changed).unwrap_err(),
            "local-volume-comparison-shape-invalid"
        );

        let mut changed = compare_snapshots(&before, &after, None).unwrap();
        changed.observed_elapsed_ms += 1;
        assert_eq!(
            validate_comparison(&changed).unwrap_err(),
            "local-volume-comparison-shape-invalid"
        );
    }

    #[test]
    fn invalid_native_stats_fail_closed() {
        for result in [
            snapshot_from_stats(0, 0, 0, 4_096, 1),
            snapshot_from_stats(1, 2, 0, 4_096, 1),
            snapshot_from_stats(2, 1, 2, 4_096, 1),
            snapshot_from_stats(2, 1, 1, 0, 1),
            snapshot_from_stats(2, 1, 1, 4_096, 0),
        ] {
            assert_eq!(result.unwrap_err(), "local-volume-native-stats-invalid");
        }
    }

    #[test]
    fn live_snapshot_has_no_path_and_is_self_consistent() {
        let temp = tempfile::tempdir().unwrap();
        let live = snapshot_volume(temp.path(), 1).unwrap();
        assert!(live.total_bytes > 0);
        assert!(live.available_bytes <= live.free_bytes);
        assert!(validate_snapshot(&live).is_ok());
        let encoded = serde_json::to_string(&live).unwrap();
        assert!(!encoded.contains(&temp.path().to_string_lossy().to_string()));
    }

    #[test]
    fn snapshot_evidence_is_immutable_path_free_and_bounded() {
        let temp = tempfile::tempdir().unwrap();
        let unrelated = temp
            .path()
            .join(LOCAL_VOLUME_EVIDENCE_DIRECTORY)
            .join("keep.txt");
        std::fs::create_dir_all(unrelated.parent().unwrap()).unwrap();
        std::fs::write(&unrelated, b"unrelated").unwrap();

        let first = snapshot(10_000, 2_000, 1_500, 1);
        let first_path = write_snapshot_evidence(temp.path(), &first).unwrap();
        assert_eq!(
            serde_json::from_slice::<LocalVolumeSnapshot>(&std::fs::read(&first_path).unwrap())
                .unwrap(),
            first
        );
        assert_eq!(
            write_snapshot_evidence(temp.path(), &first).unwrap_err(),
            "local-volume-evidence-create-failed"
        );
        assert_eq!(std::fs::read(&unrelated).unwrap(), b"unrelated");

        for observed_at_ms in 2..=129 {
            write_snapshot_evidence(temp.path(), &snapshot(10_000, 2_000, 1_500, observed_at_ms))
                .unwrap();
        }
        let records = std::fs::read_dir(temp.path().join(LOCAL_VOLUME_EVIDENCE_DIRECTORY))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(is_snapshot_record_name)
            })
            .count();
        assert_eq!(records, MAX_PERSISTED_SNAPSHOTS);
        assert!(!first_path.exists());
    }

    #[test]
    fn snapshot_evidence_rejects_relative_parent() {
        let snapshot = snapshot(10_000, 2_000, 1_500, 1);
        assert_eq!(
            write_snapshot_evidence(Path::new("relative"), &snapshot).unwrap_err(),
            "local-volume-evidence-parent-invalid"
        );
    }
}
