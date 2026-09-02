#![cfg(windows)]

#[path = "../src/podman_reclaim_public.rs"]
mod podman_reclaim_public;

use std::path::PathBuf;

#[test]
fn windows_prune_reaches_the_bounded_command_runner() {
    let missing_podman = PathBuf::from(r"C:\disksage-test-missing-podman.exe");

    let error = podman_reclaim_public::prune_dangling_images(
        &missing_podman,
        podman_reclaim_public::DEFAULT_PODMAN_MACHINE,
        "unused-on-purpose",
        "reviewed Windows execution boundary",
        1,
    )
    .expect_err("the missing test executable must fail before any mutation");

    assert_eq!(
        error, "podman-prune-machine-inspect-spawn",
        "Windows must enter the bounded process runner instead of being rejected as unsupported"
    );
}
