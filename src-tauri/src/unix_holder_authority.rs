//! Unix post-open holder authorization bound to the reviewed filesystem object.
//!
//! The reviewed target is supplied as an already-open directory [`File`]. Tree
//! identities are collected descriptor-relatively; the root pathname is never
//! selected again. Holder evidence is consumed from lsof's machine-readable field
//! output and matched by `(device, inode)`, with the current DiskSage process
//! excluded because it intentionally retains the root capability.
//!
//! Missing or permission-limited evidence fails closed. No process is terminated.

use std::collections::HashSet;
use std::ffi::{CStr, CString};
use std::fs::File;
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

const MAX_TREE_ENTRIES: u64 = 2_000_000;
const MAX_TREE_DEPTH: usize = 128;
const HOLDER_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_LSOF_STDOUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_LSOF_STDERR_BYTES: usize = 1024 * 1024;

#[cfg(target_os = "linux")]
const RESOLVE_NO_XDEV: u64 = 0x01;
#[cfg(target_os = "linux")]
const RESOLVE_NO_SYMLINKS: u64 = 0x04;
#[cfg(target_os = "linux")]
const RESOLVE_BENEATH: u64 = 0x08;

#[cfg(target_os = "linux")]
#[repr(C)]
struct OpenHow {
    flags: u64,
    mode: u64,
    resolve: u64,
}

type FileIdentity = (u64, u64);

struct DirStream(*mut libc::DIR);

