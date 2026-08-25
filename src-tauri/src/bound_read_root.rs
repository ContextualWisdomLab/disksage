//! Root-object binding for read-only filesystem evidence.
//!
//! A caller-supplied path is not a stable authority boundary: the directory entry can be renamed
//! or replaced between a path check and later traversal. `BoundReadRoot` opens the directory with
//! no-follow/reparse-point semantics, verifies that a second open still names the same filesystem
//! object, and keeps that handle alive for the whole audit. Unix callers should traverse through
//! the descriptor-relative helpers on this guard: they walk components with `openat`/`fstatat` and
//! enumerate directories with `fdopendir`, so pathname replacement cannot redirect child I/O.
//! Windows keeps a handle that deliberately excludes delete sharing, which blocks root rename or
//! deletion while the guard is alive.

use same_file::Handle;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BoundEntryKind {
    Directory,
    File,
    Symlink,
    Other,
}

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

#[cfg(any(target_os = "linux", target_os = "macos"))]
const NOFOLLOW_DIRECTORY_FLAGS: i32 = libc::O_DIRECTORY | libc::O_NOFOLLOW;

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
fn handle_traversal_path(handle: &Handle, _display_path: &Path) -> Option<PathBuf> {
    use std::os::fd::AsRawFd;
    // `/proc/self` is process-relative: an external evidence probe launched by DiskSage would
    // otherwise resolve its own descriptor table instead of the descriptor held by this process.
    // Pin the namespace to DiskSage's PID so read-only child probes can resolve the same bound root.
    Some(PathBuf::from(format!(
        "/proc/{}/fd/{}",
        std::process::id(),
        handle.as_file().as_raw_fd()
    )))
}

#[cfg(target_os = "macos")]
fn handle_traversal_path(handle: &Handle, _display_path: &Path) -> Option<PathBuf> {
    use std::ffi::{CStr, OsString};
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStringExt;

    let mut buffer = vec![0 as libc::c_char; libc::PATH_MAX as usize];
    let result = unsafe {
        // SAFETY: `buffer` is writable for PATH_MAX bytes and the descriptor is held alive by
        // `Handle`. F_GETPATH writes a NUL-terminated path on success and does not retain it.
        libc::fcntl(
            handle.as_file().as_raw_fd(),
            libc::F_GETPATH,
            buffer.as_mut_ptr(),
        )
    };
    if result == -1 {
        return None;
    }
    let path = unsafe {
        // SAFETY: successful F_GETPATH guarantees a NUL-terminated string within the supplied
        // PATH_MAX-sized buffer.
        CStr::from_ptr(buffer.as_ptr())
    };
    Some(PathBuf::from(OsString::from_vec(path.to_bytes().to_vec())))
}

#[cfg(windows)]
fn handle_traversal_path(_handle: &Handle, display_path: &Path) -> Option<PathBuf> {
    Some(display_path.to_path_buf())
}

#[cfg(unix)]
fn invalid_relative_path() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        "bound root traversal requires normal relative path components",
    )
}

#[cfg(unix)]
fn relative_components(relative: &Path) -> std::io::Result<Vec<std::ffi::CString>> {
    use std::os::unix::ffi::OsStrExt;
    use std::path::Component;

    if relative.is_absolute() {
        return Err(invalid_relative_path());
    }
    relative
        .components()
        .map(|component| match component {
            Component::Normal(value) => {
                std::ffi::CString::new(value.as_bytes()).map_err(|_| invalid_relative_path())
            }
            _ => Err(invalid_relative_path()),
        })
        .collect()
}

#[cfg(unix)]
fn duplicate_cloexec(file: &std::fs::File) -> std::io::Result<std::fs::File> {
    use std::os::fd::{AsRawFd, FromRawFd};

    let fd = unsafe {
        // SAFETY: the source descriptor is live for the duration of this call; F_DUPFD_CLOEXEC
        // returns a new independently owned descriptor on success.
        libc::fcntl(file.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 0)
    };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(unsafe {
        // SAFETY: `fd` was freshly allocated above and ownership is transferred exactly once.
        std::fs::File::from_raw_fd(fd)
    })
}

