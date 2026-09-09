#![cfg(unix)]

//! Regression coverage for bounded Colima subprocess inspection.
//!
//! A child process may spawn a descendant that inherits stdout. DiskSage must
//! return at the configured inspection deadline and terminate that descendant
//! so a timed-out reclaim cannot keep mutating storage after the API fails.

use disksage_lib::colima_reclaim::plan_colima_reclaim;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn timed_out_colima_descendants_are_terminated() {
    let temp = tempfile::tempdir().expect("temporary directory should be available");
    let cache = temp.path().join("cache");
    fs::create_dir(&cache).expect("cache directory should be created");

    let executable = temp.path().join("colima");
    let descendant_marker = temp.path().join("descendant-survived");
    fs::write(
        &executable,
        "#!/bin/sh\n(sleep 0.4; printf survived > \"$(dirname \"$0\")/descendant-survived\") &\nsleep 10\n",
    )
    .expect("fake Colima executable should be written");
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
        .expect("fake Colima executable should be runnable");

    let started = Instant::now();
    let plan = plan_colima_reclaim(&executable, &cache, Duration::from_millis(100));
    let elapsed = started.elapsed();

    assert!(
        plan.issues.iter().any(|issue| issue == "colima-list-timeout"),
        "timed out list inspection must remain explicit: {:?}",
        plan.issues
    );
    assert!(
        elapsed < Duration::from_millis(700),
        "a descendant holding stdout open extended a 100 ms deadline to {elapsed:?}"
    );

    thread::sleep(Duration::from_millis(600));
    assert!(
        !descendant_marker.exists(),
        "a timed-out Colima descendant kept running after DiskSage returned failure"
    );
}
