//! Native filesystem paths must remain native through the provider-runtime CLI boundary.
//!
//! The output destination is an OS path, not a UTF-8 protocol field. On Unix, a valid absolute
//! path containing non-UTF-8 bytes must therefore remain usable without reflecting those bytes in
//! diagnostics or changing the audit's path-free public evidence contract.

#![cfg(unix)]

use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

fn binary_path() -> &'static Path {
    static BINARY_PATH: OnceLock<PathBuf> = OnceLock::new();
    BINARY_PATH
        .get_or_init(|| {
            let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let target_dir = manifest_dir.join("target").join("cloud-cli-contracts");
            std::fs::create_dir_all(&target_dir)
                .expect("shared Cargo contract target directory must be created");
            let cargo = std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
            let output = Command::new(cargo)
                .current_dir(&manifest_dir)
                .args([
                    "build",
                    "--locked",
                    "--features",
                    "cloud-cli",
                    "--bin",
                    "disksage-provider-client-runtime",
                    "--target-dir",
                ])
                .arg(&target_dir)
                .output()
                .expect("provider client-runtime binary build must start");
            assert!(
                output.status.success(),
                "provider client-runtime binary build failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            target_dir.join("debug").join("disksage-provider-client-runtime")
        })
        .as_path()
}

#[test]
fn non_utf8_absolute_output_path_remains_a_valid_native_destination() {
    let directory = tempfile::tempdir().expect("native output parent must be created");
    let mut file_name = b"provider-runtime-".to_vec();
    file_name.push(0xff);
    file_name.extend_from_slice(b".json");
    let output_path = directory.path().join(OsStr::from_bytes(&file_name));
    assert!(output_path.is_absolute());

    let output = Command::new(binary_path())
        .env_remove("HOME")
        .arg("--output")
        .arg(&output_path)
        .output()
        .expect("provider client-runtime CLI must launch with a native output path");

    assert_eq!(
        output.status.code(),
        Some(0),
        "native absolute paths must not be rejected as UTF-8 protocol input: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("successful provider-runtime stdout must remain JSON");
    assert_eq!(stdout["schema_kind"], "disksage.provider-client-runtime-audit");
    assert_eq!(stdout["local_paths_included"], false);
    assert_eq!(stdout["cloud_write_executed"], false);

    let bytes = std::fs::read(&output_path).expect("native output path must receive the audit");
    let persisted: serde_json::Value =
        serde_json::from_slice(&bytes).expect("persisted provider-runtime audit must remain JSON");
    assert_eq!(persisted["schema_version"], 1);
    assert_eq!(persisted["local_paths_included"], false);

    assert!(
        !output.stdout.windows(file_name.len()).any(|window| window == file_name),
        "public evidence must not reflect the native output filename"
    );
}

#[test]
fn shared_writable_output_parent_never_gains_audit_publication_authority() {
    use std::os::unix::fs::PermissionsExt;

    for mode in [0o720, 0o702] {
        let directory = tempfile::tempdir().expect("authority fixture root must be created");
        let parent = directory.path().join(format!("shared-{mode:o}"));
        std::fs::create_dir(&parent).expect("shared-writable parent fixture must be created");
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(mode))
            .expect("shared-writable parent mode must be set");
        let output_path = parent.join("provider-runtime.json");

        let output = Command::new(binary_path())
            .env_remove("HOME")
            .arg("--output")
            .arg(&output_path)
            .output()
            .expect("provider client-runtime CLI must launch for authority rejection");

        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700))
            .expect("fixture permissions must be restored for cleanup");
        assert_eq!(
            output.status.code(),
            Some(2),
            "group/other-writable output parents must fail closed"
        );
        assert!(
            output.stdout.is_empty(),
            "rejected publication authority must not emit a success report"
        );
        assert_eq!(
            String::from_utf8(output.stderr).expect("diagnostic must remain UTF-8"),
            "provider-client-runtime-output-parent-writable-by-others\n"
        );
        assert!(
            !output_path.exists(),
            "rejected authority must not receive even a partial audit artifact"
        );
    }
}