impl DirStream {
    fn from_fd(fd: RawFd) -> Result<Self, String> {
        let dot = b".\0";
        let independent = unsafe {
            libc::openat(
                fd,
                dot.as_ptr().cast(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
            )
        };
        if independent < 0 {
            return Err(format!(
                "cargo-target-holder-stream-open-failed:{}",
                std::io::Error::last_os_error()
            ));
        }
        let stream = unsafe { libc::fdopendir(independent) };
        if stream.is_null() {
            let error = std::io::Error::last_os_error();
            unsafe {
                libc::close(independent);
            }
            return Err(format!("cargo-target-holder-fdopendir-failed:{error}"));
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
                    "cargo-target-holder-readdir-failed:{}",
                    std::io::Error::from_raw_os_error(errno)
                ));
            }
            Ok(Some(CStr::from_ptr((*entry).d_name.as_ptr()).to_owned()))
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

fn stat_identity(stat: &libc::stat) -> Result<FileIdentity, String> {
    let device = u64::try_from(stat.st_dev)
        .map_err(|_| "cargo-target-holder-device-id-invalid".to_string())?;
    let inode = u64::try_from(stat.st_ino)
        .map_err(|_| "cargo-target-holder-inode-id-invalid".to_string())?;
    Ok((device, inode))
}

fn fstat_identity(file: &File) -> Result<(FileIdentity, libc::dev_t), String> {
    let mut stat = unsafe { std::mem::zeroed::<libc::stat>() };
    if unsafe { libc::fstat(file.as_raw_fd(), &mut stat) } != 0 {
        return Err(format!(
            "cargo-target-holder-root-fstat-failed:{}",
            std::io::Error::last_os_error()
        ));
    }
    if stat.st_mode & libc::S_IFMT != libc::S_IFDIR {
        return Err("cargo-target-holder-root-not-directory".into());
    }
    Ok((stat_identity(&stat)?, stat.st_dev))
}

fn stat_at(dir_fd: RawFd, name: &CStr) -> Result<libc::stat, String> {
    let mut stat = unsafe { std::mem::zeroed::<libc::stat>() };
    if unsafe {
        libc::fstatat(
            dir_fd,
            name.as_ptr(),
            &mut stat,
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0
    {
        return Err(format!(
            "cargo-target-holder-fstatat-failed:{}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(stat)
}

fn verify_opened_child(
    file: File,
    expected: &libc::stat,
    root_device: libc::dev_t,
) -> Result<File, String> {
    let mut opened = unsafe { std::mem::zeroed::<libc::stat>() };
    if unsafe { libc::fstat(file.as_raw_fd(), &mut opened) } != 0 {
        return Err(format!(
            "cargo-target-holder-child-fstat-failed:{}",
            std::io::Error::last_os_error()
        ));
    }
    if opened.st_dev != expected.st_dev || opened.st_ino != expected.st_ino {
        return Err("cargo-target-holder-child-replaced".into());
    }
    if opened.st_dev != root_device {
        return Err("cargo-target-holder-cross-device".into());
    }
    Ok(file)
}

#[cfg(target_os = "linux")]
fn open_child_directory(
    parent_fd: RawFd,
    name: &CStr,
    expected: &libc::stat,
    root_device: libc::dev_t,
) -> Result<File, String> {
    let how = OpenHow {
        flags: (libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC) as u64,
        mode: 0,
        resolve: RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS | RESOLVE_NO_XDEV,
    };
    let raw_fd = unsafe {
        libc::syscall(
            libc::SYS_openat2,
            parent_fd,
            name.as_ptr(),
            &how as *const OpenHow,
            std::mem::size_of::<OpenHow>(),
        )
    };
    if raw_fd < 0 {
        return Err(format!(
            "cargo-target-holder-openat2-failed:{}",
            std::io::Error::last_os_error()
        ));
    }
    let fd = i32::try_from(raw_fd)
        .map_err(|_| "cargo-target-holder-openat2-fd-overflow".to_string())?;
    let file = unsafe { File::from_raw_fd(fd) };
    verify_opened_child(file, expected, root_device)
}

#[cfg(target_os = "macos")]
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
            "cargo-target-holder-openat-failed:{}",
            std::io::Error::last_os_error()
        ));
    }
    let file = unsafe { File::from_raw_fd(fd) };
    verify_opened_child(file, expected, root_device)
}

struct IdentityWalk {
    entries: u64,
    deadline: Instant,
    root_device: libc::dev_t,
    identities: HashSet<FileIdentity>,
}

impl IdentityWalk {
    fn new(root_device: libc::dev_t, root_identity: FileIdentity) -> Self {
        let mut identities = HashSet::new();
        identities.insert(root_identity);
        Self {
            entries: 0,
            deadline: Instant::now() + HOLDER_TIMEOUT,
            root_device,
            identities,
        }
    }

    fn admit(&mut self, depth: usize) -> Result<(), String> {
        if depth > MAX_TREE_DEPTH {
            return Err("cargo-target-holder-depth-limit".into());
        }
        if Instant::now() >= self.deadline {
            return Err("cargo-target-holder-tree-timeout".into());
        }
        self.entries = self
            .entries
            .checked_add(1)
            .ok_or_else(|| "cargo-target-holder-entry-overflow".to_string())?;
        if self.entries > MAX_TREE_ENTRIES {
            return Err("cargo-target-holder-entry-limit".into());
        }
        Ok(())
    }
}

fn collect_directory_identities(
    directory: &File,
    depth: usize,
    state: &mut IdentityWalk,
) -> Result<(), String> {
    let dir_fd = directory.as_raw_fd();
    let mut stream = DirStream::from_fd(dir_fd)?;
    while let Some(name) = stream.next_name()? {
        if name.as_bytes() == b"." || name.as_bytes() == b".." {
            continue;
        }
        state.admit(depth)?;
        let stat = stat_at(dir_fd, &name)?;
        if stat.st_dev != state.root_device {
            return Err("cargo-target-holder-cross-device".into());
        }
        state.identities.insert(stat_identity(&stat)?);
        let kind = stat.st_mode & libc::S_IFMT;
        if kind == libc::S_IFDIR {
            let child = open_child_directory(dir_fd, &name, &stat, state.root_device)?;
            collect_directory_identities(&child, depth + 1, state)?;
        } else if kind != libc::S_IFREG && kind != libc::S_IFLNK {
            return Err("cargo-target-holder-unsafe-entry-type".into());
        }
    }
    Ok(())
}

fn collect_reviewed_identities(root: &File) -> Result<HashSet<FileIdentity>, String> {
    let (root_identity, root_device) = fstat_identity(root)?;
    let mut state = IdentityWalk::new(root_device, root_identity);
    collect_directory_identities(root, 1, &mut state)?;
    Ok(state.identities)
}

fn resolve_lsof_executable() -> Result<PathBuf, String> {
    let candidates = [
        PathBuf::from("/usr/sbin/lsof"),
        PathBuf::from("/usr/bin/lsof"),
        PathBuf::from("/usr/local/sbin/lsof"),
        PathBuf::from("/opt/homebrew/bin/lsof"),
        PathBuf::from("/usr/local/bin/lsof"),
    ];
    for candidate in candidates {
        let Ok(metadata) = std::fs::symlink_metadata(&candidate) else {
            continue;
        };
        let executable = if metadata.file_type().is_symlink() {
            std::fs::canonicalize(&candidate)
                .ok()
                .and_then(|path| std::fs::metadata(path).ok())
                .is_some_and(|resolved| {
                    resolved.is_file() && resolved.permissions().mode() & 0o111 != 0
                })
        } else {
            metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
        };
        if executable {
            return Ok(candidate);
        }
    }
    Err("cargo-target-lsof-unavailable".into())
}

#[derive(Debug)]
struct BoundedPipe {
    bytes: Vec<u8>,
    truncated: bool,
}

fn read_bounded(mut pipe: impl Read, limit: usize) -> std::io::Result<BoundedPipe> {
    let mut bytes = Vec::new();
    let mut truncated = false;
    let mut chunk = [0u8; 8192];
    loop {
        match pipe.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                let remaining = limit.saturating_sub(bytes.len());
                let retained = remaining.min(read);
                bytes.extend_from_slice(&chunk[..retained]);
                if retained < read {
                    truncated = true;
                }
            }
            Err(error) => return Err(error),
        }
    }
    Ok(BoundedPipe { bytes, truncated })
}

