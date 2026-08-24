//! Regression for the File Provider dump success-path process-group boundary.
//!
//! A File Provider helper can outlive the `fileproviderctl` leader while retaining the inherited
//! stdout descriptor. The successful leader-exit arm must therefore terminate the private process
//! group before the production code joins the stdout reader; otherwise a normal provider probe can
//! block indefinitely waiting for EOF.

#[test]
fn successful_provider_dump_terminates_private_group_before_reader_join() {
    let source = include_str!("../src/provider_global_sync.rs");
    let run_dump = source
        .split_once("fn run_dump(provider: CloudProvider) -> Result<String, String> {")
        .expect("provider global-sync run_dump boundary must exist")
        .1
        .split_once("pub fn inspect_new_copy_admission")
        .expect("provider global-sync run_dump boundary must end before public admission")
        .0;

    assert!(
        run_dump.contains(
            "Ok(Some(status)) => {\n                kill_group();\n                break status;\n            }"
        ),
        "successful fileproviderctl exit must terminate the private process group before joining stdout"
    );

    let success_arm = run_dump
        .find("Ok(Some(status)) => {")
        .expect("successful child-exit arm must exist");
    let group_kill = run_dump[success_arm..]
        .find("kill_group();")
        .map(|offset| success_arm + offset)
        .expect("successful child-exit arm must kill its private process group");
    let reader_join = run_dump
        .find("let bytes = reader")
        .expect("reader join boundary must exist");
    assert!(
        group_kill < reader_join,
        "private process group must be terminated before the stdout reader is joined"
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
