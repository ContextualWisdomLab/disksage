//! Public iCloud-local-eviction boundary.
//!
//! The implementation module contains provider-neutral observation helpers needed by the iCloud
//! workflow. This facade keeps the exported mutation contract provider-specific and fail-closed.

pub use crate::cloud_local_eviction_impl::*;

use crate::cloud::{CloudProvider, CloudRoot};
use std::path::Path;

#[cfg(all(target_os = "macos", not(coverage)))]
#[link(name = "proc")]
extern "C" {
    fn proc_pidpath(pid: i32, buffer: *mut std::ffi::c_void, buffersize: u32) -> i32;
}

#[cfg(all(target_os = "macos", not(coverage)))]
fn native_eviction_helper_parent_is_current_executable() -> bool {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    use std::path::PathBuf;

    const PROC_PIDPATHINFO_MAXSIZE: usize = 4096;

    let parent_pid = unsafe { libc::getppid() };
    if parent_pid <= 1 {
        return false;
    }

    let mut buffer = [0u8; PROC_PIDPATHINFO_MAXSIZE];
    let length = unsafe {
        proc_pidpath(
            parent_pid,
            buffer.as_mut_ptr().cast(),
            u32::try_from(buffer.len()).unwrap_or(u32::MAX),
        )
    };
    let Ok(length) = usize::try_from(length) else {
        return false;
    };
    if length == 0 || length > buffer.len() {
        return false;
    }

    let parent_path_bytes = buffer[..length]
        .strip_suffix(&[0])
        .unwrap_or(&buffer[..length]);
    if parent_path_bytes.is_empty() || parent_path_bytes.contains(&0) {
        return false;
    }
    let parent_path = PathBuf::from(OsStr::from_bytes(parent_path_bytes));
    let Some(current_executable) = std::env::current_exe()
        .ok()
        .and_then(|path| std::fs::canonicalize(path).ok())
    else {
        return false;
    };
    let Some(parent_executable) = std::fs::canonicalize(parent_path).ok() else {
        return false;
    };
    parent_executable == current_executable
}

/// Enter the private native-eviction helper only when its parent process is the same DiskSage
/// executable. This validation lives in the public library boundary so every binary that exposes
/// helper startup receives the same authority check instead of relying on per-binary bootstrap.
pub fn run_native_icloud_eviction_helper_if_requested() -> bool {
    #[cfg(all(target_os = "macos", not(coverage)))]
    if std::env::var_os("DISKSAGE_NATIVE_ICLOUD_EVICTION_HELPER").is_some()
        && !native_eviction_helper_parent_is_current_executable()
    {
        eprintln!("icloud-native-eviction-helper-parent-untrusted");
        std::process::exit(2);
    }

    crate::cloud_local_eviction_impl::run_native_icloud_eviction_helper_if_requested()
}

fn require_icloud_provider(root: &CloudRoot) -> Result<(), String> {
    if root.provider != CloudProvider::Icloud {
        return Err("icloud-local-eviction-provider-mismatch".into());
    }
    Ok(())
}

/// Build a read-only local-copy eviction plan only for an iCloud-owned root.
#[cfg(not(coverage))]
pub fn plan_icloud_local_eviction(
    root: &CloudRoot,
    path: &Path,
    observed_at_ms: u64,
) -> Result<IcloudLocalEvictionPlan, String> {
    require_icloud_provider(root)?;
    crate::cloud_local_eviction_impl::plan_icloud_local_eviction(root, path, observed_at_ms)
}

/// Bind approval only to a provider-correct iCloud plan.
pub fn approve_icloud_local_eviction(
    plan: &IcloudLocalEvictionPlan,
    approved_plan_fingerprint: &str,
    approved_at_ms: u64,
    approved_by: &str,
    rationale: &str,
) -> Result<IcloudLocalEvictionApproval, String> {
    if plan.provider != CloudProvider::Icloud {
        return Err("icloud-local-eviction-provider-mismatch".into());
    }
    crate::cloud_local_eviction_impl::approve_icloud_local_eviction(
        plan,
        approved_plan_fingerprint,
        approved_at_ms,
        approved_by,
        rationale,
    )
}

/// Execute only when both the live root and the reviewed plan are explicitly iCloud-owned.
#[cfg(not(coverage))]
pub fn execute_icloud_local_eviction(
    root: &CloudRoot,
    approved_plan: &IcloudLocalEvictionPlan,
    approval: &IcloudLocalEvictionApproval,
    confirmation_plan_fingerprint: &str,
    requested_at_ms: u64,
) -> Result<IcloudLocalEvictionResult, String> {
    require_icloud_provider(root)?;
    if approved_plan.provider != CloudProvider::Icloud {
        return Err("icloud-local-eviction-provider-mismatch".into());
    }
    crate::cloud_local_eviction_impl::execute_icloud_local_eviction(
        root,
        approved_plan,
        approval,
        confirmation_plan_fingerprint,
        requested_at_ms,
    )
}
