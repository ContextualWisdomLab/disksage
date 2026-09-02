#![cfg(windows)]

use std::ffi::c_void;
use std::io;
use std::mem::size_of;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::process::{Child, Command};
use std::ptr::null_mut;

type Handle = *mut c_void;
type Bool = i32;

const CREATE_SUSPENDED: u32 = 0x0000_0004;
const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS: i32 = 9;
const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;
const TH32CS_SNAPTHREAD: u32 = 0x0000_0004;
const THREAD_SUSPEND_RESUME: u32 = 0x0002;
const ERROR_NO_MORE_FILES: u32 = 18;

#[repr(C)]
#[derive(Default)]
struct JobObjectBasicLimitInformation {
    per_process_user_time_limit: i64,
    per_job_user_time_limit: i64,
    limit_flags: u32,
    minimum_working_set_size: usize,
    maximum_working_set_size: usize,
    active_process_limit: u32,
    affinity: usize,
    priority_class: u32,
    scheduling_class: u32,
}

#[repr(C)]
#[derive(Default)]
struct IoCounters {
    read_operation_count: u64,
    write_operation_count: u64,
    other_operation_count: u64,
    read_transfer_count: u64,
    write_transfer_count: u64,
    other_transfer_count: u64,
}

#[repr(C)]
#[derive(Default)]
struct JobObjectExtendedLimitInformation {
    basic_limit_information: JobObjectBasicLimitInformation,
    io_info: IoCounters,
    process_memory_limit: usize,
    job_memory_limit: usize,
    peak_process_memory_used: usize,
    peak_job_memory_used: usize,
}

#[repr(C)]
#[derive(Default)]
struct ThreadEntry32 {
    dw_size: u32,
    cnt_usage: u32,
    thread_id: u32,
    owner_process_id: u32,
    base_priority: i32,
    delta_priority: i32,
    flags: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateJobObjectW(job_attributes: *const c_void, name: *const u16) -> Handle;
    fn SetInformationJobObject(
        job: Handle,
        information_class: i32,
        information: *const c_void,
        information_length: u32,
    ) -> Bool;
    fn AssignProcessToJobObject(job: Handle, process: Handle) -> Bool;
    fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> Handle;
    fn Thread32First(snapshot: Handle, entry: *mut ThreadEntry32) -> Bool;
    fn Thread32Next(snapshot: Handle, entry: *mut ThreadEntry32) -> Bool;
    fn OpenThread(desired_access: u32, inherit_handle: Bool, thread_id: u32) -> Handle;
    fn ResumeThread(thread: Handle) -> u32;
    fn GetLastError() -> u32;
    fn CloseHandle(object: Handle) -> Bool;
}

fn invalid_handle_value() -> Handle {
    -1_isize as Handle
}

fn process_thread_ids(process_id: u32) -> io::Result<Vec<u32>> {
    // SAFETY: TH32CS_SNAPTHREAD ignores the process-id argument and returns an owned snapshot
    // handle. The handle is checked for INVALID_HANDLE_VALUE before use and closed on every path.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == invalid_handle_value() {
        return Err(io::Error::last_os_error());
    }

    let mut entry = ThreadEntry32 {
        dw_size: u32::try_from(size_of::<ThreadEntry32>())
            .expect("THREADENTRY32 size fits in u32"),
        ..ThreadEntry32::default()
    };
    // SAFETY: `snapshot` is a live thread snapshot and `entry` points to writable storage whose
    // `dw_size` field matches the Windows THREADENTRY32 contract.
    let first = unsafe { Thread32First(snapshot, &mut entry) };
    if first == 0 {
        let error = io::Error::last_os_error();
        // SAFETY: this function exclusively owns the snapshot handle.
        let _ = unsafe { CloseHandle(snapshot) };
        return Err(error);
    }

    let mut thread_ids = Vec::new();
    loop {
        if entry.owner_process_id == process_id {
            thread_ids.push(entry.thread_id);
        }
        // SAFETY: arguments have the same validity as Thread32First above. A false return with
        // ERROR_NO_MORE_FILES is the documented end-of-snapshot condition.
        if unsafe { Thread32Next(snapshot, &mut entry) } == 0 {
            // SAFETY: GetLastError has no preconditions and reads the calling thread's last error.
            let last_error = unsafe { GetLastError() };
            if last_error != ERROR_NO_MORE_FILES {
                let error = io::Error::from_raw_os_error(last_error as i32);
                // SAFETY: this function exclusively owns the snapshot handle.
                let _ = unsafe { CloseHandle(snapshot) };
                return Err(error);
            }
            break;
        }
    }
    // SAFETY: this function exclusively owns the snapshot handle.
    let _ = unsafe { CloseHandle(snapshot) };

    if thread_ids.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "suspended child has no discoverable thread",
        ));
    }
    Ok(thread_ids)
}

