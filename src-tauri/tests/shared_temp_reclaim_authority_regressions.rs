#![cfg(unix)]

use disksage_lib::shared_temp_reclaim::{plan_shared_temp_reclaim, seal_completed_temp_artifact};
use std::fs;

fn artifact(prefix: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in(if cfg!(target_os = "macos") {
            "/private/tmp"
        } else {
            "/tmp"
        })
        .expect("temporary artifact")
}

fn planned_artifact(prefix: &str) -> tempfile::TempDir {
    let artifact = artifact(prefix);
    fs::write(artifact.path().join("result.bin"), b"same").expect("write payload");
    seal_completed_temp_artifact(artifact.path(), "disksage:test", 10).expect("seal artifact");
    artifact
}

#[test]
fn producer_seal_never_grants_permanent_mutation_authority() {
    let artifact = planned_artifact("disksage-advisory-only-");
    let plan = plan_shared_temp_reclaim(artifact.path(), 11).expect("plan artifact");
    assert!(!plan.eligible_after_human_approval);
    assert!(plan.exact_approval_phrase.is_none());
    assert!(plan
        .blockers
        .iter()
        .any(|value| value == "shared-temp-permanent-execution-disabled"));
    assert!(artifact.path().exists());
}

#[test]
#[cfg_attr(
    target_os = "macos",
    ignore = "APFS rejects this byte sequence before DiskSage opens it"
)]
fn non_utf8_artifact_path_is_rejected_without_mutation() {
    use std::os::unix::ffi::OsStringExt;
    let root = if cfg!(target_os = "macos") {
        "/private/tmp"
    } else {
        "/tmp"
    };
    let path = std::path::Path::new(root).join(std::ffi::OsString::from_vec(vec![
        b'd', b'i', b's', b'k', b's', b'a', b'g', b'e', b'-', 0xff,
    ]));
    fs::create_dir(&path).expect("create non-UTF-8 artifact");
    let error = seal_completed_temp_artifact(&path, "disksage:test", 10)
        .expect_err("non-UTF-8 paths must fail closed");
    assert_eq!(error, "shared-temp-path-non-utf8-unsupported");
    assert!(path.exists());
    fs::remove_dir(&path).expect("remove test artifact");
}
