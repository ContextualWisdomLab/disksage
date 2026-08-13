//! Credential-free parser and process-boundary coverage for the Podman reclaim probe.
//!
//! Every fixture uses a synthetic executable and temporary files. The suite never consults a
//! real Podman machine, socket, image, container, volume, account, or mutation endpoint.

#[cfg(unix)]
use disksage_lib::podman_reclaim::{probe_podman_reclaim, DEFAULT_PODMAN_MACHINE};
#[cfg(unix)]
use serde_json::json;
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::{symlink, PermissionsExt};
#[cfg(unix)]
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::time::Duration;

#[cfg(unix)]
fn write_executable(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

#[cfg(unix)]
fn write_probe_script(
    directory: &Path,
    inspect: &str,
    guest_df: &str,
    info: &str,
    system_df: &str,
    images: &str,
) -> PathBuf {
    let executable = directory.join("podman");
    write_executable(
        &executable,
        &format!(
            r#"#!/bin/sh
case "$*" in
  "machine inspect podman-machine-default")
    cat <<'DISKSAGE_INSPECT'
{inspect}
DISKSAGE_INSPECT
    ;;
  "machine ssh podman-machine-default -- df -B1 --output=size,used,avail /")
    cat <<'DISKSAGE_GUEST_DF'
{guest_df}
DISKSAGE_GUEST_DF
    ;;
  "--connection podman-machine-default info --format json")
    cat <<'DISKSAGE_INFO'
{info}
DISKSAGE_INFO
    ;;
  "--connection podman-machine-default system df --format json")
    cat <<'DISKSAGE_SYSTEM_DF'
{system_df}
DISKSAGE_SYSTEM_DF
    ;;
  "--connection podman-machine-default images --all --format json")
    cat <<'DISKSAGE_IMAGES'
{images}
DISKSAGE_IMAGES
    ;;
  *)
    printf '%s\n' "unexpected arguments: $*" >&2
    exit 91
    ;;
esac
"#
        ),
    );
    executable
}

#[cfg(unix)]
fn valid_info() -> String {
    json!({
        "store": {
            "graphRoot": "/var/lib/containers/storage",
            "graphRootAllocated": 10,
            "graphRootUsed": 5,
            "imageStore": { "number": 0 },
            "containerStore": { "number": 0, "running": 0, "stopped": 0 }
        }
    })
    .to_string()
}

#[cfg(unix)]
fn valid_system_df() -> String {
    json!([
        { "Type": "Images", "Total": 0, "Active": 0, "RawSize": 0, "RawReclaimable": 0 },
        { "Type": "Containers", "Total": 0, "Active": 0, "RawSize": 0, "RawReclaimable": 0 },
        { "Type": "Local Volumes", "Total": 0, "Active": 0, "RawSize": 0, "RawReclaimable": 0 }
    ])
    .to_string()
}

#[cfg(unix)]
fn valid_inspect(directory: &Path) -> String {
    json!([{
        "ConfigDir": { "Path": directory.to_string_lossy().into_owned() },
        "Name": DEFAULT_PODMAN_MACHINE,
        "State": "running",
        "Resources": { "DiskSize": 1 }
    }])
    .to_string()
}

#[cfg(unix)]
fn install_raw_config(directory: &Path, raw_path: &Path) {
    fs::write(
        directory.join(format!("{DEFAULT_PODMAN_MACHINE}.json")),
        json!({ "ImagePath": { "Path": raw_path.to_string_lossy().into_owned() } }).to_string(),
    )
    .unwrap();
}

#[cfg(unix)]
fn assert_issue(plan: &disksage_lib::podman_reclaim::PodmanReclaimPlan, expected: &str) {
    assert!(
        plan.issues.iter().any(|issue| issue == expected),
        "expected {expected:?}, got {:?}",
        plan.issues
    );
    assert!(!plan.evidence_complete);
    assert!(plan
        .assessment
        .reason_codes
        .iter()
        .any(|code| code == "partial-evidence"));
}

#[cfg(unix)]
#[test]
fn inspect_parser_rejects_malformed_count_name_config_and_identity_shapes() {
    let cases = [
        ("not-json".to_string(), "invalid-machine-inspect-json:"),
        ("[]".to_string(), "unexpected-machine-count:0"),
        (
            json!([{
                "ConfigDir": { "Path": "/tmp" },
                "Name": "../unsafe",
                "State": "running",
                "Resources": { "DiskSize": 1 }
            }])
            .to_string(),
            "unsafe-machine-name",
        ),
        (
            json!([{
                "ConfigDir": { "Path": "relative" },
                "Name": DEFAULT_PODMAN_MACHINE,
                "State": "running",
                "Resources": { "DiskSize": 1 }
            }])
            .to_string(),
            "machine-config-dir-not-absolute",
        ),
        (
            json!([{
                "ConfigDir": { "Path": "/tmp" },
                "Name": "another-machine",
                "State": "running",
                "Resources": { "DiskSize": 1 }
            }])
            .to_string(),
            "machine-name-mismatch",
        ),
    ];

    for (inspect, expected) in cases {
        let temp = tempfile::tempdir().unwrap();
        let executable = write_probe_script(
            temp.path(),
            &inspect,
            "1 0 1",
            &valid_info(),
            &valid_system_df(),
            "[]",
        );
        let plan = probe_podman_reclaim(
            &executable,
            DEFAULT_PODMAN_MACHINE,
            Duration::from_secs(2),
        );
        assert!(plan.machine.is_none());
        if expected.ends_with(':') {
            assert!(
                plan.issues.iter().any(|issue| issue.starts_with(expected)),
                "expected prefix {expected:?}, got {:?}",
                plan.issues
            );
            assert!(!plan.evidence_complete);
        } else {
            assert_issue(&plan, expected);
        }
    }
}

