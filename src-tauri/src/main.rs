// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(all(target_os = "macos", not(coverage)))]
#[link(name = "proc")]
extern "C" {
    fn proc_pidpath(pid: i32, buffer: *mut std::ffi::c_void, buffersize: u32) -> i32;
}

#[cfg(all(target_os = "macos", not(coverage)))]
fn native_eviction_helper_parent_is_current_executable() -> bool {
    use std::ffi::{CStr, OsStr};
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
    if length <= 0 {
        return false;
    }

    let parent_path = unsafe { CStr::from_ptr(buffer.as_ptr().cast()) };
    let parent_path = PathBuf::from(OsStr::from_bytes(parent_path.to_bytes()));
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

// coverage 빌드에서는 GUI 부트스트랩을 컴파일하지 않는다 (#[coverage(off)]는 아직 unstable)
#[cfg(not(coverage))]
fn main() {
    #[cfg(target_os = "macos")]
    if std::env::var_os("DISKSAGE_NATIVE_ICLOUD_EVICTION_HELPER").is_some()
        && !native_eviction_helper_parent_is_current_executable()
    {
        eprintln!("icloud-native-eviction-helper-parent-untrusted");
        std::process::exit(2);
    }

    if disksage_lib::cloud_local_eviction::run_native_icloud_eviction_helper_if_requested() {
        return;
    }
    disksage_lib::run()
}

#[cfg(coverage)]
fn main() {}

#[cfg(all(coverage, test))]
mod coverage_tests {
    // 커버리지 빌드의 no-op main도 라인으로 집계되므로 실행해 준다
    #[test]
    fn noop_main_runs() {
        super::main();
    }
}
