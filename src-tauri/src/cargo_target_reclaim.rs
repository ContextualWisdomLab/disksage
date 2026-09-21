//! Cargo `target/` reclaim with fail-closed tool accounting.
//!
//! Operational scripts previously treated a missing `cargo` on PATH as a successful
//! `CLEANED` row when directory size was unchanged. This module requires an absolute
//! cargo executable, a zero exit status, and records reclaim bytes from measured
//! before/after size (zero delta ⇒ reclaim 0, never a success reclaim claim).
//!
//! Deletion scope is pinned with `cargo clean --target-dir <measured>` so workspace
//! root targets, `CARGO_TARGET_DIR`, or `.cargo/config` `build.target-dir` cannot
//! redirect deletion away from the inspected path.
//!
//! Before any `cargo clean`, the measured target must pass ownership + active-use
//! probes. Missing/`lsof` exit 127 / unexpected probe failure fails closed (no clean).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::fs::File;
#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
#[cfg(unix)]
use std::os::unix::process::CommandExt;

const CARGO_CLEAN_TIMEOUT: Duration = Duration::from_secs(600);
const LSOF_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_COMMAND_OUTPUT_BYTES: usize = 64 * 1024;

/// Active-use / ownership gate invoked before deletion. Tests inject stubs.
pub(crate) type ActiveUseProbe = fn(&Path) -> Result<(), String>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoTargetCleanResult {
    pub cargo_path: PathBuf,
    pub project_dir: PathBuf,
    pub target_dir: PathBuf,
    pub bytes_before: u64,
    pub bytes_after: u64,
    pub observed_reduction_bytes: u64,
    pub status_code: i32,
    pub executed: bool,
}

/// Resolve a concrete cargo binary. Never relies on an unverified PATH lookup alone.
pub fn resolve_cargo_executable() -> Result<PathBuf, String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(home) = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        candidates.push(home.join("bin").join(cargo_bin_name()));
    }
    if let Some(home) = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        candidates.push(home.join(".cargo").join("bin").join(cargo_bin_name()));
    }
    #[cfg(target_os = "macos")]
    {
        candidates.push(PathBuf::from("/opt/homebrew/bin/cargo"));
        candidates.push(PathBuf::from("/usr/local/bin/cargo"));
    }
    #[cfg(target_os = "linux")]
    {
        candidates.push(PathBuf::from("/usr/bin/cargo"));
        candidates.push(PathBuf::from("/usr/local/bin/cargo"));
    }

    for candidate in candidates {
        if executable_file(&candidate) {
            return Ok(candidate);
        }
    }
    Err("cargo-executable-unavailable".into())
}

fn cargo_bin_name() -> &'static str {
    if cfg!(windows) {
        "cargo.exe"
    } else {
        "cargo"
    }
}

fn executable_file(path: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if !meta.is_file() || meta.file_type().is_symlink() {
        // Allow normal files; also accept symlink-to-file via canonicalize follow.
        if meta.file_type().is_symlink() {
            let Ok(real) = std::fs::canonicalize(path) else {
                return false;
            };
            return std::fs::metadata(&real).is_ok_and(|m| m.is_file());
        }
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn bounded_dir_size(path: &Path) -> Result<u64, String> {
    if !path.exists() {
        return Ok(0);
    }
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    let mut entries = 0u64;
    const MAX_ENTRIES: u64 = 2_000_000;
    while let Some(dir) = stack.pop() {
        let read = std::fs::read_dir(&dir).map_err(|e| format!("cargo-target-size-read-failed:{e}"))?;
        for entry in read {
            let entry = entry.map_err(|e| format!("cargo-target-size-entry-failed:{e}"))?;
            entries += 1;
            if entries > MAX_ENTRIES {
                return Err("cargo-target-size-entry-limit".into());
            }
            let ft = entry
                .file_type()
                .map_err(|e| format!("cargo-target-size-type-failed:{e}"))?;
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                stack.push(entry.path());
            } else if ft.is_file() {
                let len = entry
                    .metadata()
                    .map_err(|e| format!("cargo-target-size-meta-failed:{e}"))?
                    .len();
                total = total.saturating_add(len);
            }
        }
    }
    Ok(total)
}

fn ensure_absolute_project(project_dir: &Path) -> Result<(), String> {
    if !project_dir.is_absolute() {
        return Err("cargo-target-project-not-absolute".into());
    }
    let manifest = project_dir.join("Cargo.toml");
    if !manifest.is_file() {
        return Err("cargo-target-manifest-missing".into());
    }
    Ok(())
}

/// Canonical component containment (not string-prefix). Requires a strict child path.
fn is_strict_canonical_child(root: &Path, candidate: &Path) -> bool {
    let root_c: Vec<_> = root.components().collect();
    let cand_c: Vec<_> = candidate.components().collect();
    cand_c.len() > root_c.len() && cand_c.iter().zip(root_c.iter()).all(|(a, b)| a == b)
}

/// Measured target must be absolute and a strict canonical child of `project_dir`
/// (rejects symlink escapes / shared redirects outside the project tree).
fn resolve_measured_target_dir(project_dir: &Path, target_dir: &Path) -> Result<PathBuf, String> {
    if !target_dir.is_absolute() {
        return Err("cargo-target-dir-not-absolute".into());
    }
    let project_canon = std::fs::canonicalize(project_dir)
        .map_err(|e| format!("cargo-target-project-canonicalize-failed:{e}"))?;
    // `Path::exists` reports false for dangling links, so inspect the final component first.
    let target_canon = match std::fs::symlink_metadata(target_dir) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err("cargo-target-dir-symlink".into());
        }
        Ok(_) => std::fs::canonicalize(target_dir)
            .map_err(|e| format!("cargo-target-dir-canonicalize-failed:{e}"))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = target_dir
                .parent()
                .ok_or_else(|| "cargo-target-dir-parent-missing".to_string())?;
            let parent_canon = std::fs::canonicalize(parent)
                .map_err(|e| format!("cargo-target-dir-parent-canonicalize-failed:{e}"))?;
            let name = target_dir
                .file_name()
                .ok_or_else(|| "cargo-target-dir-name-missing".to_string())?;
            parent_canon.join(name)
        }
        Err(error) => return Err(format!("cargo-target-dir-metadata-failed:{error}")),
    };
    if !is_strict_canonical_child(&project_canon, &target_canon) {
        return Err("cargo-target-dir-outside-project".into());
    }
    Ok(target_canon)
}

