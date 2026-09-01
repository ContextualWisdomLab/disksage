#![cfg(unix)]

//! Regression coverage for bounded Colima subprocess inspection.
//!
//! A child process may spawn a descendant that inherits stdout. DiskSage must
//! still return at the configured inspection deadline instead of waiting for
//! that descendant to close the inherited pipe.

use disksage_lib::colima_reclaim::plan_colima_reclaim;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::{Duration, Instant};

#[test]
fn descendant_stdout_cannot_extend_the_colima_timeout() {
    let temp = tempfile::tempdir().expect("temporary directory should be available");
    let cache = temp.path().join("cache");
    fs::create_dir(&cache).expect("cache directory should be created");

    let executable = temp.path().join("colima");
    fs::write(&executable, "#!/bin/sh\n(sleep 1) &\nsleep 10\n")
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
}