#[cfg(unix)]
fn open_directory_components(
    root: &std::fs::File,
    components: &[std::ffi::CString],
) -> std::io::Result<std::fs::File> {
    use std::os::fd::{AsRawFd, FromRawFd};

    let mut current = duplicate_cloexec(root)?;
    for component in components {
        let fd = unsafe {
            // SAFETY: `current` is a live directory descriptor and `component` is NUL-terminated.
            libc::openat(
                current.as_raw_fd(),
                component.as_ptr(),
                libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        current = unsafe {
            // SAFETY: `fd` is newly returned by openat and becomes owned by this File.
            std::fs::File::from_raw_fd(fd)
        };
    }
    Ok(current)
}

#[cfg(target_os = "linux")]
unsafe fn errno_location() -> *mut libc::c_int {
    libc::__errno_location()
}

#[cfg(target_os = "macos")]
unsafe fn errno_location() -> *mut libc::c_int {
    libc::__error()
}

#[cfg(unix)]
struct DirectoryStream(*mut libc::DIR);

#[cfg(unix)]
impl Drop for DirectoryStream {
    fn drop(&mut self) {
        unsafe {
            // SAFETY: the pointer is created only by successful fdopendir and is closed once here.
            libc::closedir(self.0);
        }
    }
}

/// An opened directory identity used as the authority root for read-only traversal.
///
/// The guard never grants mutation authority. Canonical paths are display/lineage evidence only;
/// security-sensitive Unix child I/O should use [`Self::read_dir_names`], [`Self::entry_kind`],
/// and [`Self::open_file`] so every component remains relative to the opened root descriptor.
pub(crate) struct BoundReadRoot {
    handle: Handle,
    display_path: PathBuf,
}

impl BoundReadRoot {
    /// Atomically bind a real, non-symlink/reparse directory and reject path replacement races.
    pub(crate) fn open(path: &Path) -> Option<Self> {
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
        if !path_is_real_directory(&self.display_path) {
            return None;
        }
        let canonical = std::fs::canonicalize(&self.display_path).ok()?;
        let current = open_directory_handle(&self.display_path)?;
        if self.handle != current {
            return None;
        }
        let canonical_handle = open_directory_handle(&canonical)?;
        (self.handle == canonical_handle).then_some(canonical)
    }

    /// Return directory entry names beneath `relative` without resolving the caller root again.
    pub(crate) fn read_dir_names(
        &self,
        relative: &Path,
    ) -> std::io::Result<Vec<std::ffi::OsString>> {
        #[cfg(unix)]
        {
            use std::ffi::{CStr, OsString};
            use std::os::fd::AsRawFd;
            use std::os::unix::ffi::OsStringExt;

            let components = relative_components(relative)?;
            let directory = open_directory_components(self.handle.as_file(), &components)?;
            let current_directory = std::ffi::CString::new(".").expect("literal has no NUL");
            let stream_fd = unsafe {
                // SAFETY: `directory` is a live directory descriptor and the literal component
                // cannot escape it. Opening `.` creates a fresh open-file description, so each
                // enumeration starts at offset zero instead of sharing a prior readdir offset.
                libc::openat(
                    directory.as_raw_fd(),
                    current_directory.as_ptr(),
                    libc::O_RDONLY | libc::O_CLOEXEC | libc::O_DIRECTORY | libc::O_NOFOLLOW,
                )
            };
            if stream_fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            let stream = unsafe {
                // SAFETY: fdopendir consumes the duplicated descriptor on success.
                libc::fdopendir(stream_fd)
            };
            if stream.is_null() {
                let error = std::io::Error::last_os_error();
                unsafe {
                    // SAFETY: fdopendir failed, so ownership of stream_fd was not transferred.
                    libc::close(stream_fd);
                }
                return Err(error);
            }
            let stream = DirectoryStream(stream);
            let mut names = Vec::new();
            loop {
                unsafe {
                    // SAFETY: errno is thread-local and writing zero before readdir is the POSIX
                    // pattern for distinguishing EOF from an enumeration error.
                    *errno_location() = 0;
                }
                let entry = unsafe {
                    // SAFETY: the directory stream remains live for the duration of the call.
                    libc::readdir(stream.0)
                };
                if entry.is_null() {
                    let errno = unsafe {
                        // SAFETY: errno_location returns the current thread's live errno pointer.
                        *errno_location()
                    };
                    if errno == 0 {
                        break;
                    }
                    return Err(std::io::Error::from_raw_os_error(errno));
                }
                let name = unsafe {
                    // SAFETY: POSIX dirent names are NUL-terminated within d_name.
                    CStr::from_ptr((*entry).d_name.as_ptr())
                }
                .to_bytes();
                if name == b"." || name == b".." {
                    continue;
                }
                names.push(OsString::from_vec(name.to_vec()));
            }
            return Ok(names);
        }

        #[cfg(windows)]
        {
            let root = self.stable_path().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "bound root unavailable")
            })?;
            return std::fs::read_dir(root.join(relative))?
                .map(|entry| entry.map(|entry| entry.file_name()))
                .collect();
        }
    }

    /// Inspect one relative child without following a symlink/reparse point.
    pub(crate) fn entry_kind(&self, relative: &Path) -> std::io::Result<BoundEntryKind> {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;

            let mut components = relative_components(relative)?;
            if components.is_empty() {
                return Ok(BoundEntryKind::Directory);
            }
            let name = components.pop().expect("non-empty relative components");
            let parent = open_directory_components(self.handle.as_file(), &components)?;
            let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
            let result = unsafe {
                // SAFETY: parent is a live directory descriptor, name is NUL-terminated, and stat
                // points to writable storage for exactly one libc::stat value.
                libc::fstatat(
                    parent.as_raw_fd(),
                    name.as_ptr(),
                    stat.as_mut_ptr(),
                    libc::AT_SYMLINK_NOFOLLOW,
                )
            };
            if result != 0 {
                return Err(std::io::Error::last_os_error());
            }
            let mode = unsafe {
                // SAFETY: successful fstatat initialized the entire stat value.
                stat.assume_init().st_mode
            };
            let file_type = mode & libc::S_IFMT;
            return Ok(if file_type == libc::S_IFDIR {
                BoundEntryKind::Directory
            } else if file_type == libc::S_IFREG {
                BoundEntryKind::File
            } else if file_type == libc::S_IFLNK {
                BoundEntryKind::Symlink
            } else {
                BoundEntryKind::Other
            });
        }

        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

            let root = self.stable_path().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "bound root unavailable")
            })?;
            let metadata = std::fs::symlink_metadata(root.join(relative))?;
            return Ok(if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                BoundEntryKind::Symlink
            } else if metadata.is_dir() {
                BoundEntryKind::Directory
            } else if metadata.is_file() {
                BoundEntryKind::File
            } else {
                BoundEntryKind::Other
            });
        }
    }

    /// Open one relative child for read-only evidence without following any parent or final symlink.
    pub(crate) fn open_file(&self, relative: &Path) -> std::io::Result<std::fs::File> {
        #[cfg(unix)]
        {
            use std::os::fd::{AsRawFd, FromRawFd};

            let mut components = relative_components(relative)?;
            let name = components.pop().ok_or_else(invalid_relative_path)?;
            let parent = open_directory_components(self.handle.as_file(), &components)?;
            let fd = unsafe {
                // SAFETY: parent is a live directory descriptor and name is NUL-terminated. O_NOFOLLOW
                // rejects a final symlink; O_NONBLOCK prevents audit code from hanging on a FIFO.
                libc::openat(
                    parent.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
                )
            };
            if fd < 0 {
                return Err(std::io::Error::last_os_error());
            }
            return Ok(unsafe {
                // SAFETY: fd was freshly returned by openat and is transferred exactly once.
                std::fs::File::from_raw_fd(fd)
            });
        }

        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;

            const FILE_SHARE_READ: u32 = 0x0000_0001;
            const FILE_SHARE_WRITE: u32 = 0x0000_0002;
            const FILE_SHARE_DELETE: u32 = 0x0000_0004;
            const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

            let root = self.stable_path().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "bound root unavailable")
            })?;
            return std::fs::OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
                // Do not follow a leaf reparse point if it is swapped after entry_kind.
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(root.join(relative));
        }
    }

    /// Legacy path exposure retained for Linux/Windows callers during migration.
    ///
    /// On macOS this is useful only as display/compatibility evidence; child traversal must use the
    /// descriptor-relative helpers above because an F_GETPATH result is not rename-stable.
    pub(crate) fn stable_path(&self) -> Option<PathBuf> {
        let stable = handle_traversal_path(&self.handle, &self.display_path)?;
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
    use std::io::Read;

    #[test]
    fn binds_real_directory_and_preserves_canonical_identity() {
        let root = tempfile::tempdir().unwrap();
        let guard = BoundReadRoot::open(root.path()).expect("real directory must bind");
        assert_eq!(
            guard.canonical_path(),
            std::fs::canonicalize(root.path()).ok()
        );
        assert!(guard.read_dir_names(Path::new("")).unwrap().is_empty());
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
    fn descriptor_relative_namespace_keeps_original_object_after_path_replacement() {
        let root = tempfile::tempdir().unwrap();
        let selected = root.path().join("selected");
        let moved = root.path().join("moved");
        std::fs::create_dir(&selected).unwrap();
        std::fs::create_dir(selected.join("nested")).unwrap();
        std::fs::write(selected.join("nested").join("marker.txt"), b"original").unwrap();

        let guard = BoundReadRoot::open(&selected).expect("selected directory must bind");
        assert!(guard.canonical_path().is_some());

        std::fs::rename(&selected, &moved).unwrap();
        std::fs::create_dir(&selected).unwrap();
        std::fs::create_dir(selected.join("nested")).unwrap();
        std::fs::write(selected.join("nested").join("marker.txt"), b"replacement").unwrap();

        let root_names = guard.read_dir_names(Path::new("")).unwrap();
        assert_eq!(root_names, vec![std::ffi::OsString::from("nested")]);
        assert_eq!(guard.read_dir_names(Path::new("")).unwrap(), root_names);
        assert_eq!(
            guard.entry_kind(Path::new("nested")).unwrap(),
            BoundEntryKind::Directory
        );
        assert_eq!(
            guard.entry_kind(Path::new("nested/marker.txt")).unwrap(),
            BoundEntryKind::File
        );
        let mut file = guard.open_file(Path::new("nested/marker.txt")).unwrap();
        let mut contents = Vec::new();
        file.read_to_end(&mut contents).unwrap();
        assert_eq!(contents, b"original");
        assert!(guard.canonical_path().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn descriptor_relative_helpers_reject_parent_and_symlink_components() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("real")).unwrap();
        std::fs::write(root.path().join("real").join("file"), b"data").unwrap();
        symlink(root.path().join("real"), root.path().join("linked")).unwrap();
        let guard = BoundReadRoot::open(root.path()).unwrap();

        assert!(guard.read_dir_names(Path::new("../escape")).is_err());
        assert_eq!(
            guard.entry_kind(Path::new("linked")).unwrap(),
            BoundEntryKind::Symlink
        );
        assert!(guard.open_file(Path::new("linked/file")).is_err());
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
        assert_eq!(
            guard.canonical_path(),
            std::fs::canonicalize(&selected).ok()
        );
    }
}