#[derive(Debug)]
struct BoundedCommandOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

fn read_bounded(mut pipe: impl Read) -> std::io::Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match pipe.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) if output.len() < MAX_COMMAND_OUTPUT_BYTES => {
                let retained = (MAX_COMMAND_OUTPUT_BYTES - output.len()).min(read);
                output.extend_from_slice(&chunk[..retained]);
            }
            Ok(_) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(output)
}

fn terminate_child(child: &mut std::process::Child) {
    #[cfg(unix)]
    unsafe {
        // The child creates a private process group, so descendants cannot outlive timeout.
        let _ = libc::kill(-(child.id() as libc::pid_t), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn run_bounded_command(
    command: &mut Command,
    timeout: Duration,
    error_prefix: &str,
) -> Result<BoundedCommandOutput, String> {
    #[cfg(unix)]
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("{error_prefix}-spawn-failed:{error}"))?;
    let stdout = child.stdout.take().ok_or_else(|| format!("{error_prefix}-stdout-unavailable"))?;
    let stderr = child.stderr.take().ok_or_else(|| format!("{error_prefix}-stderr-unavailable"))?;
    let stdout_reader = std::thread::spawn(move || read_bounded(stdout));
    let stderr_reader = std::thread::spawn(move || read_bounded(stderr));
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                terminate_child(&mut child);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!("{error_prefix}-timeout"));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(error) => {
                terminate_child(&mut child);
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!("{error_prefix}-wait-failed:{error}"));
            }
        }
    };
    let stdout = stdout_reader.join()
        .map_err(|_| format!("{error_prefix}-stdout-reader-panicked"))?
        .map_err(|error| format!("{error_prefix}-stdout-read-failed:{error}"))?;
    let stderr = stderr_reader.join()
        .map_err(|_| format!("{error_prefix}-stderr-reader-panicked"))?
        .map_err(|error| format!("{error_prefix}-stderr-read-failed:{error}"))?;
    Ok(BoundedCommandOutput { status, stdout, stderr })
}

#[cfg(unix)]
struct OpenedTargetDir {
    file: File,
    handle_path: PathBuf,
}

#[cfg(unix)]
fn open_verified_target_dir(
    target_dir: &Path,
    expected: &std::fs::Metadata,
) -> Result<OpenedTargetDir, String> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true).custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let file = options.open(target_dir)
        .map_err(|error| format!("cargo-target-dir-open-failed:{error}"))?;
    let opened = file.metadata()
        .map_err(|error| format!("cargo-target-dir-open-metadata-failed:{error}"))?;
    if !opened.is_dir() || opened.dev() != expected.dev() || opened.ino() != expected.ino() {
        return Err("cargo-target-dir-replaced".into());
    }

    let fd = file.as_raw_fd();
    #[cfg(target_os = "linux")]
    let handle_path = PathBuf::from(format!("/proc/self/fd/{fd}"));
    #[cfg(target_os = "macos")]
    let handle_path = PathBuf::from(format!("/dev/fd/{fd}"));
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    return Err("cargo-target-identity-bound-cleanup-unsupported".into());

    let handle_metadata = std::fs::metadata(&handle_path)
        .map_err(|error| format!("cargo-target-dir-handle-path-failed:{error}"))?;
    if handle_metadata.dev() != opened.dev() || handle_metadata.ino() != opened.ino() {
        return Err("cargo-target-dir-handle-identity-mismatch".into());
    }
    Ok(OpenedTargetDir { file, handle_path })
}

