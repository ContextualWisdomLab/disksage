//! Identity-aware, read-only scan-tree navigation.
//!
//! The UI passes a path selected from a prior scan. Lexical ancestry alone is not sufficient
//! authority because a final directory symlink or reparse path can still resolve outside the
//! scanned root. This module canonicalizes the scanned root and requested directory immediately
//! before enumeration, then requires the requested object to remain within that canonical root.
//! It never mutates the filesystem.

use crate::commands::{AppState, EntryView, NodeView};
use crate::scanner::ScanResult;
use std::path::{Component, Path, PathBuf};

const OUTSIDE_ROOT: &str = "path outside scanned root";
const NOT_IN_SCAN: &str = "path unavailable in scan result";

fn canonical_navigation_path(res: &ScanResult, path: &Path) -> Result<PathBuf, String> {
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(OUTSIDE_ROOT.into());
    }
    if !path.starts_with(&res.root) {
        return Err(OUTSIDE_ROOT.into());
    }

    let canonical_root = std::fs::canonicalize(&res.root).map_err(|_| OUTSIDE_ROOT.to_string())?;
    let canonical_path = std::fs::canonicalize(path).map_err(|_| OUTSIDE_ROOT.to_string())?;
    if canonical_path != canonical_root && !canonical_path.starts_with(&canonical_root) {
        return Err(OUTSIDE_ROOT.into());
    }
    Ok(canonical_path)
}

