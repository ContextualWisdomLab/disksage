#![cfg(unix)]

#[path = "../src/private_directory_publication.rs"]
mod private_directory_publication;
#[path = "../src/private_evidence.rs"]
mod private_evidence;

use private_directory_publication::write_private_bytes_create_new_with_parents_with_hooks;
use private_evidence::write_object_bound_bytes_create_new_with_hooks;
use std::ffi::CString;
use std::fs;
use std::os::fd::RawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{FileTypeExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

fn set_private_directory(path: &Path) {
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).expect("set private directory mode");
}

fn replace_with_fifo(path: &Path) {
    fs::remove_file(path).expect("remove admitted record pathname");
    let path_c = CString::new(path.as_os_str().as_bytes()).expect("fifo pathname");
    let result = unsafe { libc::mkfifo(path_c.as_ptr(), 0o600) };
    assert_eq!(result, 0, "create replacement FIFO");
}

fn delayed_nonblocking_writer(path: PathBuf) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(2));
        let path_c = CString::new(path.as_os_str().as_bytes()).expect("writer pathname");
        let fd: RawFd = unsafe {
            libc::open(
                path_c.as_ptr(),
                libc::O_WRONLY | libc::O_NONBLOCK | libc::O_CLOEXEC,
            )
        };
        if fd >= 0 {
            unsafe { libc::close(fd) };
        }
    })
}

#[test]
fn no_policy_final_reopen_rejects_fifo_without_waiting_for_a_writer() {
    let fixture = tempfile::tempdir().expect("tempdir");
    set_private_directory(fixture.path());
    let target = fixture.path().join("receipt.json");
    let target_for_hook = target.clone();
    let writer = delayed_nonblocking_writer(target.clone());
    let started = Instant::now();

    let error = write_private_bytes_create_new_with_parents_with_hooks(
        &target,
        b"authorized",
        0o600,
        0o700,
        || {},
        move || replace_with_fifo(&target_for_hook),
    )
    .expect_err("FIFO pathname substitution must fail closed");
    let elapsed = started.elapsed();

    assert_eq!(error, "private-directory-publication-file-identity-drift");
    assert!(
        elapsed < Duration::from_secs(1),
        "final identity inspection must not block on a substituted FIFO: {elapsed:?}"
    );
    writer.join().expect("writer thread");
    assert!(
        fs::symlink_metadata(&target)
            .expect("replacement metadata")
            .file_type()
            .is_fifo(),
        "failure cleanup must not mutate the unrelated replacement pathname"
    );
}

#[test]
fn source_policy_final_reopen_rejects_fifo_without_waiting_for_a_writer() {
    let source = tempfile::tempdir().expect("source tempdir");
    let destination = tempfile::tempdir().expect("destination tempdir");
    set_private_directory(source.path());
    set_private_directory(destination.path());
    let target = destination.path().join("audit.json");
    let target_for_hook = target.clone();
    let writer = delayed_nonblocking_writer(target.clone());
    let started = Instant::now();

    let error = write_object_bound_bytes_create_new_with_hooks(
        &target,
        b"private",
        0o600,
        Some(source.path()),
        || {},
        || {},
        move || replace_with_fifo(&target_for_hook),
    )
    .expect_err("FIFO pathname substitution must fail closed");
    let elapsed = started.elapsed();

    assert_eq!(format!("{error:?}"), "RecordIdentityDrift");
    assert!(
        elapsed < Duration::from_secs(1),
        "final identity inspection must not block on a substituted FIFO: {elapsed:?}"
    );
    writer.join().expect("writer thread");
    assert!(
        fs::symlink_metadata(&target)
            .expect("replacement metadata")
            .file_type()
            .is_fifo(),
        "failure cleanup must not mutate the unrelated replacement pathname"
    );
}