fn resume_suspended_process(process_id: u32) -> io::Result<()> {
    let thread_ids = process_thread_ids(process_id)?;
    for thread_id in thread_ids {
        // SAFETY: the numeric thread ID came from a current system snapshot. The returned handle is
        // checked before use and closed below without being transferred.
        let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, thread_id) };
        if thread.is_null() {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `thread` is a live handle opened with THREAD_SUSPEND_RESUME access.
        let previous_suspend_count = unsafe { ResumeThread(thread) };
        // SAFETY: this function exclusively owns the thread handle.
        let _ = unsafe { CloseHandle(thread) };
        if previous_suspend_count == u32::MAX {
            return Err(io::Error::last_os_error());
        }
        if previous_suspend_count != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "child thread was not suspended exactly once before Job Object attachment",
            ));
        }
    }
    Ok(())
}

/// Owns the Windows Job Object that contains one subprocess and all descendants it creates.
///
/// `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` makes the guard a fail-closed lifetime boundary: dropping
/// it terminates every process still attached to the job, including descendants that inherited the
/// Podman stdout/stderr handles and would otherwise keep DiskSage reader threads alive after timeout.
pub(crate) struct ProcessTreeGuard {
    job: Handle,
}

impl ProcessTreeGuard {
    /// Configures a command so its primary thread cannot execute before process-tree control exists.
    ///
    /// Callers must pair this with `attach_and_resume` immediately after `Command::spawn`. Creating
    /// the process suspended closes the otherwise unavoidable spawn-to-Job-assignment window in
    /// which a hostile or fast child could create descendants outside the Job Object.
    pub(crate) fn prepare_suspended(command: &mut Command) {
        command.creation_flags(CREATE_SUSPENDED);
    }

    fn attach_job(child: &Child) -> io::Result<Self> {
        // SAFETY: null security attributes/name request an unnamed job with default ACLs. The
        // returned HANDLE is checked before use and owned exclusively by ProcessTreeGuard.
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return Err(io::Error::last_os_error());
        }

        let mut limits = JobObjectExtendedLimitInformation::default();
        limits.basic_limit_information.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let information_length = u32::try_from(size_of::<JobObjectExtendedLimitInformation>())
            .expect("Windows Job Object information size fits in u32");
        // SAFETY: `limits` has the layout required by JobObjectExtendedLimitInformation and stays
        // alive for the call. `job` is a valid owned handle from CreateJobObjectW.
        let configured = unsafe {
            SetInformationJobObject(
                job,
                JOB_OBJECT_EXTENDED_LIMIT_INFORMATION_CLASS,
                (&limits as *const JobObjectExtendedLimitInformation).cast(),
                information_length,
            )
        };
        if configured == 0 {
            let error = io::Error::last_os_error();
            // SAFETY: `job` is owned by this function and has not been transferred.
            let _ = unsafe { CloseHandle(job) };
            return Err(error);
        }

        let process = child.as_raw_handle().cast::<c_void>();
        // SAFETY: `process` is the live process handle owned by `child`; assigning it does not
        // transfer ownership. `job` remains owned by the returned guard.
        let assigned = unsafe { AssignProcessToJobObject(job, process) };
        if assigned == 0 {
            let error = io::Error::last_os_error();
            // SAFETY: `job` is still exclusively owned here.
            let _ = unsafe { CloseHandle(job) };
            return Err(error);
        }

        Ok(Self { job })
    }

    /// Attaches a CREATE_SUSPENDED child to a kill-on-close Job Object, then starts its user code.
    ///
    /// The method fails closed if Job creation, assignment, thread discovery, or resumption fails.
    /// On a resumption failure the guard is dropped before the error is returned, terminating the
    /// still-suspended process instead of allowing an uncontrolled subprocess to survive.
    pub(crate) fn attach_and_resume(child: &Child) -> io::Result<Self> {
        let guard = Self::attach_job(child)?;
        if let Err(error) = resume_suspended_process(child.id()) {
            drop(guard);
            return Err(error);
        }
        Ok(guard)
    }

    #[cfg(test)]
    pub(crate) fn attach(child: &Child) -> io::Result<Self> {
        Self::attach_job(child)
    }
}