#[cfg(unix)]
#[test]
fn raw_image_probe_rejects_symlink_directory_and_missing_targets() {
    for kind in ["symlink", "directory", "missing"] {
        let temp = tempfile::tempdir().unwrap();
        let real_file = temp.path().join("real.raw");
        fs::write(&real_file, b"raw").unwrap();
        let raw_path = match kind {
            "symlink" => {
                let link = temp.path().join("link.raw");
                symlink(&real_file, &link).unwrap();
                link
            }
            "directory" => temp.path().join("raw-directory"),
            "missing" => temp.path().join("missing.raw"),
            _ => unreachable!(),
        };
        if kind == "directory" {
            fs::create_dir(&raw_path).unwrap();
        }
        install_raw_config(temp.path(), &raw_path);
        let executable = write_probe_script(
            temp.path(),
            &valid_inspect(temp.path()),
            "10 1 9",
            &valid_info(),
            &valid_system_df(),
            "[]",
        );
        let plan = probe_podman_reclaim(
            &executable,
            DEFAULT_PODMAN_MACHINE,
            Duration::from_secs(2),
        );
        assert!(plan.raw_image.is_none());
        match kind {
            "symlink" => assert_issue(&plan, "raw-image-symbolic-link"),
            "directory" => assert_issue(&plan, "raw-image-not-regular-file"),
            "missing" => {
                assert!(plan
                    .issues
                    .iter()
                    .any(|issue| issue.starts_with("raw-image-metadata:")));
                assert!(!plan.evidence_complete);
            }
            _ => unreachable!(),
        }
    }
}

#[cfg(unix)]
#[test]
fn guest_df_parser_distinguishes_empty_invalid_arity_and_consistency_failures() {
    let cases = [
        ("", "guest-df-empty"),
        ("10 nope 1", "guest-df-invalid"),
        ("10 1", "guest-df-field-count"),
        ("10 6 5", "guest-df-inconsistent"),
    ];
    for (guest_df, expected) in cases {
        let temp = tempfile::tempdir().unwrap();
        let raw = temp.path().join("machine.raw");
        fs::write(&raw, b"raw").unwrap();
        install_raw_config(temp.path(), &raw);
        let executable = write_probe_script(
            temp.path(),
            &valid_inspect(temp.path()),
            guest_df,
            &valid_info(),
            &valid_system_df(),
            "[]",
        );
        let plan = probe_podman_reclaim(
            &executable,
            DEFAULT_PODMAN_MACHINE,
            Duration::from_secs(2),
        );
        assert!(plan.guest_filesystem.is_none());
        assert_issue(&plan, expected);
    }
}

#[cfg(unix)]
#[test]
fn podman_info_parser_distinguishes_json_store_graph_and_numeric_failures() {
    let cases = [
        ("not-json".to_string(), "invalid-podman-info-json:"),
        (json!({}).to_string(), "podman-info-field-missing:store"),
        (
            json!({ "store": { "graphRoot": 7 } }).to_string(),
            "podman-info-field-invalid:store.graphRoot",
        ),
        (
            json!({
                "store": {
                    "graphRoot": "/var/lib/containers",
                    "graphRootAllocated": "ten"
                }
            })
            .to_string(),
            "podman-info-field-invalid:store.graphRootAllocated",
        ),
        (
            json!({
                "store": {
                    "graphRoot": "/var/lib/containers",
                    "graphRootAllocated": 10,
                    "graphRootUsed": 5,
                    "imageStore": {},
                    "containerStore": { "number": 0, "running": 0, "stopped": 0 }
                }
            })
            .to_string(),
            "podman-info-field-missing:store.imageStore.number",
        ),
    ];
    for (info, expected) in cases {
        let temp = tempfile::tempdir().unwrap();
        let raw = temp.path().join("machine.raw");
        fs::write(&raw, b"raw").unwrap();
        install_raw_config(temp.path(), &raw);
        let executable = write_probe_script(
            temp.path(),
            &valid_inspect(temp.path()),
            "10 1 9",
            &info,
            &valid_system_df(),
            "[]",
        );
        let plan = probe_podman_reclaim(
            &executable,
            DEFAULT_PODMAN_MACHINE,
            Duration::from_secs(2),
        );
        assert!(plan.store.is_none());
        if expected.ends_with(':') {
            assert!(plan.issues.iter().any(|issue| issue.starts_with(expected)));
            assert!(!plan.evidence_complete);
        } else {
            assert_issue(&plan, expected);
        }
    }
}

