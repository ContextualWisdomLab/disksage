//! Identity-preserving Unix child/process-group lifecycle primitives.
//!
//! A private process-group leader must remain waitable until every signal that targets its
//! numeric process-group ID has been sent. Reaping the leader first allows that numeric PID/PGID
//! to be reused, so a later negative-PID signal can target an unrelated process group.
//!
//! This module deliberately owns only Unix subprocess lifecycle mechanics. Domain decisions such
//! as which container, provider, cloud object, or filesystem path may be changed remain with the
//! calling bounded context.

use std::io;
use std::mem::MaybeUninit;
use std::thread;
use std::time::{Duration, Instant};

/// Result of observing one direct child without consuming its wait status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChildObservation {
    /// The selected child has not yet entered a waitable exited state.
    Running,
    /// The selected child exited, but its wait status remains unconsumed and its PID stays pinned.
    ExitedUnreaped,
}

/// Bounded outcome of waiting for a child while preserving its wait status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NoReapWaitOutcome {
    /// The child exited before the deadline and remains waitable for the caller's final reap.
    ExitedUnreaped,
    /// The deadline elapsed while the child was still running and therefore still owns its PID.
    TimedOutStillRunning,
}

/// Observe a direct child with `waitid(..., WNOHANG | WNOWAIT)` without reaping it.
///
/// `WNOWAIT` is the safety property: callers may still target the child's private process group by
/// numeric PGID while the leader remains waitable. After descendant cleanup is complete, the
/// caller must consume the status with `Child::wait()` exactly once.
pub(crate) fn observe_child_without_reap(child_pid: u32) -> io::Result<ChildObservation> {
    let child_id = libc::id_t::try_from(child_pid)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "child PID exceeds id_t"))?;
    let mut info = MaybeUninit::<libc::siginfo_t>::zeroed();
    let result = unsafe {
        libc::waitid(
            libc::P_PID,
            child_id,
            info.as_mut_ptr(),
            libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
        )
    };
    if result == -1 {
        return Err(io::Error::last_os_error());
    }
    let info = unsafe { info.assume_init() };
    match info.si_signo {
        0 => Ok(ChildObservation::Running),
        libc::SIGCHLD => Ok(ChildObservation::ExitedUnreaped),
        other => Err(io::Error::other(format!(
            "waitid returned unexpected signal {other}"
        ))),
    }
}

/// Wait for a direct child to exit or for `timeout` to elapse without consuming its wait status.
///
/// This is the shared polling boundary for private-process-group callers. An exited result still
/// pins the leader PID because `WNOWAIT` leaves it waitable; a timeout result means the leader is
/// still running. Observation errors are returned immediately and never fall back to `try_wait()`,
/// because such a fallback could reap the leader before a later group signal.
pub(crate) fn wait_for_child_without_reap(
    child_pid: u32,
    timeout: Duration,
    poll_interval: Duration,
) -> io::Result<NoReapWaitOutcome> {
    let started = Instant::now();
    loop {
        match observe_child_without_reap(child_pid)? {
            ChildObservation::ExitedUnreaped => return Ok(NoReapWaitOutcome::ExitedUnreaped),
            ChildObservation::Running if started.elapsed() >= timeout => {
                return Ok(NoReapWaitOutcome::TimedOutStillRunning);
            }
            ChildObservation::Running => thread::sleep(poll_interval),
        }
    }
}

/// Send a signal to the private process group whose leader is `child_pid`.
///
/// Callers must invoke this only while the group leader is live or exited-but-unreaped. This
/// function intentionally does not reap the leader; preserving or consuming that identity is the
/// caller's explicit lifecycle decision.
pub(crate) fn signal_private_process_group(child_pid: u32, signal: i32) -> io::Result<()> {
    let leader = libc::pid_t::try_from(child_pid)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "child PID exceeds pid_t"))?;
    if leader <= 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "child PID must be positive",
        ));
    }
    let result = unsafe { libc::kill(-leader, signal) };
    if result == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::os::unix::process::CommandExt;
    use std::process::{Child, Command, Stdio};

    fn spawn_private_group_shell(script: &str, stdout: Stdio) -> Child {
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg(script)
            .stdout(stdout)
            .stderr(Stdio::null());
        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) == -1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        command
            .spawn()
            .expect("spawn private process-group leader")
    }

    fn wait_until_exited_without_reap(child_pid: u32) {
        assert_eq!(
            wait_for_child_without_reap(
                child_pid,
                Duration::from_secs(2),
                Duration::from_millis(10),
            )
            .expect("wait for child without reap"),
            NoReapWaitOutcome::ExitedUnreaped
        );
    }

    #[test]
    fn exited_child_remains_waitable_until_explicit_reap() {
        let mut child = spawn_private_group_shell("exit 7", Stdio::null());
        let child_pid = child.id();
        wait_until_exited_without_reap(child_pid);

        assert_eq!(
            observe_child_without_reap(child_pid).expect("repeat no-reap observation"),
            ChildObservation::ExitedUnreaped
        );
        let status = child.wait().expect("explicit final reap");
        assert_eq!(status.code(), Some(7));
    }

    #[test]
    fn bounded_wait_times_out_without_reaping_or_reusing_leader_identity() {
        let mut child = spawn_private_group_shell("sleep 30", Stdio::null());
        let child_pid = child.id();
        assert_eq!(
            wait_for_child_without_reap(
                child_pid,
                Duration::from_millis(20),
                Duration::from_millis(5),
            )
            .expect("bounded wait for live child"),
            NoReapWaitOutcome::TimedOutStillRunning
        );
        assert_eq!(
            observe_child_without_reap(child_pid).expect("child remains observable after timeout"),
            ChildObservation::Running
        );

        signal_private_process_group(child_pid, libc::SIGKILL)
            .expect("terminate timed-out private process group");
        let status = child.wait().expect("reap timed-out leader after group cleanup");
        assert!(!status.success());
    }

    #[test]
    fn process_group_is_signaled_before_leader_reap_and_pipe_closes() {
        let started = Instant::now();
        let mut child = spawn_private_group_shell("sleep 30 & printf ready", Stdio::piped());
        let child_pid = child.id();
        let mut stdout = child.stdout.take().expect("child stdout pipe");

        wait_until_exited_without_reap(child_pid);
        signal_private_process_group(child_pid, libc::SIGKILL)
            .expect("terminate descendants while leader identity is pinned");
        let status = child.wait().expect("reap group leader after cleanup signal");
        assert!(status.success());

        let mut output = String::new();
        stdout
            .read_to_string(&mut output)
            .expect("drain bounded test output");
        assert_eq!(output, "ready");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn live_group_can_be_terminated_before_reap() {
        let mut child = spawn_private_group_shell("sleep 30", Stdio::null());
        let child_pid = child.id();
        assert_eq!(
            observe_child_without_reap(child_pid).expect("observe running child"),
            ChildObservation::Running
        );

        signal_private_process_group(child_pid, libc::SIGKILL)
            .expect("terminate live private process group");
        let status = child.wait().expect("reap terminated leader");
        assert!(!status.success());
    }
}
