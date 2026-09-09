#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[test]
fn docker_host_uses_default_docker_from_path_without_explicit_bin() {
    let root = std::env::temp_dir().join(format!(
        "disksage-docker-host-path-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let docker = root.join("docker");
    fs::write(&docker, "#!/bin/sh\nexit 1\n").unwrap();
    let mut permissions = fs::metadata(&docker).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&docker, permissions).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_disksage-container-orphan-plan"))
        .args([
            "--runtime",
            "docker-native",
            "--docker-host",
            "unix:///tmp/disksage-test-docker.sock",
        ])
        .env("PATH", &root)
        .output()
        .expect("container orphan CLI must launch");

    let _ = fs::remove_dir_all(&root);
    assert!(
        output.status.success(),
        "PATH-resolved default docker must reach read-only audit; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("disksage.container-orphan-plan"),
        "read-only audit must still emit a sanitized plan"
    );
}
