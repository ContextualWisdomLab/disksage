#![cfg(all(unix, feature = "cloud-cli"))]

use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::process::Command;

#[test]
fn private_output_is_created_mode_0600_under_permissive_umask() {
    let source = tempfile::tempdir().unwrap();
    let private = tempfile::tempdir().unwrap();
    std::fs::set_permissions(private.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let output_path = private.path().join("photo-audit.json");

    let mut command = Command::new(env!("CARGO_BIN_EXE_disksage-photo-similarity-audit"));
    command
        .arg("--root")
        .arg(source.path())
        .arg("--private-output")
        .arg(&output_path);
    unsafe {
        command.pre_exec(|| {
            libc::umask(0);
            Ok(())
        });
    }

    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "private report creation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata = std::fs::metadata(&output_path).unwrap();
    assert!(metadata.is_file());
    assert_eq!(metadata.mode() & 0o777, 0o600);
    assert!(metadata.len() > 0);
}