struct LsofOutput {
    status: ExitStatus,
    stdout: BoundedPipe,
    stderr: BoundedPipe,
}

fn terminate_process_group(child: &mut std::process::Child) {
    unsafe {
        let _ = libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn run_lsof(lsof: &Path) -> Result<LsofOutput, String> {
    let mut command = Command::new(lsof);
    command
        .arg("-nP")
        .arg("-F0pftDi")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("cargo-target-holder-lsof-spawn-failed:{error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "cargo-target-holder-lsof-stdout-unavailable".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "cargo-target-holder-lsof-stderr-unavailable".to_string())?;
    let stdout_reader = std::thread::spawn(move || read_bounded(stdout, MAX_LSOF_STDOUT_BYTES));
    let stderr_reader = std::thread::spawn(move || read_bounded(stderr, MAX_LSOF_STDERR_BYTES));
    let deadline = Instant::now() + HOLDER_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                terminate_process_group(&mut child);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err("cargo-target-holder-lsof-timeout".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(error) => {
                terminate_process_group(&mut child);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!("cargo-target-holder-lsof-wait-failed:{error}"));
            }
        }
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| "cargo-target-holder-lsof-stdout-reader-panicked".to_string())?
        .map_err(|error| format!("cargo-target-holder-lsof-stdout-read-failed:{error}"))?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| "cargo-target-holder-lsof-stderr-reader-panicked".to_string())?
        .map_err(|error| format!("cargo-target-holder-lsof-stderr-read-failed:{error}"))?;
    Ok(LsofOutput {
        status,
        stdout,
        stderr,
    })
}

#[derive(Default)]
struct FileRecord {
    pid: u32,
    descriptor: Option<Vec<u8>>,
    file_type: Option<Vec<u8>>,
    device: Option<u64>,
    inode: Option<u64>,
}

fn parse_u32_ascii(value: &[u8], field: &str) -> Result<u32, String> {
    let value = std::str::from_utf8(value)
        .map_err(|_| format!("cargo-target-holder-lsof-invalid-{field}"))?;
    value
        .parse::<u32>()
        .map_err(|_| format!("cargo-target-holder-lsof-invalid-{field}"))
}

fn parse_u64_ascii(value: &[u8], field: &str) -> Result<u64, String> {
    let value = std::str::from_utf8(value)
        .map_err(|_| format!("cargo-target-holder-lsof-invalid-{field}"))?;
    value
        .parse::<u64>()
        .map_err(|_| format!("cargo-target-holder-lsof-invalid-{field}"))
}

fn parse_device(value: &[u8]) -> Result<u64, String> {
    let value = std::str::from_utf8(value)
        .map_err(|_| "cargo-target-holder-lsof-invalid-device".to_string())?;
    let hex = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .ok_or_else(|| "cargo-target-holder-lsof-invalid-device".to_string())?;
    u64::from_str_radix(hex, 16)
        .map_err(|_| "cargo-target-holder-lsof-invalid-device".to_string())
}