fn entry_is_link_or_reparse(path: &Path, file_type: &std::fs::FileType) -> bool {
    if file_type.is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        return std::fs::symlink_metadata(path)
            .map(|metadata| metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
            .unwrap_or(true);
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

fn scanned_directory_size(
    res: &ScanResult,
    display_path: &Path,
    canonical_path: &Path,
) -> Option<u64> {
    res.dir_sizes
        .get(display_path)
        .or_else(|| res.dir_sizes.get(canonical_path))
        .copied()
}

/// Return one level of scan navigation only when the requested directory resolves inside the
/// canonical scanned root and was actually admitted by the scan. Directories pruned by scanner
/// policy remain absent from navigation even though they still exist on disk.
pub(crate) fn node_view(res: &ScanResult, path: &Path) -> Result<NodeView, String> {
    let canonical_path = canonical_navigation_path(res, path)?;
    let canonical_root = std::fs::canonicalize(&res.root).map_err(|_| OUTSIDE_ROOT.to_string())?;
    let relative = canonical_path
        .strip_prefix(&canonical_root)
        .map_err(|_| OUTSIDE_ROOT.to_string())?;
    // macOS canonicalizes `/var` to `/private/var`; keep scanner keys and UI paths in
    // the original namespace while reading entries through the verified canonical path.
    let display_path = res.root.join(relative);
    let view_size = match scanned_directory_size(res, &display_path, &canonical_path) {
        Some(size) => size,
        None if res.cancelled && display_path == res.root => {
            return Ok(NodeView {
                path: path.to_string_lossy().into_owned(),
                size: 0,
                entries: Vec::new(),
            });
        }
        None => return Err(NOT_IN_SCAN.into()),
    };
    let complete_file_manifest_matches = !res.cancelled
        && res
            .directory_file_manifests
            .get(&display_path)
            .or_else(|| res.directory_file_manifests.get(&canonical_path))
            .is_some_and(|expected| {
                crate::scanner::current_directory_file_manifest(&canonical_path).as_ref()
                    == Some(expected)
            });
    let mut entries = Vec::new();
    for entry in
        std::fs::read_dir(&canonical_path).map_err(|_| "node directory unavailable".to_string())?
    {
        let Ok(entry) = entry else { continue };
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let entry_path = entry.path();
        if entry_is_link_or_reparse(&entry_path, &file_type) {
            continue;
        }
        let display_entry_path = display_path.join(entry.file_name());
        let (size, is_dir) = if file_type.is_dir() {
            let Some(size) = scanned_directory_size(res, &display_entry_path, &entry_path) else {
                continue;
            };
            (size, true)
        } else {
            if !complete_file_manifest_matches
                && !res.admitted_files.contains(&display_entry_path)
                && !res.admitted_files.contains(&entry_path)
            {
                continue;
            }
            (
                std::fs::symlink_metadata(&entry_path)
                    .map(|metadata| metadata.len())
                    .unwrap_or_default(),
                false,
            )
        };
        entries.push(EntryView {
            name: entry.file_name().to_string_lossy().into_owned(),
            path: display_entry_path.to_string_lossy().into_owned(),
            size,
            is_dir,
        });
    }
    entries.sort_by(|left, right| right.size.cmp(&left.size));
    Ok(NodeView {
        path: path.to_string_lossy().into_owned(),
        size: view_size,
        entries,
    })
}

/// Tauri boundary for identity-aware node navigation.
#[cfg(not(coverage))]
#[tauri::command(rename = "get_node")]
pub(crate) fn get_node_secure(
    path: String,
    state: tauri::State<'_, AppState>,
) -> Result<NodeView, String> {
    let guard = state
        .result
        .lock()
        .map_err(|_| "scan result lock unavailable".to_string())?;
    let result = guard.as_ref().ok_or_else(|| "no scan result".to_string())?;
    node_view(result, &PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::scan_dir_with_interval;
    use std::sync::atomic::AtomicBool;

    fn scan(root: &Path) -> ScanResult {
        scan_dir_with_interval(root, &AtomicBool::new(false), 1, |_| {})
    }

    #[test]
    fn legitimate_descendant_lists_entries_sorted_by_size() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("sub")).unwrap();
        std::fs::write(root.path().join("sub").join("large.bin"), vec![0u8; 32]).unwrap();
        std::fs::write(root.path().join("small.bin"), vec![0u8; 4]).unwrap();
        let result = scan(root.path());

        let view = node_view(&result, root.path()).unwrap();
        assert_eq!(view.entries.len(), 2);
        assert_eq!(view.entries[0].name, "sub");
        assert!(view.entries[0].is_dir);
    }

    #[test]
    fn directories_pruned_from_scan_are_hidden_and_not_navigable() {
        let root = tempfile::tempdir().unwrap();
        let visible = root.path().join("visible");
        let pruned = root.path().join("provider-managed");
        std::fs::create_dir(&visible).unwrap();
        std::fs::create_dir(&pruned).unwrap();
        std::fs::write(visible.join("kept.bin"), b"kept").unwrap();
        std::fs::write(pruned.join("cloud.bin"), b"cloud").unwrap();
        let mut result = scan(root.path());
        result.dir_sizes.remove(&pruned);

        let root_view = node_view(&result, root.path()).unwrap();
        assert!(root_view
            .entries
            .iter()
            .any(|entry| entry.name == "visible"));
        assert!(root_view
            .entries
            .iter()
            .all(|entry| entry.name != "provider-managed"));
        assert!(matches!(node_view(&result, &pruned), Err(error) if error == NOT_IN_SCAN));
    }

    #[test]
    fn legitimate_empty_directory_remains_navigable() {
        let root = tempfile::tempdir().unwrap();
        let empty = root.path().join("empty");
        std::fs::create_dir(&empty).unwrap();
        let result = scan(root.path());

        let root_view = node_view(&result, root.path()).unwrap();
        assert!(root_view.entries.iter().any(|entry| entry.name == "empty"));
        let empty_view = node_view(&result, &empty).unwrap();
        assert!(empty_view.entries.is_empty());
        assert_eq!(empty_view.size, 0);
    }

    #[test]
    fn cancelled_scan_hides_regular_files_without_admission_evidence() {
        let root = tempfile::tempdir().unwrap();
        let observed = root.path().join("observed.bin");
        let unvisited = root.path().join("unvisited.bin");
        std::fs::write(&observed, b"observed").unwrap();
        std::fs::write(&unvisited, b"unvisited").unwrap();
        let mut result = scan(root.path());
        result.cancelled = true;
        result.admitted_files.remove(&unvisited);

        let view = node_view(&result, root.path()).unwrap();
        assert!(view
            .entries
            .iter()
            .any(|entry| entry.name == "observed.bin"));
        assert!(view
            .entries
            .iter()
            .all(|entry| entry.name != "unvisited.bin"));
    }

    #[test]
    fn immediately_cancelled_scan_keeps_an_empty_root_view() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("unscanned.bin"), b"not observed").unwrap();
        let result = scan_dir_with_interval(root.path(), &AtomicBool::new(true), 1, |_| {});
        assert!(result.cancelled);
        assert!(result.dir_sizes.is_empty());

        let view = node_view(&result, root.path()).unwrap();
        assert_eq!(view.size, 0);
        assert!(view.entries.is_empty());
    }

    #[test]
    fn lexical_parent_component_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let result = scan(root.path());
        assert_eq!(
            canonical_navigation_path(&result, &root.path().join("..")),
            Err(OUTSIDE_ROOT.to_string())
        );
    }

    #[test]
    fn lexical_sibling_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let sibling = tempfile::tempdir().unwrap();
        let result = scan(root.path());
        assert_eq!(
            canonical_navigation_path(&result, sibling.path()),
            Err(OUTSIDE_ROOT.to_string())
        );
    }

    #[test]
    fn missing_navigation_target_fails_closed() {
        let root = tempfile::tempdir().unwrap();
        let result = scan(root.path());
        assert_eq!(
            canonical_navigation_path(&result, &root.path().join("missing")),
            Err(OUTSIDE_ROOT.to_string())
        );
    }

    #[cfg(unix)]
    #[test]
    fn final_symlink_escape_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        std::fs::write(external.path().join("secret.bin"), b"outside").unwrap();
        let escape = root.path().join("escape");
        std::os::unix::fs::symlink(external.path(), &escape).unwrap();
        let result = scan(root.path());

        assert_eq!(
            node_view(&result, &escape).err().as_deref(),
            Some(OUTSIDE_ROOT)
        );
    }

    #[cfg(unix)]
    #[test]
    fn child_symlink_entries_remain_hidden() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("real.bin"), b"inside").unwrap();
        std::os::unix::fs::symlink(root.path().join("real.bin"), root.path().join("linked.bin"))
            .unwrap();
        let result = scan(root.path());

        let view = node_view(&result, root.path()).unwrap();
        assert!(view.entries.iter().all(|entry| entry.name != "linked.bin"));
    }
}
