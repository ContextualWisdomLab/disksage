//! Identity-preserving Unix child/process-group lifecycle primitives.
//!
//! A private process-group leader must remain waitable until every signal that targets its
//! numeric process-group ID has been sent. Reaping the leader first allows that numeric PID/PGID
//! to be reused, so a later negative-PID signal can target an unrelated process group.
//!
//! This module deliberately owns only Unix subprocess lifecycle mechanics. Domain decisions such
//! as which container, provider, cloud object, or filesystem path may be changed remain with the
//! calling bounded context.

use std::io::{self, Read};
use std::mem::MaybeUninit;
use std::os::fd::AsRawFd;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
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

/// Shared cancellation token for readers that must not wait forever for inherited pipe writers.
///
/// Callers cancel when the direct child is being settled or when lifecycle observation has failed
/// closed. Readers perform a bounded final drain so buffered command output is retained without
/// allowing a continuously writing descendant to keep the capture join alive indefinitely.
#[derive(Debug, Clone)]
pub(crate) struct PipeReaderCancellation {
    cancelled: Arc<AtomicBool>,
}

impl PipeReaderCancellation {
    pub(crate) fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// Spawn a bounded Unix pipe reader that can stop after child settlement without requiring EOF.
///
/// Child stdout/stderr descriptors can remain open in descendants that escape the caller's
/// private process group. A blocking `read` followed by an unbounded thread join would then extend
/// a completed CLI operation indefinitely. This helper preserves existing descriptor flags,
/// enables `O_NONBLOCK`, retries `Interrupted`, and waits through `WouldBlock` while the child
/// remains active. After observing cancellation it drains at most the uncaptured output allowance
/// plus one truncation byte, then exits even if a descendant keeps the pipe continuously readable.
/// Output remains capped and reports whether bytes beyond `max_capture_bytes` were observed.
pub(crate) fn spawn_bounded_cancellable_pipe_reader<R>(
    mut reader: R,
    max_capture_bytes: usize,
    poll_interval: Duration,
    cancellation: PipeReaderCancellation,
) -> io::Result<thread::JoinHandle<io::Result<(Vec<u8>, bool)>>>
where
    R: Read + AsRawFd + Send + 'static,
{
    let fd = reader.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags == -1 {
        return Err(io::Error::last_os_error());
    }
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1 {
        return Err(io::Error::last_os_error());
    }

    Ok(thread::spawn(move || {
        let mut buffer = [0u8; 65_536];
        let mut captured = Vec::new();
        let mut truncated = false;
        let mut final_drain_bytes_remaining = None;
        loop {
            if cancellation.is_cancelled() && final_drain_bytes_remaining.is_none() {
                final_drain_bytes_remaining = Some(
                    max_capture_bytes
                        .saturating_sub(captured.len())
                        .saturating_add(1),
                );
            }
            if final_drain_bytes_remaining == Some(0) {
                break;
            }
            let read_limit = final_drain_bytes_remaining
                .unwrap_or(buffer.len())
                .min(buffer.len());
            match reader.read(&mut buffer[..read_limit]) {
                Ok(0) => break,
                Ok(read) => {
                    let room = max_capture_bytes.saturating_sub(captured.len());
                    let retained = read.min(room);
                    captured.extend_from_slice(&buffer[..retained]);
                    if retained < read {
                        truncated = true;
                    }
                    if let Some(bytes_remaining) = final_drain_bytes_remaining.as_mut() {
                        *bytes_remaining = bytes_remaining.saturating_sub(read);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                    if cancellation.is_cancelled() {
                        break;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if cancellation.is_cancelled() {
                        break;
                    }
                    thread::sleep(poll_interval);
                }
                Err(error) => return Err(error),
            }
        }
        Ok((captured, truncated))
    }))
}

/// Retry only operations interrupted before completion; every other error remains fail closed.
fn retry_interrupted<T>(mut operation: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    loop {
        match operation() {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => return result,
        }
    }
}

/// Observe a direct child with `waitid(..., WNOHANG | WNOWAIT)` without reaping it.
///
/// `WNOWAIT` is the safety property: callers may still target the child's private process group by
/// numeric PGID while the leader remains waitable. With `WNOHANG`, POSIX defines a zero `si_pid`
/// when no selected child is waitable; using the returned child PID is therefore the portable
/// discriminator instead of treating `si_signo` as the readiness flag. `EINTR` is retried because
/// it does not invalidate the pinned child identity; every other observation error is returned.
/// After descendant cleanup is complete, the caller must consume the status with `Child::wait()`
/// exactly once.
pub(crate) fn observe_child_without_reap(child_pid: u32) -> io::Result<ChildObservation> {
    let child_id = libc::id_t::try_from(child_pid)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "child PID exceeds id_t"))?;
    let mut info = MaybeUninit::<libc::siginfo_t>::zeroed();
    retry_interrupted(|| {
        let result = unsafe {
            libc::waitid(
                libc::P_PID,
                child_id,
                info.as_mut_ptr(),
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        };
        if result == -1 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    })?;
    let info = unsafe { info.assume_init() };
    let observed_pid = unsafe { info.si_pid() };
    if observed_pid == 0 {
        return Ok(ChildObservation::Running);
    }
    let observed_pid = u32::try_from(observed_pid)
        .map_err(|_| io::Error::other("waitid returned an invalid child PID"))?;
    if observed_pid != child_pid {
        return Err(io::Error::other(format!(
            "waitid returned unexpected child PID {observed_pid}; expected {child_pid}"
        )));
    }
    if info.si_signo != libc::SIGCHLD {
        return Err(io::Error::other(format!(
            "waitid returned unexpected signal {} for child {child_pid}",
            info.si_signo
        )));
    }
    Ok(ChildObservation::ExitedUnreaped)
}

/// Wait for a direct child to exit or for `timeout` to elapse without consuming its wait status.
///
/// This is the shared polling boundary for private-process-group callers. An exited result still
/// pins the leader PID because `WNOWAIT` leaves it waitable; a timeout result means the leader is
/// still running. Interrupted observations are retried inside `observe_child_without_reap`; other
/// errors are returned immediately and never fall back to `try_wait()`, because such a fallback
/// could reap the leader before a later group signal.
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
    use std::cell::Cell;
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;
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
    fn retry_interrupted_retries_only_interrupted_errors() {
        let attempts = Cell::new(0usize);
        let result = retry_interrupted(|| {
            let attempt = attempts.get();
            attempts.set(attempt + 1);
            if attempt < 2 {
                Err(io::Error::from(io::ErrorKind::Interrupted))
            } else {
                Ok(17usize)
            }
        })
        .expect("interrupted operations are retried");
        assert_eq!(result, 17);
        assert_eq!(attempts.get(), 3);

        let error = retry_interrupted::<()>(|| Err(io::Error::from(io::ErrorKind::InvalidInput)))
            .expect_err("non-interrupted errors remain fail closed");
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn cancellable_pipe_reader_drains_available_bytes_without_waiting_for_eof() {
        let (reader, mut writer) = UnixStream::pair().expect("unix stream pair");
        writer.write_all(b"ready").expect("write fixture bytes");
        let cancellation = PipeReaderCancellation::new();
        let handle = spawn_bounded_cancellable_pipe_reader(
            reader,
            64,
            Duration::from_millis(1),
            cancellation.clone(),
        )
        .expect("spawn cancellable reader");

        cancellation.cancel();
        let (captured, truncated) = handle
            .join()
            .expect("reader thread join")
            .expect("reader result");

        assert_eq!(captured, b"ready");
        assert!(!truncated);
        drop(writer);
    }

    #[test]
    fn cancellable_pipe_reader_preserves_output_cap_semantics() {
        let (reader, mut writer) = UnixStream::pair().expect("unix stream pair");
        writer
            .write_all(b"abcdefgh")
            .expect("write over-cap fixture bytes");
        let cancellation = PipeReaderCancellation::new();
        let handle = spawn_bounded_cancellable_pipe_reader(
            reader,
            4,
            Duration::from_millis(1),
            cancellation.clone(),
        )
        .expect("spawn cancellable reader");

        cancellation.cancel();
        let (captured, truncated) = handle
            .join()
            .expect("reader thread join")
            .expect("reader result");

        assert_eq!(captured, b"abcd");
        assert!(truncated);
    }

    #[test]
    fn cancellable_pipe_reader_stops_when_input_stays_continuously_readable() {
        struct AlwaysReadyReader(UnixStream);

        impl Read for AlwaysReadyReader {
            fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
                buffer.fill(b'x');
                Ok(buffer.len())
            }
        }

        impl AsRawFd for AlwaysReadyReader {
            fn as_raw_fd(&self) -> std::os::fd::RawFd {
                self.0.as_raw_fd()
            }
        }

        let (reader, _writer) = UnixStream::pair().expect("unix stream pair");
        let cancellation = PipeReaderCancellation::new();
        let handle = spawn_bounded_cancellable_pipe_reader(
            AlwaysReadyReader(reader),
            64,
            Duration::from_millis(1),
            cancellation.clone(),
        )
        .expect("spawn cancellable reader");

        cancellation.cancel();
        let (captured, truncated) = handle
            .join()
            .expect("reader thread join")
            .expect("reader result");

        assert_eq!(captured, vec![b'x'; 64]);
        assert!(truncated);
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
        let status = child.wait().expect("reap terminated leader after group cleanup");
        assert!(!status.success());
    }
}