fn filesystem_type(file_type: &[u8]) -> bool {
    file_type == b"REG"
        || file_type == b"VREG"
        || file_type == b"DIR"
        || file_type == b"VDIR"
        || file_type == b"NFS"
}

fn finish_record(
    record: &mut Option<FileRecord>,
    reviewed: &HashSet<FileIdentity>,
    self_pid: u32,
) -> Result<(), String> {
    let Some(record) = record.take() else {
        return Ok(());
    };
    if record.pid == self_pid {
        return Ok(());
    }
    let descriptor = record.descriptor.as_deref().unwrap_or_default();
    if descriptor == b"NOFD" || descriptor == b"err" {
        return Err("cargo-target-active-use-probe-failed:lsof-permission-limited".into());
    }
    if let (Some(device), Some(inode)) = (record.device, record.inode) {
        if reviewed.contains(&(device, inode)) {
            return Err("cargo-target-active-holders-present".into());
        }
        return Ok(());
    }
    if record
        .file_type
        .as_deref()
        .is_some_and(filesystem_type)
    {
        return Err("cargo-target-active-use-probe-failed:lsof-filesystem-identity-missing".into());
    }
    Ok(())
}

fn classify_lsof_fields(
    reviewed: &HashSet<FileIdentity>,
    status_code: i32,
    stdout: &[u8],
    stderr: &[u8],
    self_pid: u32,
) -> Result<(), String> {
    if status_code != 0 {
        return Err(format!(
            "cargo-target-active-use-probe-failed:lsof-exit-status:{status_code}"
        ));
    }
    if !stderr.is_empty() {
        return Err("cargo-target-active-use-probe-failed:lsof-stderr-nonempty".into());
    }
    let mut current_pid = None;
    let mut current_file = None;
    let mut process_records = 0u64;
    for field in stdout
        .split(|byte| *byte == 0 || *byte == b'\n')
        .filter(|field| !field.is_empty())
    {
        let (tag, value) = field
            .split_first()
            .ok_or_else(|| "cargo-target-holder-lsof-empty-field".to_string())?;
        match *tag {
            b'p' => {
                finish_record(&mut current_file, reviewed, self_pid)?;
                let pid = parse_u32_ascii(value, "pid")?;
                current_pid = Some(pid);
                process_records = process_records
                    .checked_add(1)
                    .ok_or_else(|| "cargo-target-holder-lsof-process-overflow".to_string())?;
            }
            b'f' => {
                finish_record(&mut current_file, reviewed, self_pid)?;
                let pid = current_pid
                    .ok_or_else(|| "cargo-target-holder-lsof-file-without-process".to_string())?;
                current_file = Some(FileRecord {
                    pid,
                    descriptor: Some(value.to_vec()),
                    ..FileRecord::default()
                });
            }
            b't' => {
                current_file
                    .as_mut()
                    .ok_or_else(|| "cargo-target-holder-lsof-type-without-file".to_string())?
                    .file_type = Some(value.to_vec());
            }
            b'D' => {
                current_file
                    .as_mut()
                    .ok_or_else(|| "cargo-target-holder-lsof-device-without-file".to_string())?
                    .device = Some(parse_device(value)?);
            }
            b'i' => {
                current_file
                    .as_mut()
                    .ok_or_else(|| "cargo-target-holder-lsof-inode-without-file".to_string())?
                    .inode = Some(parse_u64_ascii(value, "inode")?);
            }
            _ => {}
        }
    }
    finish_record(&mut current_file, reviewed, self_pid)?;
    if process_records == 0 {
        return Err("cargo-target-active-use-probe-failed:lsof-empty-output".into());
    }
    Ok(())
}

