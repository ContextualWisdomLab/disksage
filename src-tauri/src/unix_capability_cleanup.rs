//! Unix capability-rooted traversal and contents cleanup for destructive reclaim.
//!
//! The caller supplies an already-reviewed directory [`File`]. This module never
//! re-selects that root by pathname. Descendants are inspected relative to the
//! retained directory descriptor with `fstatat(AT_SYMLINK_NOFOLLOW)`, directories
//! are opened with `openat(O_DIRECTORY|O_NOFOLLOW|O_CLOEXEC)`, and entries are
//! removed with `unlinkat`. Symlinks are unlinked as entries and are never followed.
//!
//! The reviewed root itself is intentionally retained. POSIX does not provide an
//! fd-self directory unlink primitive, so callers must prefer an empty retained
//! root over re-introducing a pathname-selection race at root disposition.

use std::collections::HashSet;
use std::ffi::{CStr, CString};
use std::fs::File;
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::time::{Duration, Instant};

const MAX_ENTRIES: u64 = 2_000_000;
const MAX_DEPTH: usize = 128;
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CleanupStats {
    pub(crate) entries_removed: u64,
}

struct DirStream(*mut libc::DIR);

impl DirStream {
    fn from_fd(fd: RawFd) -> Result<Self, String> {
        let duplicate = unsafe { libc::dup(fd) };
        if duplicate < 0 {
            return Err(format!(
                "cargo-target-capability-dup-failed:{}",
                std::io::Error::last_os_error()
            ));
        }
        let stream = unsafe { libc::fdopendir(duplicate) };
        if stream.is_null() {
            let error = std::io::Error::last_os_error();
            unsafe {
                libc::close(duplicate);
            }
            return Err(format!("cargo-target-capability-fdopendir-failed:{error}"));
        }
        Ok(Self(stream))
    }

    fn next_name(&mut self) -> Result<Option<CString>, String> {
        unsafe {
            *errno_location() = 0;
            let entry = libc::readdir(self.0);
            if entry.is_null() {
                let errno = *errno_location();
                if errno == 0 {
                    return Ok(None);
                }
                return Err(format!(
                    "cargo-target-capability-readdir-failed:{}",
                    std::io::Error::from_raw_os_error(errno)
                ));
            }
            let name = CStr::from_ptr((*entry).d_name.as_ptr()).to_owned();
            Ok(Some(name))
        }
    }
}

impl Drop for DirStream {
    fn drop(&mut self) {
        unsafe {
            libc::closedir(self.0);
        }
    }
}

#[cfg(target_os = "linux")]
unsafe fn errno_location() -> *mut libc::c_int {
    unsafe { libc::__errno_location() }
}

#[cfg(target_os = "macos")]
unsafe fn errno_location() -> *mut libc::c_int {
    unsafe { libc::__error() }
}

#[derive(Debug)]
struct WalkState {
    entries: u64,
    deadline: Instant,
    root_device: libc::dev_t,
    seen_files: HashSet<(libc::dev_t, libc::ino_t)>,
    allocated_bytes: u64,
    removed: u64,
}

impl WalkState {
    fn new(root_device: libc::dev_t) -> Self {
        Self {
            entries: 0,
            deadline: Instant::now() + CLEANUP_TIMEOUT,
            root_device,
            seen_files: HashSet::new(),
            allocated_bytes: 0,
            removed: 0,
        }
    }

    fn admit_entry(&mut self, depth: usize) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Err("cargo-target-capability-depth-limit".into());
        }
        if Instant::now() >= self.deadline {
            return Err("cargo-target-capability-timeout".into());
        }
        self.entries = self
            .entries
            .checked_add(1)
            .ok_or_else(|| "cargo-target-capability-entry-overflow".to_string())?;
        if self.entries > MAX_ENTRIES {
            return Err("cargo-target-capability-entry-limit".into());
        }
        Ok(())
    }

    fn add_allocation(&mut self, stat: &libc::stat) -> Result<(), String> {
        if !self.seen_files.insert((stat.st_dev, stat.st_ino)) {
            return Ok(());
        }
        let blocks = u64::try_from(stat.st_blocks)
            .map_err(|_| "cargo-target-size-allocation-overflow".to_string())?;
        let allocated = blocks
            .checked_mul(512)
            .ok_or_else(|| "cargo-target-size-allocation-overflow".to_string())?;
        self.allocated_bytes = self
            .allocated_bytes
            .checked_add(allocated)
            .ok_or_else(|| "cargo-target-size-allocation-overflow".to_string())?;
        Ok(())
    }
}

