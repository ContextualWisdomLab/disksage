use disksage_lib::safety::{journal_append, journal_recent, JournalEntry};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const CHILD_MODE_ENV: &str = "DISKSAGE_JOURNAL_LOCK_CHILD";
const JOURNAL_PATH_ENV: &str = "DISKSAGE_JOURNAL_LOCK_PATH";
const READY_PATH_ENV: &str = "DISKSAGE_JOURNAL_LOCK_READY";
const RELEASE_PATH_ENV: &str = "DISKSAGE_JOURNAL_LOCK_RELEASE";

fn wait_for_path(path: &Path, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if path.exists() {
            return true;
        }
        thread::sleep(Duration::from_millis(10));
    }
    path.exists()
}

#[test]
fn journal_lock_holder_child() {
    if std::env::var_os(CHILD_MODE_ENV).is_none() {
        return;
    }

    let journal_path = PathBuf::from(
        std::env::var_os(JOURNAL_PATH_ENV).expect("child journal path must be supplied"),
    );
    let ready_path = PathBuf::from(
        std::env::var_os(READY_PATH_ENV).expect("child ready path must be supplied"),
    );
    let release_path = PathBuf::from(
        std::env::var_os(RELEASE_PATH_ENV).expect("child release path must be supplied"),
    );

    let journal = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&journal_path)
        .expect("child must open the journal");
    fs4::FileExt::lock(&journal).expect("child must acquire the cross-process journal lock");
    std::fs::write(&ready_path, b"ready").expect("child must publish lock readiness");

    assert!(
        wait_for_path(&release_path, Duration::from_secs(10)),
        "parent did not release the journal lock holder"
    );
    fs4::FileExt::unlock(&journal).expect("child must release the journal lock");
}

#[test]
fn journal_append_waits_for_cross_process_lock() {
    if std::env::var_os(CHILD_MODE_ENV).is_some() {
        return;
    }

    let temp = tempfile::tempdir().expect("temporary directory must exist");
    let journal_path = temp.path().join("journal.jsonl");
    let ready_path = temp.path().join("lock.ready");
    let release_path = temp.path().join("lock.release");

    let current_test = std::env::current_exe().expect("integration-test executable must be known");
    let mut child = Command::new(current_test)
        .arg("--exact")
        .arg("journal_lock_holder_child")
        .arg("--nocapture")
        .env(CHILD_MODE_ENV, "1")
        .env(JOURNAL_PATH_ENV, &journal_path)
        .env(READY_PATH_ENV, &ready_path)
        .env(RELEASE_PATH_ENV, &release_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("journal lock-holder process must start");

    if !wait_for_path(&ready_path, Duration::from_secs(5)) {
        let _ = child.kill();
        let _ = child.wait();
        panic!("journal lock-holder process never acquired its lock");
    }

    let append_path = journal_path.clone();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = journal_append(
            &append_path,
            &JournalEntry {
                ts_ms: 42,
                op: "cross-process-lock-test".into(),
                path: "/tmp/disksage-cross-process-lock".into(),
                bytes: 1,
                outcome: "pending".into(),
            },
        );
        let _ = sender.send(result);
    });

    match receiver.recv_timeout(Duration::from_millis(200)) {
        Err(mpsc::RecvTimeoutError::Timeout) => {}
        Ok(result) => {
            let _ = std::fs::write(&release_path, b"release");
            let _ = child.wait();
            panic!("journal_append ignored a lock held by another process: {result:?}");
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            let _ = std::fs::write(&release_path, b"release");
            let _ = child.wait();
            panic!("journal append worker disconnected before lock release");
        }
    }

    std::fs::write(&release_path, b"release").expect("parent must release the lock holder");
    let child_status = child.wait().expect("journal lock-holder process must exit");
    assert!(child_status.success(), "journal lock-holder process failed");

    receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("journal append must resume after cross-process lock release")
        .expect("journal append must succeed after cross-process lock release");

    let entries = journal_recent(&journal_path, 10);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].op, "cross-process-lock-test");
}
