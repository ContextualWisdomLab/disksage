//! Root-object binding for read-only filesystem evidence.
//!
//! A caller-supplied path is not a stable authority boundary: the directory entry can be renamed
//! or replaced between a path check and later traversal. `BoundReadRoot` opens the directory with
//! no-follow/reparse-point semantics, verifies that a second open still names the same filesystem
//! object, and keeps that handle alive for the whole audit. Unix traversal uses the open-handle
//! namespace (`/proc/self/fd` or `/dev/fd`); Windows keeps a handle that deliberately excludes
//! delete sharing so rename/delete replacement is blocked while evidence is collected.

use same_file::Handle;
use std::path::{Path, PathBuf};

#[cfg(windows)]
fn metadata_is_real_directory(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.is_dir()
        && !metadata.file_type().is_symlink()
        && metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
}

#[cfg(not(windows))]
fn metadata_is_real_directory(metadata: &std::fs::Metadata) -> bool {
    metadata.is_dir() && !metadata.file_type().is_symlink()
}

fn path_is_real_directory(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|metadata| metadata_is_real_directory(&metadata))
        .unwrap_or(false)
}

#[cfg(windows)]
fn open_directory_handle(path: &Path) -> Option<Handle> {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    let file = std::fs::OpenOptions::new()
        .read(true)
        // Excluding FILE_SHARE_DELETE keeps the selected directory name from being renamed or
        // removed while this guard is alive.
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .ok()?;
    Handle::from_file(file).ok()
}

#[cfg(target_os = "linux")]
const NOFOLLOW_DIRECTORY_FLAGS: i32 = 0o600000; // O_DIRECTORY | O_NOFOLLOW

#[cfg(target_os = "macos")]
const NOFOLLOW_DIRECTORY_FLAGS: i32 = 0x0010_0100; // O_DIRECTORY | O_NOFOLLOW

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn open_directory_handle(path: &Path) -> Option<Handle> {
    use std::os::unix::fs::OpenOptionsExt;

    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(NOFOLLOW_DIRECTORY_FLAGS)
        .open(path)
        .ok()?;
    Handle::from_file(file).ok()
}

#[cfg(target_os = "linux")]
fn handle_namespace_path(handle: &Handle, _display_path: &Path) -> Option<PathBuf> {
    use std::os::fd::AsRawFd;
    Some(PathBuf::from(format!(
        "/proc/self/fd/{}",
        handle.as_file().as_raw_fd()
    )))
}

#[cfg(target_os = "macos")]
fn handle_namespace_path(handle: &Handle, _display_path: &Path) -> Option<PathBuf> {
    use std::os::fd::AsRawFd;
    Some(PathBuf::from(format!(
        "/dev/fd/{}",
        handle.as_file().as_raw_fd()
    )))
}

#[cfg(windows)]
fn handle_namespace_path(_handle: &Handle, display_path: &Path) -> Option<PathBuf> {
    Some(display_path.to_path_buf())
}

/// An opened directory identity used as the authority root for read-only traversal.
///
/// The guard never grants mutation authority. Callers should retain the canonical path only for
/// durable report lineage and perform filesystem I/O through [`Self::stable_path`].
pub(crate) struct BoundReadRoot {
    handle: Handle,
    display_path: PathBuf,
}

impl BoundReadRoot {
    /// Atomically bind a real, non-symlink/reparse directory and reject path replacement races.
    pub(crate) fn open(path: &Path) -> Option<Self> {
        if !path_is_real_directory(path) {
            return None;
        }

        let handle = open_directory_handle(path)?;
        if !path_is_real_directory(path) {
            return None;
        }
        let current = open_directory_handle(path)?;
        if handle != current {
            return None;
        }

        Some(Self {
            handle,
            display_path: path.to_path_buf(),
        })
    }

    /// Return a durable canonical display path only while the caller path still names this object.
    pub(crate) fn canonical_path(&self) -> Option<PathBuf> {
        let canonical = std::fs::canonicalize(&self.display_path).ok()?;
        let current = open_directory_handle(&canonical)?;
        (self.handle == current).then_some(canonical)
    }

    /// Return a path whose I/O is bound to the opened directory identity.
    pub(crate) fn stable_path(&self) -> Option<PathBuf> {
        let stable = handle_namespace_path(&self.handle, &self.display_path)?;
        let expected = Handle::from_file(self.handle.as_file().try_clone().ok()?).ok()?;
        #[cfg(windows)]
        let observed = open_directory_handle(&stable)?;
        #[cfg(not(windows))]
        let observed = Handle::from_path(&stable).ok()?;
        (expected == observed).then_some(stable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binds_real_directory_and_preserves_canonical_identity() {
        let root = tempfile::tempdir().unwrap();
        let guard = BoundReadRoot::open(root.path()).expect("real directory must bind");
        assert_eq!(guard.canonical_path(), std::fs::canonicalize(root.path()).ok());
        let stable = guard.stable_path().expect("bound root must expose stable traversal path");
        std::fs::write(stable.join("marker.bin"), b"bound").unwrap();
        assert_eq!(std::fs::read(stable.join("marker.bin")).unwrap(), b"bound");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_root() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let real = root.path().join("real");
        let linked = root.path().join("linked");
        std::fs::create_dir(&real).unwrap();
        symlink(&real, &linked).unwrap();
        assert!(BoundReadRoot::open(&linked).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn canonical_path_rejects_symlink_replacement_to_same_bound_directory() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let selected = root.path().join("selected");
        let moved = root.path().join("moved");
        std::fs::create_dir(&selected).unwrap();

        let guard = BoundReadRoot::open(&selected).expect("selected directory must bind");
        std::fs::rename(&selected, &moved).unwrap();
        symlink(&moved, &selected).unwrap();

        assert!(
            guard.canonical_path().is_none(),
            "caller pathname becoming a symlink must fail closed even when it resolves to the original directory"
        );
    }

    #[cfg(unix)]
    #[test]
    fn stable_namespace_keeps_original_object_and_detects_path_replacement() {
        let root = tempfile::tempdir().unwrap();
        let selected = root.path().join("selected");
        let moved = root.path().join("moved");
        std::fs::create_dir(&selected).unwrap();
        std::fs::write(selected.join("marker.txt"), b"original").unwrap();

        let guard = BoundReadRoot::open(&selected).expect("selected directory must bind");
        let stable = guard.stable_path().expect("stable namespace must be available");
        assert!(guard.canonical_path().is_some());

        std::fs::rename(&selected, &moved).unwrap();
        std::fs::create_dir(&selected).unwrap();
        std::fs::write(selected.join("marker.txt"), b"replacement").unwrap();

        assert_eq!(std::fs::read(stable.join("marker.txt")).unwrap(), b"original");
        assert!(guard.canonical_path().is_none());
    }

    #[cfg(windows)]
    #[test]
    fn windows_guard_blocks_root_rename_while_bound() {
        let root = tempfile::tempdir().unwrap();
        let selected = root.path().join("selected");
        let moved = root.path().join("moved");
        std::fs::create_dir(&selected).unwrap();
        let guard = BoundReadRoot::open(&selected).expect("selected directory must bind");

        assert!(std::fs::rename(&selected, &moved).is_err());
        assert_eq!(guard.canonical_path(), std::fs::canonicalize(&selected).ok());
    }
}
