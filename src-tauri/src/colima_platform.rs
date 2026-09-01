//! Host-side Colima path resolution used by DiskSage platform adapters.

use std::path::{Path, PathBuf};

fn explicit_absolute_path(name: &str) -> Result<Option<PathBuf>, String> {
    let Some(value) = std::env::var_os(name).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err("colima-cache-home-relative-unsupported".into());
    }
    Ok(Some(path))
}

/// Resolves the cache directory using the same precedence as Colima: `COLIMA_CACHE_HOME`, then
/// `XDG_CACHE_HOME/colima`, then the operating system's user cache directory plus `colima`.
/// Explicit relative cache roots fail closed because destructive cache authority must identify one
/// unambiguous host path.
pub fn configured_cache_root(platform_cache_dir: &Path) -> Result<PathBuf, String> {
    if let Some(path) = explicit_absolute_path("COLIMA_CACHE_HOME")? {
        return Ok(path);
    }
    if let Some(path) = explicit_absolute_path("XDG_CACHE_HOME")? {
        return Ok(path.join("colima"));
    }
    if !platform_cache_dir.is_absolute() {
        return Err("colima-cache-home-relative-unsupported".into());
    }
    Ok(platform_cache_dir.join("colima"))
}
