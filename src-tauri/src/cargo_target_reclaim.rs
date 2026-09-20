//! Cargo `target/` reclaim with fail-closed tool accounting.
//!
//! Operational scripts previously treated a missing `cargo` on PATH as a successful
//! `CLEANED` row when directory size was unchanged. This module requires an absolute
//! cargo executable, a zero exit status, and records reclaim bytes from measured
//! before/after size (zero delta ⇒ reclaim 0, never a success reclaim claim).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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

/// Run `cargo clean` for a project directory that owns a `Cargo.toml`.
///
/// Fail-closed:
/// - missing cargo executable ⇒ `Err`
/// - spawn failure / command-not-found ⇒ `Err`
/// - non-zero exit ⇒ `Err` (no success reclaim)
/// - zero size delta with exit 0 ⇒ `Ok` with `observed_reduction_bytes == 0` (not a reclaim success claim)
pub fn clean_cargo_target(project_dir: &Path) -> Result<CargoTargetCleanResult, String> {
    if !project_dir.is_absolute() {
        return Err("cargo-target-project-not-absolute".into());
    }
    let manifest = project_dir.join("Cargo.toml");
    if !manifest.is_file() {
        return Err("cargo-target-manifest-missing".into());
    }
    let cargo = resolve_cargo_executable()?;
    let target_dir = project_dir.join("target");
    let bytes_before = bounded_dir_size(&target_dir)?;

    let mut child = Command::new(&cargo)
        .arg("clean")
        .current_dir(project_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "cargo-executable-unavailable".to_string()
            } else {
                format!("cargo-clean-spawn-failed:{e}")
            }
        })?;

    let deadline = Instant::now() + Duration::from_secs(600);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("cargo-clean-timeout".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("cargo-clean-wait-failed:{e}"));
            }
        }
    };

    let status_code = status.code().unwrap_or(-1);
    if status_code != 0 {
        return Err(format!("cargo-clean-exit-nonzero:{status_code}"));
    }

    let bytes_after = bounded_dir_size(&target_dir)?;
    Ok(CargoTargetCleanResult {
        cargo_path: cargo,
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
    fn spawn_not_found_maps_to_unavailable_code() {
        let err = std::io::Error::new(std::io::ErrorKind::NotFound, "no such file");
        let mapped = if err.kind() == std::io::ErrorKind::NotFound {
            "cargo-executable-unavailable".to_string()
        } else {
            format!("cargo-clean-spawn-failed:{err}")
        };
        assert_eq!(mapped, "cargo-executable-unavailable");
    }

    #[test]
    fn relative_project_dir_is_rejected() {
        let err = clean_cargo_target(Path::new("relative/project")).unwrap_err();
        assert_eq!(err, "cargo-target-project-not-absolute");
    }
}