/// Refuses cleanup when any non-DiskSage process holds the reviewed root or a descendant.
///
/// The probe never starts from a pathname. It first snapshots the reviewed tree's
/// `(device, inode)` identities through the retained directory capability, then matches
/// those identities against complete machine-readable lsof records. Any warning,
/// permission-limited `NOFD`, truncated output, malformed filesystem identity, timeout,
/// or unsupported lsof result fails closed.
pub(crate) fn ensure_opened_target_has_no_active_holders(root: &File) -> Result<(), String> {
    let reviewed = collect_reviewed_identities(root)?;
    let lsof = resolve_lsof_executable()?;
    let output = run_lsof(&lsof)?;
    if output.stdout.truncated || output.stderr.truncated {
        return Err("cargo-target-active-use-probe-failed:lsof-output-truncated".into());
    }
    let status_code = output.status.code().unwrap_or(127);
    classify_lsof_fields(
        &reviewed,
        status_code,
        &output.stdout.bytes,
        &output.stderr.bytes,
        std::process::id(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    fn field_snapshot(pid: u32, identity: FileIdentity) -> Vec<u8> {
        format!(
            "p{pid}\0f3\0tREG\0D0x{:x}\0i{}\0\n",
            identity.0, identity.1
        )
        .into_bytes()
    }

    #[test]
    fn retained_capability_identity_walk_does_not_follow_replacement_path() {
        let root = tempfile::tempdir().expect("temp root");
        let target = root.path().join("target");
        let stash = root.path().join("reviewed");
        std::fs::create_dir(&target).expect("target");
        std::fs::write(target.join("artifact"), b"payload").expect("artifact");
        let reviewed = File::open(&target).expect("reviewed root");
        let artifact = std::fs::metadata(target.join("artifact")).expect("artifact metadata");
        let artifact_identity = (artifact.dev(), artifact.ino());

        std::fs::rename(&target, &stash).expect("stash reviewed target");
        std::fs::create_dir(&target).expect("replacement target");
        std::fs::write(target.join("REPLACEMENT_SENTINEL"), b"replacement")
            .expect("replacement sentinel");
        let replacement = std::fs::metadata(target.join("REPLACEMENT_SENTINEL"))
            .expect("replacement metadata");
        let replacement_identity = (replacement.dev(), replacement.ino());

        let identities = collect_reviewed_identities(&reviewed).expect("collect identities");
        assert!(identities.contains(&artifact_identity));
        assert!(!identities.contains(&replacement_identity));
    }

    #[test]
    fn matching_non_self_holder_blocks_authorization() {
        let reviewed = HashSet::from([(0x1cu64, 42u64)]);
        let output = field_snapshot(std::process::id().saturating_add(1), (0x1c, 42));
        assert_eq!(
            classify_lsof_fields(&reviewed, 0, &output, b"", std::process::id()).unwrap_err(),
            "cargo-target-active-holders-present"
        );
    }

    #[test]
    fn current_process_is_excluded_from_holder_match() {
        let reviewed = HashSet::from([(0x1cu64, 42u64)]);
        let output = field_snapshot(std::process::id(), (0x1c, 42));
        assert!(classify_lsof_fields(&reviewed, 0, &output, b"", std::process::id()).is_ok());
    }

    #[test]
    fn nofd_is_permission_limited_and_fails_closed() {
        let reviewed = HashSet::from([(0x1cu64, 42u64)]);
        let output = b"p999999\0fNOFD\0\n";
        assert_eq!(
            classify_lsof_fields(&reviewed, 0, output, b"", std::process::id()).unwrap_err(),
            "cargo-target-active-use-probe-failed:lsof-permission-limited"
        );
    }

    #[test]
    fn filesystem_record_without_device_inode_fails_closed() {
        let reviewed = HashSet::from([(0x1cu64, 42u64)]);
        let output = b"p999999\0f3\0tREG\0\n";
        assert_eq!(
            classify_lsof_fields(&reviewed, 0, output, b"", std::process::id()).unwrap_err(),
            "cargo-target-active-use-probe-failed:lsof-filesystem-identity-missing"
        );
    }

    #[test]
    fn warning_or_nonzero_lsof_result_never_authorizes_cleanup() {
        let reviewed = HashSet::from([(0x1cu64, 42u64)]);
        let output = b"p999999\0f3\0tREG\0D0x2\0i7\0\n";
        assert!(classify_lsof_fields(&reviewed, 1, output, b"", std::process::id()).is_err());
        assert!(classify_lsof_fields(
            &reviewed,
            0,
            output,
            b"lsof: WARNING: incomplete kernel visibility\n",
            std::process::id(),
        )
        .is_err());
    }
}