fn stat_at(dir_fd: RawFd, name: &CStr) -> Result<libc::stat, String> {
    let mut stat = unsafe { std::mem::zeroed::<libc::stat>() };
    let rc = unsafe {
        libc::fstatat(
            dir_fd,
            name.as_ptr(),
            &mut stat,
            libc::AT_SYMLINK_NOFOLLOW,
        )
    };
    if rc != 0 {
        return Err(format!(
            "cargo-target-capability-fstatat-failed:{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(stat)
}

fn open_child_directory(
    parent_fd: RawFd,
    name: &CStr,
    expected: &libc::stat,
    root_device: libc::dev_t,
) -> Result<File, String> {
    let fd = unsafe {
        libc::openat(
            parent_fd,
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(format!(
            "cargo-target-capability-openat-failed:{}",
            std::io::Error::last_os_error()
        ));
    }
    let file = unsafe { File::from_raw_fd(fd) };
    let mut opened = unsafe { std::mem::zeroed::<libc::stat>() };
    if unsafe { libc::fstat(file.as_raw_fd(), &mut opened) } != 0 {
        return Err(format!(
            "cargo-target-capability-fstat-failed:{}",
            std::io::Error::last_os_error()
        ));
    }
    if opened.st_dev != expected.st_dev || opened.st_ino != expected.st_ino {
        return Err("cargo-target-capability-child-replaced".into());
    }
    if opened.st_dev != root_device {
        return Err("cargo-target-capability-cross-device".into());
    }
    Ok(file)
}

fn unlink_at(dir_fd: RawFd, name: &CStr, flags: libc::c_int) -> Result<(), String> {
    if unsafe { libc::unlinkat(dir_fd, name.as_ptr(), flags) } != 0 {
        return Err(format!(
            "cargo-target-capability-unlinkat-failed:{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

fn walk_directory(
    directory: &File,
    depth: usize,
    state: &mut WalkState,
    remove: bool,
) -> Result<(), String> {
    let dir_fd = directory.as_raw_fd();
    let mut stream = DirStream::from_fd(dir_fd)?;
    while let Some(name) = stream.next_name()? {
        if name.as_bytes() == b"." || name.as_bytes() == b".." {
            continue;
        }
        state.admit_entry(depth)?;
        let stat = stat_at(dir_fd, &name)?;
        if stat.st_dev != state.root_device {
            return Err("cargo-target-capability-cross-device".into());
        }
        let kind = stat.st_mode & libc::S_IFMT;
        if kind == libc::S_IFDIR {
            let child = open_child_directory(dir_fd, &name, &stat, state.root_device)?;
            walk_directory(&child, depth + 1, state, remove)?;
            if remove {
                unlink_at(dir_fd, &name, libc::AT_REMOVEDIR)?;
                state.removed = state
                    .removed
                    .checked_add(1)
                    .ok_or_else(|| "cargo-target-capability-entry-overflow".to_string())?;
            }
        } else if kind == libc::S_IFREG {
            state.add_allocation(&stat)?;
            if remove {
                unlink_at(dir_fd, &name, 0)?;
                state.removed = state
                    .removed
                    .checked_add(1)
                    .ok_or_else(|| "cargo-target-capability-entry-overflow".to_string())?;
            }
        } else if kind == libc::S_IFLNK {
            if remove {
                unlink_at(dir_fd, &name, 0)?;
                state.removed = state
                    .removed
                    .checked_add(1)
                    .ok_or_else(|| "cargo-target-capability-entry-overflow".to_string())?;
            }
        } else {
            return Err("cargo-target-capability-unsafe-entry-type".into());
        }
    }
    Ok(())
}

fn root_device(root: &File) -> Result<libc::dev_t, String> {
    let mut stat = unsafe { std::mem::zeroed::<libc::stat>() };
    if unsafe { libc::fstat(root.as_raw_fd(), &mut stat) } != 0 {
        return Err(format!(
            "cargo-target-capability-root-fstat-failed:{}",
            std::io::Error::last_os_error()
        ));
    }
    if stat.st_mode & libc::S_IFMT != libc::S_IFDIR {
        return Err("cargo-target-capability-root-not-directory".into());
    }
    Ok(stat.st_dev)
}

/// Measures allocated regular-file bytes beneath an already-reviewed directory capability.
///
/// Hard-linked files are counted once per `(device, inode)` within this view. Symlinks are
/// not followed or credited. The returned number is allocation evidence for this tree view,
/// not proof that the underlying blocks would be physically released by unlinking entries.
pub(crate) fn measure_allocated_bytes(root: &File) -> Result<u64, String> {
    let mut state = WalkState::new(root_device(root)?);
    walk_directory(root, 1, &mut state, false)?;
    Ok(state.allocated_bytes)
}

/// Removes descendants beneath an already-reviewed directory capability while retaining root.
///
/// If an error occurs after one or more irreversible unlinks, the error is prefixed with
/// `cargo-target-partial-clean-failed` so callers cannot claim rollback of removed descendants.
pub(crate) fn remove_contents(root: &File) -> Result<CleanupStats, String> {
    let mut state = WalkState::new(root_device(root)?);
    match walk_directory(root, 1, &mut state, true) {
        Ok(()) => Ok(CleanupStats {
            entries_removed: state.removed,
        }),
        Err(error) if state.removed > 0 => Err(format!(
            "cargo-target-partial-clean-failed:removed={}:{}",
            state.removed, error
        )),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn cleanup_is_root_capability_relative_and_never_follows_symlink() {
        let root = tempfile::tempdir().expect("temp root");
        let target = root.path().join("target");
        let outside = root.path().join("outside");
        std::fs::create_dir_all(target.join("nested")).expect("nested target");
        std::fs::create_dir_all(&outside).expect("outside");
        std::fs::write(target.join("nested/artifact"), vec![7u8; 8192]).expect("artifact");
        std::fs::write(outside.join("SENTINEL"), b"must-survive").expect("sentinel");
        symlink(&outside, target.join("outside-link")).expect("symlink");

        let reviewed = File::open(&target).expect("open target");
        let before = measure_allocated_bytes(&reviewed).expect("measure before");
        assert!(before > 0);
        let stats = remove_contents(&reviewed).expect("capability cleanup");
        assert!(stats.entries_removed >= 3);
        assert!(target.is_dir(), "reviewed root must remain present");
        assert_eq!(
            std::fs::read_dir(&target).expect("read retained root").count(),
            0,
            "reviewed root must be empty after successful contents cleanup"
        );
        assert_eq!(
            std::fs::read(outside.join("SENTINEL")).expect("outside sentinel"),
            b"must-survive"
        );
        assert_eq!(measure_allocated_bytes(&reviewed).expect("measure after"), 0);
    }

    #[test]
    fn allocation_measurement_deduplicates_hard_links() {
        let root = tempfile::tempdir().expect("temp root");
        let target = root.path().join("target");
        std::fs::create_dir(&target).expect("target");
        let first = target.join("first.bin");
        let second = target.join("second.bin");
        std::fs::write(&first, vec![3u8; 16384]).expect("first file");
        std::fs::hard_link(&first, &second).expect("hard link");
        let metadata = std::fs::metadata(&first).expect("metadata");
        use std::os::unix::fs::MetadataExt;
        let expected = metadata.blocks().checked_mul(512).expect("allocation");

        let reviewed = File::open(&target).expect("open target");
        assert_eq!(measure_allocated_bytes(&reviewed).expect("measure"), expected);
    }
}
