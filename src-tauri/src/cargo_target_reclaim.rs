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

use std::io::Read;
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
    // target may not exist yet; canonicalize parent + join name when absent
    let target_canon = if target_dir.exists() {
        std::fs::canonicalize(target_dir)
            .map_err(|e| format!("cargo-target-dir-canonicalize-failed:{e}"))?
    } else {
        let parent = target_dir
            .parent()
            .ok_or_else(|| "cargo-target-dir-parent-missing".to_string())?;
        let parent_canon = std::fs::canonicalize(parent)
            .map_err(|e| format!("cargo-target-dir-parent-canonicalize-failed:{e}"))?;
        let name = target_dir
            .file_name()
            .ok_or_else(|| "cargo-target-dir-name-missing".to_string())?;
        parent_canon.join(name)
    };
    if !is_strict_canonical_child(&project_canon, &target_canon) {
        return Err("cargo-target-dir-outside-project".into());
    }
    Ok(target_canon)
}

/// Run `cargo clean` for a project directory that owns a `Cargo.toml`.
///
/// Always passes `--target-dir <project>/target` so measurement and deletion match.
///
/// Fail-closed:
/// - missing cargo executable ⇒ `Err`
/// - spawn failure / command-not-found ⇒ `Err`
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
    ensure_absolute_project(project_dir)?;
    if !cargo.is_absolute() {
        return Err("cargo-executable-not-absolute".into());
    }
    if !executable_file(cargo) {
        return Err("cargo-executable-unavailable".into());
    }
    let target_dir = resolve_measured_target_dir(project_dir, target_dir)?;
    let bytes_before = bounded_dir_size(&target_dir)?;

    let mut child = Command::new(cargo)
        .arg("clean")
        .arg("--target-dir")
        .arg(&target_dir)
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

    // Drain stderr on a helper thread so a verbose failure cannot fill the pipe and block.
    let stderr_handle = child.stderr.take().map(|mut stderr| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            const MAX: usize = 64 * 1024;
            loop {
                match stderr.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        if buf.len() < MAX {
                            let take = (MAX - buf.len()).min(n);
                            buf.extend_from_slice(&chunk[..take]);
                        }
                    }
                    Err(_) => break,
                }
            }
            buf
        })
    });

    let deadline = Instant::now() + Duration::from_secs(600);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                if let Some(h) = stderr_handle {
                    let _ = h.join();
                }
                return Err("cargo-clean-timeout".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                if let Some(h) = stderr_handle {
                    let _ = h.join();
                }
                return Err(format!("cargo-clean-wait-failed:{e}"));
            }
        }
    };

    if let Some(h) = stderr_handle {
        let _ = h.join();
    }

    let status_code = status.code().unwrap_or(-1);
    if status_code != 0 {
        return Err(format!("cargo-clean-exit-nonzero:{status_code}"));
    }

    let bytes_after = bounded_dir_size(&target_dir)?;
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
        let err = clean_cargo_target_with(&root, &root.join("target"), &missing).unwrap_err();
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

        let err = clean_cargo_target_with(&root, &target, &fake).unwrap_err();
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

        // Mock cargo: record argv; mutate ONLY the allowlisted absolute target embedded below.
        // Never delete via unset env / root-glob fallback.
        let fake = project.join("fake-cargo");
        let script = format!(
            "#!/bin/sh\n\
ALLOWED_TARGET='{allowed}'\n\
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
if [ -n \"$target\" ] && [ \"$target\" = \"$ALLOWED_TARGET\" ]; then\n\
  find \"$ALLOWED_TARGET\" -mindepth 1 -maxdepth 1 -exec rm -rf {{}} +\n\
  exit 0\n\
fi\n\
exit 42\n",
            allowed = target_canon.display(),
            log = argv_log.display()
        );
        fs::write(&fake, script).unwrap();
        let mut perms = fs::metadata(&fake).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&fake, perms).unwrap();

        let result = clean_cargo_target_with(&project, &target_canon, &fake).expect("clean ok");
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
            argv_txt.contains(target_canon.to_string_lossy().as_ref()),
            "consumer must pass measured target; argv={argv_txt:?}"
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
        assert_eq!(err, "cargo-target-dir-outside-project");
        assert!(
            outside.join("SENTINEL").is_file(),
            "symlink-escaped shared target must not be cleaned"
        );
        let _ = fs::remove_dir_all(&root);
    }
}
