// The historical command implementation remains isolated in a private module so the public
// command surface can enforce additional filesystem authority without duplicating the file.
mod legacy;
pub use legacy::*;

use crate::scanner::ScanResult;
use std::path::{Path, PathBuf};

/// Return one scanned directory level without allowing a lexical or symlink-mediated escape from
/// the scan root. The directory is opened through the already-validated canonical target so a
/// mutable symlink alias cannot redirect the actual `read_dir` after containment validation.
pub fn node_view(res: &ScanResult, path: &Path) -> Result<NodeView, String> {
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
        || !path.starts_with(&res.root)
    {
        return Err("path outside scanned root".into());
    }

    let canonical_root = std::fs::canonicalize(&res.root).map_err(|error| error.to_string())?;
    let canonical_path = std::fs::canonicalize(path).map_err(|error| error.to_string())?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err("path outside scanned root".into());
    }

    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&canonical_path).map_err(|error| error.to_string())? {
        let Ok(entry) = entry else { continue };
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        let display_path = path.join(entry.file_name());
        let (size, is_dir) = if file_type.is_dir() {
            (
                res.dir_sizes.get(&display_path).copied().unwrap_or(0),
                true,
            )
        } else {
            (entry.metadata().map(|metadata| metadata.len()).unwrap_or(0), false)
        };
        entries.push(EntryView {
            name: entry.file_name().to_string_lossy().into_owned(),
            path: display_path.to_string_lossy().into_owned(),
            size,
            is_dir,
        });
    }
    entries.sort_by(|left, right| right.size.cmp(&left.size));
    Ok(NodeView {
        path: path.to_string_lossy().into_owned(),
        size: res.dir_sizes.get(path).copied().unwrap_or(0),
        entries,
    })
}

/// Tauri boundary for node inspection. It delegates to the containment-enforcing public helper
/// above rather than the private legacy implementation.
#[cfg(not(coverage))]
#[tauri::command]
pub fn get_node(path: String, state: tauri::State<AppState>) -> Result<NodeView, String> {
    let guard = state.result.lock().unwrap();
    let result = guard.as_ref().ok_or("no scan result")?;
    node_view(result, &PathBuf::from(path))
}