#[test]
fn shared_writable_output_ancestor_never_gains_directory_replacement_authority() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().expect("authority fixture root must be created");
    let shared_ancestor = directory.path().join("shared-ancestor");
    let private_parent = shared_ancestor.join("private-parent");
    std::fs::create_dir(&shared_ancestor).expect("shared ancestor must be created");
    std::fs::create_dir(&private_parent).expect("private output parent must be created");
    std::fs::set_permissions(&shared_ancestor, std::fs::Permissions::from_mode(0o770))
        .expect("shared ancestor mode must be set");
    std::fs::set_permissions(&private_parent, std::fs::Permissions::from_mode(0o700))
        .expect("private parent mode must be set");
    let output_path = private_parent.join("provider-runtime.json");

    let output = Command::new(binary_path())
        .env_remove("HOME")
        .arg("--output")
        .arg(&output_path)
        .output()
        .expect("provider client-runtime CLI must launch for ancestor authority rejection");

    std::fs::set_permissions(&shared_ancestor, std::fs::Permissions::from_mode(0o700))
        .expect("fixture permissions must be restored for cleanup");
    assert_eq!(
        output.status.code(),
        Some(2),
        "a non-sticky shared-writable ancestor can replace the private parent and must fail closed"
    );
    assert!(
        output.stdout.is_empty(),
        "rejected ancestor authority must not emit a success report"
    );
    assert_eq!(
        String::from_utf8(output.stderr).expect("diagnostic must remain UTF-8"),
        "provider-client-runtime-output-parent-writable-by-others\n"
    );
    assert!(
        !output_path.exists(),
        "rejected ancestor authority must not receive an audit artifact"
    );
}

#[test]
fn symlink_output_ancestor_never_gains_publication_authority() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().expect("authority fixture root must be created");
    let real_ancestor = directory.path().join("real-ancestor");
    let real_parent = real_ancestor.join("private-parent");
    std::fs::create_dir(&real_ancestor).expect("real ancestor must be created");
    std::fs::create_dir(&real_parent).expect("real parent must be created");
    let symlink_ancestor = directory.path().join("redirected-ancestor");
    symlink(&real_ancestor, &symlink_ancestor).expect("ancestor symlink fixture must be created");
    let output_path = symlink_ancestor
        .join("private-parent")
        .join("provider-runtime.json");

    let output = Command::new(binary_path())
        .env_remove("HOME")
        .arg("--output")
        .arg(&output_path)
        .output()
        .expect("provider client-runtime CLI must launch for symlink-ancestor rejection");

    assert_eq!(
        output.status.code(),
        Some(2),
        "a symlink in the output authority chain must fail closed"
    );
    assert!(
        output.stdout.is_empty(),
        "rejected symlink authority must not emit a success report"
    );
    assert_eq!(
        String::from_utf8(output.stderr).expect("diagnostic must remain UTF-8"),
        "provider-client-runtime-output-parent-unsafe\n"
    );
    assert!(
        !real_parent.join("provider-runtime.json").exists(),
        "a rejected symlink ancestor must not redirect audit publication"
    );
}

#[test]
fn symlink_output_leaf_never_overwrites_or_mutates_its_target() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().expect("leaf identity fixture root must be created");
    let target = directory.path().join("external-sensitive.json");
    let original = b"outside-evidence-must-remain-byte-identical";
    std::fs::write(&target, original).expect("external target fixture must be created");
    let output_path = directory.path().join("provider-runtime.json");
    symlink(&target, &output_path).expect("output leaf symlink fixture must be created");

    let output = Command::new(binary_path())
        .env_remove("HOME")
        .arg("--output")
        .arg(&output_path)
        .output()
        .expect("provider client-runtime CLI must launch for symlink-leaf rejection");

    assert_eq!(
        output.status.code(),
        Some(2),
        "create-new publication must never follow or replace an existing symlink leaf"
    );
    assert!(
        output.stdout.is_empty(),
        "rejected leaf identity must not emit a success report"
    );
    assert_eq!(
        String::from_utf8(output.stderr).expect("diagnostic must remain UTF-8"),
        "provider-client-runtime-output-create-failed\n"
    );
    assert_eq!(
        std::fs::read(&target).expect("external target must remain readable"),
        original,
        "a rejected output symlink must leave the external target byte-identical"
    );
    assert!(
        std::fs::symlink_metadata(&output_path)
            .expect("output symlink must remain present")
            .file_type()
            .is_symlink(),
        "rejected publication must not replace the symlink leaf"
    );
}