impl Drop for ProcessTreeGuard {
    fn drop(&mut self) {
        if self.job.is_null() {
            return;
        }
        // SAFETY: this guard exclusively owns the Job Object handle. KILL_ON_JOB_CLOSE makes this
        // close operation the process-tree termination boundary for any members still running.
        let _ = unsafe { CloseHandle(self.job) };
        self.job = null_mut();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, Instant};

    const PID_FILE_ENV: &str = "DISKSAGE_WINDOWS_TREE_PID_FILE";
    const START_FILE_ENV: &str = "DISKSAGE_WINDOWS_TREE_START_FILE";
    const GRANDCHILD_ENV: &str = "DISKSAGE_WINDOWS_TREE_GRANDCHILD";

    fn wait_for_path(path: &std::path::Path, timeout: Duration) -> bool {
        let started = Instant::now();
        while started.elapsed() < timeout {
            if path.exists() {
                return true;
            }
            thread::sleep(Duration::from_millis(25));
        }
        path.exists()
    }

    fn process_is_running(process_id: u32) -> bool {
        let filter = format!("PID eq {process_id}");
        let Ok(output) = Command::new("tasklist")
            .args(["/FI", &filter, "/FO", "CSV", "/NH"])
            .output()
        else {
            return true;
        };
        if !output.status.success() {
            return true;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        text.lines().any(|line| {
            line.split(',')
                .nth(1)
                .map(str::trim)
                .map(|field| field.trim_matches('"') == process_id.to_string())
                .unwrap_or(false)
        })
    }

    fn force_kill(process_id: u32) {
        let _ = Command::new("taskkill")
            .args(["/PID", &process_id.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    #[test]
    fn windows_process_tree_grandchild_fixture() {
        if std::env::var_os(GRANDCHILD_ENV).is_none() {
            return;
        }
        thread::sleep(Duration::from_secs(30));
    }

    #[test]
    fn windows_process_tree_parent_fixture() {
        let Some(pid_file) = std::env::var_os(PID_FILE_ENV).map(std::path::PathBuf::from) else {
            return;
        };
        let Some(start_file) = std::env::var_os(START_FILE_ENV).map(std::path::PathBuf::from) else {
            return;
        };
        assert!(
            wait_for_path(&start_file, Duration::from_secs(5)),
            "parent fixture was never released after process-tree control attached"
        );
        let executable = std::env::current_exe().expect("test executable path must be available");
        let grandchild = Command::new(executable)
            .arg("windows_process_tree_grandchild_fixture")
            .arg("--nocapture")
            .env(GRANDCHILD_ENV, "1")
            .spawn()
            .expect("grandchild fixture must spawn");
        fs::write(&pid_file, grandchild.id().to_string()).expect("grandchild PID must be recorded");
        thread::sleep(Duration::from_secs(30));
    }

    #[test]
    fn dropping_guard_terminates_descendants_that_inherit_output_handles() {
        let root = std::env::temp_dir().join(format!(
            "disksage-windows-process-tree-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("fixture directory must be creatable");
        let pid_file = root.join("grandchild.pid");
        let start_file = root.join("start");
        let executable = std::env::current_exe().expect("test executable path must be available");
        let mut child = Command::new(executable)
            .arg("windows_process_tree_parent_fixture")
            .arg("--nocapture")
            .env(PID_FILE_ENV, &pid_file)
            .env(START_FILE_ENV, &start_file)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("parent fixture must spawn");

        let guard = ProcessTreeGuard::attach(&child).expect("process-tree control must attach");
        fs::write(&start_file, b"go").expect("parent fixture release marker must be writable");
        assert!(
            wait_for_path(&pid_file, Duration::from_secs(5)),
            "parent fixture did not record its descendant"
        );
        let descendant_id: u32 = fs::read_to_string(&pid_file)
            .expect("grandchild PID must be readable")
            .trim()
            .parse()
            .expect("grandchild PID must be numeric");

        drop(guard);
        let _ = child.kill();
        let _ = child.wait();

        let started = Instant::now();
        while process_is_running(descendant_id) && started.elapsed() < Duration::from_secs(2) {
            thread::sleep(Duration::from_millis(50));
        }
        if process_is_running(descendant_id) {
            force_kill(descendant_id);
            panic!(
                "dropping process-tree control left descendant PID {descendant_id} alive after the parent was terminated"
            );
        }
        let _ = fs::remove_dir_all(root);
    }
}
