//! Regression coverage for private-evidence creation under a restrictive process umask.
//!
//! The Unix `openat(O_CREAT, 0o600)` mode argument is filtered by the process umask. DiskSage
//! promises a durable private evidence object with exact mode `0600`, so publication must explicitly
//! harden the already-opened descriptor rather than relying on the creation request alone.

#![cfg(unix)]

use disksage_lib::private_evidence::write_private_json_create_new;
use std::os::unix::fs::PermissionsExt;

const CHILD_ENV: &str = "DISKSAGE_PRIVATE_EVIDENCE_RESTRICTIVE_UMASK_CHILD";

#[test]
fn restrictive_umask_still_publishes_mode_0600() {
    if std::env::var_os(CHILD_ENV).is_none() {
        let status = std::process::Command::new(std::env::current_exe().expect("test executable"))
            .arg("--exact")
            .arg("restrictive_umask_still_publishes_mode_0600")
            .arg("--nocapture")
            .env(CHILD_ENV, "1")
            .status()
            .expect("spawn isolated restrictive-umask test process");
        assert!(
            status.success(),
            "private evidence publication must succeed under a restrictive umask"
        );
        return;
    }

    // Isolate the process-global umask in this child test process so concurrently executing tests
    // cannot observe the temporary mask. Removing owner-write makes a raw openat(..., 0o600)
    // create mode 0400 and reproduces the production failure boundary.
    unsafe {
        libc::umask(0o200);
    }

    let source = tempfile::tempdir().expect("source tempdir");
    let private = tempfile::tempdir().expect("private tempdir");
    let path = private.path().join("audit.json");
    let receipt = write_private_json_create_new(
        source.path(),
        &path,
        &serde_json::json!({"private": true}),
    )
    .expect("publication must normalize the opened file to mode 0600");

    assert!(receipt.written);
    assert_eq!(receipt.unix_mode, "0600");
    assert_eq!(
        std::fs::metadata(&path)
            .expect("published evidence metadata")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert!(std::fs::metadata(&path).expect("published evidence").len() > 0);
}
