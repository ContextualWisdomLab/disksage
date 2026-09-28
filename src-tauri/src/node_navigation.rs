//! Identity-aware, read-only scan-tree navigation.
//!
//! The UI passes a path selected from a prior scan. Lexical ancestry alone is not sufficient
//! authority because a final directory symlink or reparse path can still resolve outside the
//! scanned root. This module canonicalizes the scanned root and requested directory immediately
//! before enumeration, then requires the requested object to remain within that canonical root.
//! It never mutates the filesystem.

use crate::commands::{AppState, EntryView, NodeView};
use crate::scanner::{is_within_managed_provider_scope, provider_home_root, ScanResult};
use std::path::{Component, Path, PathBuf};

const OUTSIDE_ROOT: &str = "path outside scanned root";

fn canonical_navigation_path_with_home(
    res: &ScanResult,
    path: &Path,
    provider_home: Option<&Path>,
) -> Result<PathBuf, String> {
    if path.components().any(|component| matches!(component, Component::ParentDir)) {
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
    if provider_home.is_some_and(|home| {
        let canonical_home = std::fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf());
        is_within_managed_provider_scope(&canonical_path, &canonical_home)
    }) {
        return Err("클라우드 전용 폴더는 열지 않습니다. 로컬 폴더를 선택하세요.".into());
    }
    Ok(canonical_path)
}

fn canonical_navigation_path(res: &ScanResult, path: &Path) -> Result<PathBuf, String> {
    let provider_home = provider_home_root();
    canonical_navigation_path_with_home(res, path, provider_home.as_deref())
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

/// Return one level of scan navigation only when the requested directory resolves inside the
/// canonical scanned root.
pub(crate) fn node_view(res: &ScanResult, path: &Path) -> Result<NodeView, String> {
    let canonical_path = canonical_navigation_path(res, path)?;
    let canonical_root =
        std::fs::canonicalize(&res.root).map_err(|_| OUTSIDE_ROOT.to_string())?;
    let relative = canonical_path
        .strip_prefix(&canonical_root)
        .map_err(|_| OUTSIDE_ROOT.to_string())?;
    // macOS canonicalizes `/var` to `/private/var`; keep scanner keys and UI paths in
    // the original namespace while reading entries through the verified canonical path.
    let display_path = res.root.join(relative);
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&canonical_path).map_err(|_| "node directory unavailable".to_string())? {
        let Ok(entry) = entry else { continue };
        let Ok(file_type) = entry.file_type() else { continue };
        let entry_path = entry.path();
        if entry_is_link_or_reparse(&entry_path, &file_type) {
            continue;
        }
        let (size, is_dir) = if file_type.is_dir() {
            (
                res.dir_sizes
                    .get(&display_path.join(entry.file_name()))
                    .or_else(|| res.dir_sizes.get(&entry_path))
                    .copied()
                    .unwrap_or_default(),
                true,
            )
        } else {
            (
                std::fs::symlink_metadata(&entry_path)
                    .map(|metadata| metadata.len())
                    .unwrap_or_default(),
                false,
            )
        };
        entries.push(EntryView {
            name: entry.file_name().to_string_lossy().into_owned(),
            path: display_path
                .join(entry.file_name())
                .to_string_lossy()
                .into_owned(),
            size,
            is_dir,
        });
    }
    entries.sort_by(|left, right| right.size.cmp(&left.size));
    Ok(NodeView {
        path: path.to_string_lossy().into_owned(),
        size: res
            .dir_sizes
            .get(&display_path)
            .or_else(|| res.dir_sizes.get(&canonical_path))
            .or_else(|| res.dir_sizes.get(path))
            .copied()
            .unwrap_or_default(),
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

    fn managed_provider_root(home: &Path) -> PathBuf {
        #[cfg(target_os = "macos")]
        return home.join("Library").join("CloudStorage");
        #[cfg(not(target_os = "macos"))]
        return home.join("OneDrive");
    }

    #[test]
    fn managed_provider_root_navigation_fails_closed() {
        let home = tempfile::tempdir().unwrap();
        let root = managed_provider_root(home.path());
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("placeholder.bin"), b"placeholder").unwrap();
        let result = scan(&root);

        assert!(canonical_navigation_path_with_home(&result, &root, Some(home.path())).is_err());
    }

    #[test]
    fn managed_provider_descendant_navigation_fails_closed() {
        let home = tempfile::tempdir().unwrap();
        let root = managed_provider_root(home.path());
        let child = root.join("Account");
        std::fs::create_dir_all(&child).unwrap();
        let result = scan(&root);

        assert!(canonical_navigation_path_with_home(&result, &child, Some(home.path())).is_err());
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

        assert_eq!(node_view(&result, &escape).err().as_deref(), Some(OUTSIDE_ROOT));
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
