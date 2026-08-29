//! Destination-filesystem headroom authority for native cloud copies.
//!
//! DiskSage stages native File Provider copies below the final destination parent. A source file
//! may live on a different filesystem, so source-volume capacity cannot authorize or veto that
//! staging mutation. The probe therefore resolves the nearest existing destination ancestor; any
//! missing descendants will be created on that same filesystem before the staging file exists.

use std::path::{Path, PathBuf};

fn destination_volume_probe_path(destination: &Path) -> Result<PathBuf, String> {
    let mut probe = destination
        .parent()
        .ok_or_else(|| "local-volume-headroom-destination-parent-missing".to_string())?;
    loop {
        match std::fs::symlink_metadata(probe) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err("local-volume-headroom-destination-parent-unsafe".into());
                }
                return Ok(probe.to_path_buf());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                probe = probe
                    .parent()
                    .ok_or_else(|| "local-volume-headroom-destination-parent-missing".to_string())?;
            }
            Err(_) => return Err("local-volume-headroom-destination-parent-unavailable".into()),
        }
    }
}

pub(crate) fn require_destination_copy_headroom(
    destination: &Path,
    candidate_bytes: u64,
    observed_at_ms: u64,
) -> Result<(), String> {
    let probe = destination_volume_probe_path(destination)?;
    let snapshot = crate::volume_pressure::snapshot_volume(&probe, observed_at_ms)?;
    if crate::volume_pressure::has_copy_headroom(snapshot.available_bytes, candidate_bytes) {
        Ok(())
    } else {
        Err("local-volume-headroom-insufficient".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_destination_descendants_probe_the_existing_destination_filesystem() {
        let root = tempfile::tempdir().unwrap();
        let destination = root
            .path()
            .join("DiskSage Archive")
            .join("documents")
            .join("report.pdf");

        assert_eq!(destination_volume_probe_path(&destination).unwrap(), root.path());
    }

    #[test]
    fn nearest_existing_destination_parent_is_authoritative() {
        let root = tempfile::tempdir().unwrap();
        let existing = root.path().join("DiskSage Archive");
        std::fs::create_dir(&existing).unwrap();
        let destination = existing.join("documents").join("report.pdf");

        assert_eq!(destination_volume_probe_path(&destination).unwrap(), existing);
    }

    #[test]
    fn real_destination_statvfs_preserves_the_bounded_headroom_error() {
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("archive").join("report.pdf");

        assert_eq!(
            require_destination_copy_headroom(&destination, u64::MAX, 1),
            Err("local-volume-headroom-insufficient".into())
        );
    }

    #[cfg(unix)]
    #[test]
    fn existing_symlink_destination_parent_is_not_capacity_authority() {
        let root = tempfile::tempdir().unwrap();
        let actual = root.path().join("actual");
        std::fs::create_dir(&actual).unwrap();
        let linked = root.path().join("linked");
        std::os::unix::fs::symlink(&actual, &linked).unwrap();

        assert_eq!(
            destination_volume_probe_path(&linked.join("report.pdf")),
            Err("local-volume-headroom-destination-parent-unsafe".into())
        );
    }
}