#[cfg(not(unix))]
struct OpenedTargetDir {
    handle_path: PathBuf,
}

#[cfg(not(unix))]
fn open_verified_target_dir(
    _target_dir: &Path,
    _expected: &std::fs::Metadata,
) -> Result<OpenedTargetDir, String> {
    Err("cargo-target-identity-bound-cleanup-unsupported".into())
}

#[cfg(unix)]
struct DetachedTargetDir {
    opened: OpenedTargetDir,
    original_path: PathBuf,
    clean_path: PathBuf,
    quarantine_path: PathBuf,
    committed: bool,
}

#[cfg(unix)]
impl DetachedTargetDir {
    fn commit(&mut self) {
        self.committed = true;
    }
}

#[cfg(unix)]
impl Drop for DetachedTargetDir {
    fn drop(&mut self) {
        if !self.committed {
            let clean = std::fs::symlink_metadata(&self.clean_path);
            let opened = self.opened.file.metadata();
            let original_missing = std::fs::symlink_metadata(&self.original_path)
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound);
            let same_opened_dir = match (clean, opened) {
                (Ok(clean), Ok(opened)) => {
                    clean.is_dir()
                        && !clean.file_type().is_symlink()
                        && clean.dev() == opened.dev()
                        && clean.ino() == opened.ino()
                }
                _ => false,
            };
            if original_missing && same_opened_dir {
                let _ = std::fs::rename(&self.clean_path, &self.original_path);
            }
        }
        if self.committed || std::fs::symlink_metadata(&self.clean_path).is_err() {
            let _ = std::fs::remove_dir(&self.quarantine_path);
        }
    }
}

#[cfg(unix)]
fn detach_verified_target_dir(
    target_dir: &Path,
    opened: OpenedTargetDir,
) -> Result<DetachedTargetDir, String> {
    let parent = target_dir.parent().ok_or_else(|| "cargo-target-dir-parent-missing".to_string())?;
    let quarantine = tempfile::Builder::new()
        .prefix(".disksage-cargo-clean-")
        .tempdir_in(parent)
        .map_err(|error| format!("cargo-target-quarantine-create-failed:{error}"))?;
    let clean_path = quarantine.path().join("target");
    std::fs::rename(target_dir, &clean_path)
        .map_err(|error| format!("cargo-target-dir-detach-failed:{error}"))?;
    let quarantine_path = quarantine.keep();
    let moved = std::fs::symlink_metadata(&clean_path)
        .map_err(|error| format!("cargo-target-dir-detached-metadata-failed:{error}"))?;
    let expected = opened.file.metadata()
        .map_err(|error| format!("cargo-target-dir-open-metadata-failed:{error}"))?;
    if moved.file_type().is_symlink()
        || !moved.is_dir()
        || moved.dev() != expected.dev()
        || moved.ino() != expected.ino()
    {
        if std::fs::symlink_metadata(target_dir)
            .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound)
        {
            let _ = std::fs::rename(&clean_path, target_dir);
        }
        let _ = std::fs::remove_dir(&quarantine_path);
        return Err("cargo-target-dir-replaced".into());
    }
    Ok(DetachedTargetDir {
        opened,
        original_path: target_dir.to_path_buf(),
        clean_path,
        quarantine_path,
        committed: false,
    })
}

#[cfg(not(unix))]
struct DetachedTargetDir {
    opened: OpenedTargetDir,
    clean_path: PathBuf,
}

#[cfg(not(unix))]
impl DetachedTargetDir {
    fn commit(&mut self) {}
}

#[cfg(not(unix))]
fn detach_verified_target_dir(
    _target_dir: &Path,
    _opened: OpenedTargetDir,
) -> Result<DetachedTargetDir, String> {
    Err("cargo-target-identity-bound-cleanup-unsupported".into())
}