#[cfg(unix)]
#[test]
fn system_df_parser_rejects_unknown_duplicate_missing_and_reclaim_overflow_shapes() {
    let cases = [
        (
            json!([{ "Type": "Mystery", "Total": 0, "Active": 0, "RawSize": 0, "RawReclaimable": 0 }]).to_string(),
            "podman-system-df-unknown-type:Mystery",
        ),
        (
            json!([
                { "Type": "Images", "Total": 0, "Active": 0, "RawSize": 0, "RawReclaimable": 0 },
                { "Type": "Images", "Total": 0, "Active": 0, "RawSize": 0, "RawReclaimable": 0 }
            ]).to_string(),
            "podman-system-df-duplicate-type",
        ),
        (
            json!([
                { "Type": "Images", "Total": 0, "Active": 0, "RawSize": 0, "RawReclaimable": 0 },
                { "Type": "Containers", "Total": 0, "Active": 0, "RawSize": 0, "RawReclaimable": 0 }
            ]).to_string(),
            "podman-system-df-missing-local-volumes",
        ),
        (
            json!([{ "Type": "Images", "Total": 1, "Active": 0, "RawSize": 1, "RawReclaimable": 2 }]).to_string(),
            "podman-system-df-inconsistent",
        ),
    ];
    for (system_df, expected) in cases {
        let temp = tempfile::tempdir().unwrap();
        let raw = temp.path().join("machine.raw");
        fs::write(&raw, b"raw").unwrap();
        install_raw_config(temp.path(), &raw);
        let executable = write_probe_script(
            temp.path(),
            &valid_inspect(temp.path()),
            "10 1 9",
            &valid_info(),
            &system_df,
            "[]",
        );
        let plan = probe_podman_reclaim(
            &executable,
            DEFAULT_PODMAN_MACHINE,
            Duration::from_secs(2),
        );
        assert!(plan.system_df.is_none());
        assert_issue(&plan, expected);
    }
}

#[cfg(unix)]
#[test]
fn image_parser_rejects_duplicate_and_size_overflow_then_hashes_normalized_tags() {
    let duplicate_id = "a".repeat(64);
    let overflow_id = "b".repeat(64);
    let normalized_id = "c".repeat(64);
    let untagged_id = "d".repeat(64);
    let cases = [
        (
            json!([
                { "Id": duplicate_id, "RepoTags": [], "Containers": 0, "Size": 1 },
                { "Id": "a".repeat(64), "RepoTags": [], "Containers": 0, "Size": 2 }
            ]).to_string(),
            Some("podman-images-duplicate-id"),
        ),
        (
            json!([
                { "Id": "a".repeat(64), "RepoTags": [], "Containers": 0, "Size": u64::MAX },
                { "Id": overflow_id, "RepoTags": [], "Containers": 0, "Size": 1 }
            ]).to_string(),
            Some("podman-images-size-overflow"),
        ),
        (
            json!([
                { "Id": normalized_id, "RepoTags": ["z:latest", "a:latest", "a:latest"], "Containers": 0, "Size": 7 },
                { "Id": untagged_id, "RepoTags": null, "Containers": 0, "Size": 11 }
            ]).to_string(),
            None,
        ),
    ];

    for (images, expected) in cases {
        let temp = tempfile::tempdir().unwrap();
        let raw = temp.path().join("machine.raw");
        fs::write(&raw, b"raw").unwrap();
        install_raw_config(temp.path(), &raw);
        let executable = write_probe_script(
            temp.path(),
            &valid_inspect(temp.path()),
            "10 1 9",
            &valid_info(),
            &valid_system_df(),
            &images,
        );
        let plan = probe_podman_reclaim(
            &executable,
            DEFAULT_PODMAN_MACHINE,
            Duration::from_secs(2),
        );
        if let Some(expected) = expected {
            assert!(plan.unused_images.is_none());
            assert_issue(&plan, expected);
        } else {
            let evidence = plan.unused_images.expect("normalized image evidence");
            assert_eq!(evidence.unused_records, 2);
            assert_eq!(evidence.unused_tagged_records, 1);
            assert_eq!(evidence.unused_untagged_records, 1);
            assert_eq!(evidence.candidate_record_size_sum, 18);
            assert_eq!(evidence.candidate_set_sha256.len(), 64);
        }
    }
}

#[cfg(unix)]
#[test]
fn inspect_timeout_is_bounded_and_fails_closed() {
    let temp = tempfile::tempdir().unwrap();
    let executable = temp.path().join("podman");
    write_executable(
        &executable,
        r#"#!/bin/sh
sleep 1
printf '%s\n' '[]'
"#,
    );
    let plan = probe_podman_reclaim(
        &executable,
        DEFAULT_PODMAN_MACHINE,
        Duration::from_millis(10),
    );
    assert_eq!(plan.issues, vec!["podman-machine-inspect-timeout"]);
    assert!(!plan.evidence_complete);
    assert!(plan.machine.is_none());
}
