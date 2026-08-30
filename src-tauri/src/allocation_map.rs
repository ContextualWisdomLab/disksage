//! Bounded, read-only allocated-block inventory for customer-selected roots.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AllocationMapEntry {
    pub root: String,
    pub allocated_bytes: u64,
    pub visited_entries: u64,
    pub classification: &'static str,
    pub evidence_complete: bool,
    pub stop_reason: Option<&'static str>,
}

fn classification(path: &Path) -> &'static str {
    let value = path.to_string_lossy();
    if value.contains("/Library/Mobile Documents/")
        || value.ends_with("/Library/Mobile Documents")
        || value.contains("/Library/CloudStorage/")
        || value.contains("/Library/Application Support/OneDrive")
        || value.contains("/Library/Application Support/FileProvider")
        || value.contains("/Library/Application Support/CloudDocs")
    {
        "provider-managed"
    } else if value.contains(".photoslibrary") || value.contains(".photolibrary") {
        "photos-managed"
    } else if value.contains("/Parallels/")
        || value.ends_with("/Parallels")
        || value.contains("/.colima/")
        || value.contains("/containers/podman/")
        || value.contains("/private/var/vm")
    {
        "virtual-machine-managed"
    } else if value.contains("/private/var/folders/") {
        "user-cache-and-temporary"
    } else if value.contains("/Caches/")
        || value.ends_with("/Caches")
        || value.contains("/.cache/")
        || value.ends_with("/.cache")
    {
        "cache"
    } else if value.ends_with("/target")
        || value.ends_with("/node_modules")
        || value.contains("/.cargo/registry")
    {
        "generated"
    } else {
        "user-or-application-data"
    }
}

/// Sum allocated filesystem blocks without following symlinks or crossing a device boundary.
pub fn measure_root(
    root: &Path,
    max_entries: u64,
    max_duration: Duration,
) -> Result<AllocationMapEntry, String> {
    use std::os::unix::fs::MetadataExt;
    if !root.is_absolute() || max_entries == 0 || max_duration.is_zero() {
        return Err("allocation-map-options-invalid".into());
    }
    let root_metadata = std::fs::symlink_metadata(root)
        .map_err(|_| "allocation-map-root-unavailable".to_string())?;
    if root_metadata.file_type().is_symlink() {
        return Err("allocation-map-root-symlink-rejected".into());
    }
    let device = root_metadata.dev();
    let started = Instant::now();
    let mut stack = vec![root.to_path_buf()];
    let mut allocated_bytes = 0_u64;
    let mut visited_entries = 0_u64;
    let mut stop_reason = None;
    while let Some(path) = stack.pop() {
        if visited_entries >= max_entries {
            stop_reason = Some("entry-limit-reached");
            break;
        }
        if started.elapsed() >= max_duration {
            stop_reason = Some("duration-limit-reached");
            break;
        }
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(value) => value,
            Err(_) => {
                stop_reason = Some("metadata-unavailable");
                break;
            }
        };
        if metadata.dev() != device {
            continue;
        }
        visited_entries += 1;
        allocated_bytes = allocated_bytes.saturating_add(metadata.blocks().saturating_mul(512));
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            let mut children = match std::fs::read_dir(&path) {
                Ok(entries) => entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .collect::<Vec<PathBuf>>(),
                Err(_) => {
                    stop_reason = Some("directory-unreadable");
                    break;
                }
            };
            children.sort();
            stack.extend(children.into_iter().rev());
        }
    }
    Ok(AllocationMapEntry {
        root: root.to_string_lossy().into_owned(),
        allocated_bytes,
        visited_entries,
        classification: classification(root),
        evidence_complete: stop_reason.is_none(),
        stop_reason,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_entries_and_never_follows_symlinks() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        let outside = temp.path().join("outside");
        std::fs::create_dir(&root).unwrap();
        std::fs::write(&outside, vec![0_u8; 4096]).unwrap();
        symlink(&outside, root.join("link")).unwrap();
        let report = measure_root(&root, 2, Duration::from_secs(1)).unwrap();
        assert_eq!(report.visited_entries, 2);
        assert!(report.evidence_complete);
        assert!(report.allocated_bytes < 4096);
        assert_eq!(
            measure_root(&root, 1, Duration::from_secs(1))
                .unwrap()
                .stop_reason,
            Some("entry-limit-reached")
        );
    }

    #[test]
    fn classifies_provider_vm_and_per_user_var_boundaries() {
        assert_eq!(
            classification(Path::new("/Users/test/Library/Mobile Documents")),
            "provider-managed"
        );
        assert_eq!(
            classification(Path::new("/Users/test/Parallels")),
            "virtual-machine-managed"
        );
        assert_eq!(
            classification(Path::new("/private/var/folders/aa/token/T")),
            "user-cache-and-temporary"
        );
    }
}