/// Classify `lsof` completion for a cargo target tree.
///
/// macOS `lsof +D` may return exit 1 **with** holder lines in stdout (see
/// independent review active-holder-reproduction.json). Any non-empty stdout is
/// therefore treated as active holders regardless of exit 0/1.
///
/// Any non-empty stderr is incomplete inspection evidence and fails closed,
/// including exit 0 + empty stdout + warning stderr (report-a118 warning_zero).
///
/// Only exit 0 or 1 with **both** stdout and stderr empty is an admissible
/// empty no-match. Exit 127 and every other non-zero outcome fail closed.
pub(crate) fn classify_target_lsof_result(
    exit_code: i32,
    stdout: &str,
    stderr: &str,
) -> Result<(), String> {
    if !stdout.trim().is_empty() {
        return Err("cargo-target-active-holders-present".into());
    }
    if !stderr.trim().is_empty() {
        return Err(format!(
            "cargo-target-active-use-probe-failed:lsof-stderr-nonempty:exit:{exit_code}"
        ));
    }
    if exit_code == 0 || exit_code == 1 {
        return Ok(());
    }
    Err(format!(
        "cargo-target-active-use-probe-failed:lsof-exit-status:{exit_code}"
    ))
}

#[cfg(unix)]
fn ensure_target_owned_by_self(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(path).map_err(|e| format!("cargo-target-owner-stat-failed:{e}"))?;
    let self_uid = unsafe { libc::geteuid() };
    if meta.uid() != self_uid {
        return Err(format!(
            "cargo-target-owner-mismatch:owner_uid={}:self_uid={self_uid}",
            meta.uid()
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_target_owned_by_self(_path: &Path) -> Result<(), String> {
    // Non-Unix platforms lack a portable owner probe here; refuse mutation.
    Err("cargo-target-owner-probe-unsupported".into())
}

fn resolve_lsof_executable() -> Result<PathBuf, String> {
    let candidates = [
        PathBuf::from("/usr/sbin/lsof"),
        PathBuf::from("/usr/bin/lsof"),
        PathBuf::from("/opt/homebrew/bin/lsof"),
    ];
    for candidate in candidates {
        if executable_file(&candidate) {
            return Ok(candidate);
        }
    }
    Err("cargo-target-lsof-unavailable".into())
}

fn run_target_lsof(
    lsof: &Path,
    target_dir: &Path,
    timeout: Duration,
) -> Result<BoundedCommandOutput, String> {
    let mut command = Command::new(lsof);
    command.arg("+D").arg(target_dir);
    run_bounded_command(&mut command, timeout, "cargo-target-lsof").map_err(|error| {
        if error.contains("spawn-failed:No such file or directory") {
            "cargo-target-lsof-unavailable".to_string()
        } else {
            error
        }
    })
}

/// Default production probe: ownership + fail-closed `lsof +D` on the measured target.
pub(crate) fn ensure_target_safe_to_reclaim(target_dir: &Path) -> Result<(), String> {
    if !target_dir.exists() {
        return Ok(());
    }
    ensure_target_owned_by_self(target_dir)?;
    let lsof = resolve_lsof_executable()?;
    let output = run_target_lsof(&lsof, target_dir, LSOF_TIMEOUT)?;
    let code = output.status.code().unwrap_or(127);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    classify_target_lsof_result(code, &stdout, &stderr)
}

/// Run `cargo clean` for a project directory that owns a `Cargo.toml`.
///
/// Always passes `--target-dir <project>/target` so measurement and deletion match.
///
/// Fail-closed:
/// - missing cargo executable ⇒ `Err`
/// - spawn failure / command-not-found ⇒ `Err`
/// - active holders / lsof probe failure / owner mismatch ⇒ `Err` (no clean)
/// - non-zero exit ⇒ `Err` (no success reclaim)
/// - zero size delta with exit 0 ⇒ `Ok` with `observed_reduction_bytes == 0` (not a reclaim success claim)
pub fn clean_cargo_target(project_dir: &Path) -> Result<CargoTargetCleanResult, String> {
    ensure_absolute_project(project_dir)?;
    let cargo = resolve_cargo_executable()?;
    let target_dir = resolve_measured_target_dir(project_dir, &project_dir.join("target"))?;
    clean_cargo_target_with(project_dir, &target_dir, &cargo)
}

/// Test/hook seam: run clean with an explicit cargo binary and measured target dir.
pub(crate) fn clean_cargo_target_with(
    project_dir: &Path,
    target_dir: &Path,
    cargo: &Path,
) -> Result<CargoTargetCleanResult, String> {
    clean_cargo_target_with_active_use(project_dir, target_dir, cargo, ensure_target_safe_to_reclaim)
}

/// Same as [`clean_cargo_target_with`] but with an injectable active-use probe.
pub(crate) fn clean_cargo_target_with_active_use(
    project_dir: &Path,
    target_dir: &Path,
    cargo: &Path,
    active_use: ActiveUseProbe,
) -> Result<CargoTargetCleanResult, String> {
    ensure_absolute_project(project_dir)?;
    if !cargo.is_absolute() {
        return Err("cargo-executable-not-absolute".into());
    }
    if !executable_file(cargo) {
        return Err("cargo-executable-unavailable".into());
    }
    let target_dir = resolve_measured_target_dir(project_dir, target_dir)?;

    let initial_metadata = match std::fs::symlink_metadata(&target_dir) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err("cargo-target-dir-unsafe-type".into());
        }
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(CargoTargetCleanResult {
                cargo_path: cargo.to_path_buf(),
                project_dir: project_dir.to_path_buf(),
                target_dir,
                bytes_before: 0,
                bytes_after: 0,
                observed_reduction_bytes: 0,
                status_code: 0,
                executed: false,
            });
        }
        Err(error) => return Err(format!("cargo-target-dir-metadata-failed:{error}")),
    };
    active_use(&target_dir)?;
    // Bind measurement and cleanup to the authorized object, not its replaceable pathname.
    let opened_target = open_verified_target_dir(&target_dir, &initial_metadata)?;
    let bytes_before = bounded_dir_size(&opened_target.handle_path)?;
    let mut detached_target = detach_verified_target_dir(&target_dir, opened_target)?;

    let mut command = Command::new(cargo);
    command
        .arg("clean")
        .arg("--target-dir")
        .arg(&detached_target.clean_path)
        .current_dir(project_dir);
    let output = run_bounded_command(&mut command, CARGO_CLEAN_TIMEOUT, "cargo-clean")
        .map_err(|error| {
            if error.contains("spawn-failed:No such file or directory") {
                "cargo-executable-unavailable".to_string()
            } else {
                error
            }
        })?;

    let status_code = output.status.code().unwrap_or(-1);
    if status_code != 0 {
        return Err(format!("cargo-clean-exit-nonzero:{status_code}"));
    }

    let bytes_after = bounded_dir_size(&detached_target.opened.handle_path)?;
    detached_target.commit();
    Ok(CargoTargetCleanResult {
        cargo_path: cargo.to_path_buf(),
        project_dir: project_dir.to_path_buf(),
        target_dir,
        bytes_before,
        bytes_after,
        observed_reduction_bytes: bytes_before.saturating_sub(bytes_after),
        status_code,
        executed: true,
    })
}

/// Classify a finished clean for ledgers: only positive observed reduction counts as reclaim.
pub fn ledger_reclaim_bytes(result: &CargoTargetCleanResult) -> u64 {
    if result.status_code != 0 || !result.executed {
        0
    } else {
        result.observed_reduction_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn zero_delta_is_reclaim_zero_not_success_bytes() {
        let result = CargoTargetCleanResult {
            cargo_path: PathBuf::from("/tmp/cargo"),
            project_dir: PathBuf::from("/tmp/proj"),
            target_dir: PathBuf::from("/tmp/proj/target"),
            bytes_before: 1000,
            bytes_after: 1000,
            observed_reduction_bytes: 0,
            status_code: 0,
            executed: true,
        };
        assert_eq!(ledger_reclaim_bytes(&result), 0);
    }

    #[test]
    fn nonzero_exit_never_counts_as_reclaim() {
        let result = CargoTargetCleanResult {
            cargo_path: PathBuf::from("/tmp/cargo"),
            project_dir: PathBuf::from("/tmp/proj"),
            target_dir: PathBuf::from("/tmp/proj/target"),
            bytes_before: 5000,
            bytes_after: 1000,
            observed_reduction_bytes: 4000,
            status_code: 127,
            executed: true,
        };
        assert_eq!(ledger_reclaim_bytes(&result), 0);
    }

    #[test]
    fn positive_delta_with_zero_exit_counts_reclaim() {
        let result = CargoTargetCleanResult {
            cargo_path: PathBuf::from("/tmp/cargo"),
            project_dir: PathBuf::from("/tmp/proj"),
            target_dir: PathBuf::from("/tmp/proj/target"),
            bytes_before: 5000,
            bytes_after: 1000,
            observed_reduction_bytes: 4000,
            status_code: 0,
            executed: true,
        };
        assert_eq!(ledger_reclaim_bytes(&result), 4000);
    }

    #[test]
    fn relative_project_dir_is_rejected() {
        let err = clean_cargo_target(Path::new("relative/project")).unwrap_err();
        assert_eq!(err, "cargo-target-project-not-absolute");
    }

    #[cfg(unix)]
    #[test]
    fn target_outside_project_is_rejected() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "disksage-cargo-outside-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n").unwrap();
        let outside = std::env::temp_dir().join(format!(
            "disksage-cargo-sentinel-{}",
            std::process::id()
        ));
        fs::create_dir_all(&outside).unwrap();
        let fake_cargo = root.join("fake-cargo");
        fs::write(&fake_cargo, "#!/bin/sh\nexit 0\n").unwrap();
        let mut perms = fs::metadata(&fake_cargo).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&fake_cargo, perms).unwrap();

        let err = clean_cargo_target_with(&root, &outside, &fake_cargo).unwrap_err();
        assert_eq!(err, "cargo-target-dir-outside-project");
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&outside);
    }

    #[test]
    fn consumer_missing_cargo_is_unavailable() {
        let root = std::env::temp_dir().join(format!(
            "disksage-cargo-missing-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n").unwrap();
        let missing = root.join("no-such-cargo-binary");
        let err = clean_cargo_target_with_active_use(
            &root,
            &root.join("target"),
            &missing,
            |_| Ok(()),
        )
        .unwrap_err();
        assert_eq!(err, "cargo-executable-unavailable");
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn consumer_nonzero_exit_is_fail_closed() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "disksage-cargo-nonzero-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let target = root.join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("junk"), "x").unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n").unwrap();
        let fake = root.join("fake-cargo");
        fs::write(&fake, "#!/bin/sh\nexit 9\n").unwrap();
        let mut perms = fs::metadata(&fake).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&fake, perms).unwrap();

        let err = clean_cargo_target_with_active_use(&root, &target, &fake, |_| Ok(())).unwrap_err();
        assert!(err.starts_with("cargo-clean-exit-nonzero:"), "{err}");
        assert!(target.join("junk").is_file(), "must not claim reclaim on nonzero");
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn consumer_pin_target_dir_protects_unrelated_sentinel() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "disksage-cargo-pin-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let project = root.join("proj");
        let target = project.join("target");
        let shared = root.join("shared-target");
        let argv_log = root.join("fake-cargo.argv");
        fs::create_dir_all(&target).unwrap();
        fs::create_dir_all(&shared).unwrap();
        fs::write(project.join("Cargo.toml"), "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n").unwrap();
        fs::write(target.join("artifact"), "delete-me").unwrap();
        fs::write(shared.join("SENTINEL"), "must-survive").unwrap();
        let target_canon = fs::canonicalize(&target).unwrap();

        // Mock cargo: record argv and mutate only the identity-verified detached target.
        let fake = project.join("fake-cargo");
        let script = format!(
            "#!/bin/sh\n\
ORIGINAL_TARGET='{original}'\n\
ARGV_LOG='{log}'\n\
printf '%s\\0' \"$@\" > \"$ARGV_LOG\"\n\
target=\"\"\n\
while [ \"$#\" -gt 0 ]; do\n\
  if [ \"$1\" = \"--target-dir\" ]; then\n\
    shift\n\
    target=\"$1\"\n\
  fi\n\
  shift || true\n\
done\n\
if [ -n \"$target\" ] && [ \"$target\" != \"$ORIGINAL_TARGET\" ] && [ -d \"$target\" ]; then\n\
  find -H \"$target\" -mindepth 1 -maxdepth 1 -exec rm -rf {{}} +\n\
  rmdir \"$target\"\n\
  exit 0\n\
fi\n\
exit 42\n",
            original = target_canon.display(),
            log = argv_log.display()
        );
        fs::write(&fake, script).unwrap();
        let mut perms = fs::metadata(&fake).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&fake, perms).unwrap();

        let result =
            clean_cargo_target_with_active_use(&project, &target_canon, &fake, |_| Ok(()))
                .expect("clean ok");
        assert!(result.executed);
        assert_eq!(result.status_code, 0);
        assert!(result.observed_reduction_bytes > 0);
        assert_eq!(ledger_reclaim_bytes(&result), result.observed_reduction_bytes);

        let argv = fs::read(&argv_log).unwrap_or_default();
        let argv_txt = String::from_utf8_lossy(&argv);
        assert!(
            argv_txt.contains("--target-dir"),
            "consumer must pass --target-dir; argv={argv_txt:?}"
        );
        assert!(
            argv_txt.contains(".disksage-cargo-clean-") && !argv_txt.contains(target_canon.to_string_lossy().as_ref()),
            "consumer must pass the identity-verified detached target; argv={argv_txt:?}"
        );
        assert!(
            shared.join("SENTINEL").is_file(),
            "unrelated sentinel must survive pinned --target-dir clean"
        );
        assert!(
            !target_canon.join("artifact").exists(),
            "measured target contents should be removed"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn equal_project_root_target_is_rejected() {
        let root = std::env::temp_dir().join(format!(
            "disksage-cargo-eqroot-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        let fake = root.join("fake-cargo");
        fs::write(&fake, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&fake).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&fake, perms).unwrap();
        }
        let err = clean_cargo_target_with(&root, &root, &fake).unwrap_err();
        assert_eq!(err, "cargo-target-dir-outside-project");
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_escape_target_is_rejected() {
        let root = std::env::temp_dir().join(format!(
            "disksage-cargo-symlink-escape-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let project = root.join("proj");
        let outside = root.join("outside-shared");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(
            project.join("Cargo.toml"),
            "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        fs::write(outside.join("SENTINEL"), "must-survive").unwrap();
        std::os::unix::fs::symlink(&outside, project.join("target")).unwrap();

        let fake = project.join("fake-cargo");
        fs::write(&fake, "#!/bin/sh\nexit 0\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&fake).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&fake, perms).unwrap();

        let err = clean_cargo_target_with(&project, &project.join("target"), &fake).unwrap_err();
        assert_eq!(err, "cargo-target-dir-symlink");
        assert!(
            outside.join("SENTINEL").is_file(),
            "symlink-escaped shared target must not be cleaned"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn dangling_target_symlink_is_rejected() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let root = std::env::temp_dir().join(format!(
            "disksage-cargo-dangling-target-{}", std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n").unwrap();
        symlink(root.join("missing-target"), root.join("target")).unwrap();
        let fake = root.join("fake-cargo");
        fs::write(&fake, "#!/bin/sh\nexit 0\n").unwrap();
        let mut permissions = fs::metadata(&fake).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&fake, permissions).unwrap();

        let error = clean_cargo_target_with_active_use(
            &root, &root.join("target"), &fake, |_| Ok(())
        ).unwrap_err();
        assert_eq!(error, "cargo-target-dir-symlink");
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn target_path_swap_during_clean_cannot_redirect_deletion() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "disksage-cargo-target-swap-{}", std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let project = root.join("project");
        let target = project.join("target");
        let moved_target = project.join("target-moved");
        let outside = root.join("outside");
        fs::create_dir_all(&target).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(project.join("Cargo.toml"), "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n").unwrap();
        fs::write(target.join("artifact"), "delete-me").unwrap();
        fs::write(outside.join("SENTINEL"), "must-survive").unwrap();

        let fake = project.join("fake-cargo");
        let script = format!(
            "#!/bin/sh\n\
target_arg=''\n\
while [ \"$#\" -gt 0 ]; do\n\
  if [ \"$1\" = '--target-dir' ]; then shift; target_arg=\"$1\"; fi\n\
  shift || true\n\
done\n\
mv '{target}' '{moved}'\n\
ln -s '{outside}' '{target}'\n\
find -H \"$target_arg\" -mindepth 1 -maxdepth 1 -exec rm -rf {{}} +\n\
rmdir \"$target_arg\"\n",
            target = target.display(), moved = moved_target.display(), outside = outside.display(),
        );
        fs::write(&fake, script).unwrap();
        let mut permissions = fs::metadata(&fake).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&fake, permissions).unwrap();

        let result = clean_cargo_target_with_active_use(&project, &target, &fake, |_| Ok(()))
            .expect("handle-bound cleanup should succeed");
        assert!(result.observed_reduction_bytes > 0);
        assert!(outside.join("SENTINEL").is_file());
        assert!(fs::symlink_metadata(&target).unwrap().file_type().is_symlink());
        assert!(!moved_target.join("artifact").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn real_cargo_cleans_identity_verified_detached_target() {
        let root = std::env::temp_dir().join(format!(
            "disksage-cargo-real-handle-{}", std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let target = root.join("target");
        fs::create_dir_all(&target).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n").unwrap();
        fs::write(root.join("src/lib.rs"), "pub fn example() {}\n").unwrap();
        fs::write(target.join("artifact"), "delete-me").unwrap();
        let cargo = fs::canonicalize(env!("CARGO")).unwrap();

        let result = clean_cargo_target_with_active_use(&root, &target, &cargo, |_| Ok(()))
            .expect("real cargo should accept the detached target");
        assert!(result.executed);
        assert!(result.observed_reduction_bytes > 0);
        assert!(!target.join("artifact").exists());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn lsof_exit_127_is_fail_closed_not_empty_match() {
        let err = classify_target_lsof_result(127, "", "").unwrap_err();
        assert_eq!(
            err,
            "cargo-target-active-use-probe-failed:lsof-exit-status:127"
        );
        assert!(classify_target_lsof_result(1, "", "").is_ok());
        assert_eq!(
            classify_target_lsof_result(0, "COMMAND PID\nrustc 1\n", "")
                .unwrap_err(),
            "cargo-target-active-holders-present"
        );
    }

    #[test]
    fn lsof_exit_1_with_holder_stdout_is_active_holders_not_empty_match() {
        // Mirrors independent review active-holder-reproduction.json:
        // Python PID held a synthetic file; lsof exit=1, stderr empty, stdout listed
        // the holder. Original cargo|rustc grep would miss it and WOULD_PROCEED.
        let sample = "COMMAND     PID       USER   FD   TYPE DEVICE SIZE/OFF      NODE NAME\n\
python3.1 21407 seonghobae    3u   REG   1,16       23 664756961 /tmp/co-pr461-holder/target/synthetic-artifact\n";
        assert_eq!(
            classify_target_lsof_result(1, sample, "").unwrap_err(),
            "cargo-target-active-holders-present"
        );
    }

    #[test]
    fn lsof_exit_0_with_warning_stderr_is_fail_closed() {
        // report-a118 warning_zero: exit0 + empty stdout + warning stderr invoked
        // fake cargo under a118b237. Must refuse before clean.
        let err = classify_target_lsof_result(
            0,
            "",
            "lsof: WARNING: can't stat() fuse.portal file system /run/user/0/doc\n",
        )
        .unwrap_err();
        assert_eq!(
            err,
            "cargo-target-active-use-probe-failed:lsof-stderr-nonempty:exit:0"
        );
    }

    #[cfg(unix)]
    #[test]
    fn lsof_timeout_terminates_the_probe() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "disksage-cargo-lsof-timeout-{}", std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("target")).unwrap();
        let pid_file = root.join("lsof.pid");
        let fake_lsof = root.join("fake-lsof");
        fs::write(&fake_lsof, format!(
            "#!/bin/sh\nprintf '%s' \"$$\" > '{}'\nexec sleep 30\n", pid_file.display()
        )).unwrap();
        let mut permissions = fs::metadata(&fake_lsof).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&fake_lsof, permissions).unwrap();

        let error = run_target_lsof(
            &fake_lsof, &root.join("target"), Duration::from_millis(100)
        ).unwrap_err();
        assert_eq!(error, "cargo-target-lsof-timeout");
        let pid: libc::pid_t = fs::read_to_string(&pid_file).unwrap().parse().unwrap();
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1, "timed-out lsof survived");
        assert_eq!(std::io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn warning_stderr_probe_blocks_clean_without_invoking_cargo() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "disksage-cargo-lsof-warn-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let target = root.join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("keep-me"), "x").unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        let ran = root.join("cargo-ran");
        let fake = root.join("fake-cargo");
        fs::write(
            &fake,
            format!("#!/bin/sh\ntouch '{}'\nexit 0\n", ran.display()),
        )
        .unwrap();
        let mut perms = fs::metadata(&fake).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&fake, perms).unwrap();

        fn probe_warning(_path: &Path) -> Result<(), String> {
            classify_target_lsof_result(0, "", "lsof: WARNING: incomplete\n")
        }

        let err =
            clean_cargo_target_with_active_use(&root, &target, &fake, probe_warning).unwrap_err();
        assert_eq!(
            err,
            "cargo-target-active-use-probe-failed:lsof-stderr-nonempty:exit:0"
        );
        assert!(!ran.exists(), "cargo must not run on warning stderr");
        assert!(target.join("keep-me").is_file());
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn active_holder_probe_failure_blocks_clean_without_invoking_cargo() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "disksage-cargo-lsof127-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let target = root.join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("keep-me"), "x").unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        let ran = root.join("cargo-ran");
        let fake = root.join("fake-cargo");
        fs::write(
            &fake,
            format!("#!/bin/sh\ntouch '{}'\nexit 0\n", ran.display()),
        )
        .unwrap();
        let mut perms = fs::metadata(&fake).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&fake, perms).unwrap();

        fn probe_lsof_127(_path: &Path) -> Result<(), String> {
            classify_target_lsof_result(127, "", "")
        }

        let err =
            clean_cargo_target_with_active_use(&root, &target, &fake, probe_lsof_127).unwrap_err();
        assert_eq!(
            err,
            "cargo-target-active-use-probe-failed:lsof-exit-status:127"
        );
        assert!(!ran.exists(), "cargo must not run when lsof probe fails");
        assert!(
            target.join("keep-me").is_file(),
            "must not delete when active-use probe fails"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn active_holders_present_blocks_clean_without_invoking_cargo() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!(
            "disksage-cargo-holders-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        let target = root.join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("keep-me"), "x").unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname=\"t\"\nversion=\"0.1.0\"\nedition=\"2021\"\n",
        )
        .unwrap();
        let ran = root.join("cargo-ran");
        let fake = root.join("fake-cargo");
        fs::write(
            &fake,
            format!("#!/bin/sh\ntouch '{}'\nexit 0\n", ran.display()),
        )
        .unwrap();
        let mut perms = fs::metadata(&fake).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&fake, perms).unwrap();

        fn probe_holders(_path: &Path) -> Result<(), String> {
            Err("cargo-target-active-holders-present".into())
        }

        let err =
            clean_cargo_target_with_active_use(&root, &target, &fake, probe_holders).unwrap_err();
        assert_eq!(err, "cargo-target-active-holders-present");
        assert!(!ran.exists(), "cargo must not run when holders are present");
        assert!(target.join("keep-me").is_file());
        let _ = fs::remove_dir_all(&root);
    }
}
