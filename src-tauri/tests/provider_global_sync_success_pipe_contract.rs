//! Regression for the File Provider dump success-path process-group boundary.
//!
//! A File Provider helper can outlive the `fileproviderctl` leader while retaining the inherited
//! stdout descriptor. The successful leader-exit path must keep the leader waitable until the
//! private group is terminated; reaping first would release the numeric PID/PGID before the later
//! negative-PID signal. The provider adapter must therefore consume the canonical no-reap Unix
//! lifecycle rather than protecting the historical `try_wait() -> kill_group()` ordering.

#[test]
fn successful_provider_dump_keeps_group_identity_pinned_until_cleanup() {
    let source = include_str!("../src/provider_global_sync.rs");
    let run_dump = source
        .split_once("fn run_dump(provider: CloudProvider) -> Result<String, String> {")
        .expect("provider global-sync run_dump boundary must exist")
        .1
        .split_once("pub fn inspect_new_copy_admission")
        .expect("provider global-sync run_dump boundary must end before public admission")
        .0;

    assert!(
        run_dump.contains("wait_for_child_without_reap("),
        "provider success/timeout observation must consume the canonical no-reap lifecycle"
    );
    assert!(
        run_dump.contains("NoReapWaitOutcome::ExitedUnreaped"),
        "successful provider probe must distinguish an exited-but-unreaped leader"
    );
    assert!(
        run_dump.contains("signal_private_process_group(child_pid, libc::SIGKILL)"),
        "provider descendants must be terminated through the canonical private-group signal"
    );
    assert!(
        !run_dump.contains("child.try_wait()"),
        "provider success must not reap the group leader before descendant cleanup"
    );

    let exited_arm = run_dump
        .find("NoReapWaitOutcome::ExitedUnreaped")
        .expect("exited-unreaped branch must exist");
    let group_kill = run_dump[exited_arm..]
        .find("signal_private_process_group(child_pid, libc::SIGKILL)")
        .map(|offset| exited_arm + offset)
        .expect("exited-unreaped branch must terminate the private group");
    let final_reap = run_dump[group_kill..]
        .find("child.wait()")
        .map(|offset| group_kill + offset)
        .expect("leader status must be consumed after private-group cleanup");
    let reader_join = run_dump
        .find("let bytes = reader")
        .expect("reader join boundary must exist");
    assert!(
        group_kill < final_reap && final_reap < reader_join,
        "private group cleanup must precede leader reap, which must precede stdout reader join"
    );
}

#[cfg(unix)]
#[test]
fn descendant_inheriting_stdout_keeps_pipe_open_until_private_group_is_terminated() {
    use std::io::Read;
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::sync::mpsc::{self, RecvTimeoutError};
    use std::thread;
    use std::time::Duration;

    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", "(sleep 30) & printf 'probe-output\\n'"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }

    let mut child = command.spawn().expect("probe fixture must spawn");
    let process_group = child.id() as libc::pid_t;
    let mut stdout = child.stdout.take().expect("probe fixture stdout must be piped");
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout.read_to_end(&mut bytes).map(|_| bytes);
        let _ = sender.send(result);
    });

    let status = child.wait().expect("probe fixture leader must be waitable");
    assert!(status.success(), "probe fixture leader must exit successfully");

    let before_group_kill = receiver.recv_timeout(Duration::from_millis(250));
    unsafe {
        let _ = libc::kill(-process_group, libc::SIGKILL);
    }
    assert!(
        matches!(before_group_kill, Err(RecvTimeoutError::Timeout)),
        "a surviving descendant that inherited stdout must prevent EOF after leader exit"
    );

    let bytes = receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("terminating the private process group must release inherited stdout promptly")
        .expect("fixture stdout read must succeed");
    assert_eq!(bytes, b"probe-output\n");
}
