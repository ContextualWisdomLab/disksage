#![cfg(windows)]

use std::ffi::c_void;
use std::fs;
use std::fs::{File, OpenOptions};
use std::mem::{size_of, MaybeUninit};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::process::Command;

const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
const FILE_SHARE_READ: u32 = 0x0000_0001;
const FILE_SHARE_WRITE: u32 = 0x0000_0002;
const FILE_SHARE_DELETE: u32 = 0x0000_0004;
const FILE_ID_INFO_CLASS: i32 = 0x12;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileId128 {
    identifier: [u8; 16],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FileIdInfo {
    volume_serial_number: u64,
    file_id: FileId128,
}

#[link(name = "Kernel32")]
unsafe extern "system" {
    fn GetFileInformationByHandleEx(
        file: *mut c_void,
        file_information_class: i32,
        file_information: *mut c_void,
        buffer_size: u32,
    ) -> i32;
}

fn create_project_with_target(name: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
    let root = tempfile::tempdir().expect("temp root");
    let project = root.path().join("project");
    let target = project.join("target");
    let artifact = target.join("artifact.bin");

    fs::create_dir_all(&target).expect("target dir");
    fs::write(
        project.join("Cargo.toml"),
        format!(
            "[package]\nname=\"{name}\"\nversion=\"0.1.0\"\nedition=\"2021\"\n"
        ),
    )
    .expect("manifest");
    fs::write(&artifact, vec![0x5a; 4096]).expect("artifact");

    (root, project, artifact)
}

fn run_reclaim(project: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_disksage-cargo-target-clean"))
        .arg("--project-dir")
        .arg(project)
        .output()
        .expect("run cargo target clean CLI")
}

fn open_directory_for_identity(path: &Path) -> File {
    OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .expect("open directory with delete sharing and backup semantics")
}

fn file_identity(file: &File) -> FileIdInfo {
    let mut info = MaybeUninit::<FileIdInfo>::zeroed();
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle().cast(),
            FILE_ID_INFO_CLASS,
            info.as_mut_ptr().cast(),
            size_of::<FileIdInfo>() as u32,
        )
    };
    assert_ne!(ok, 0, "GetFileInformationByHandleEx(FileIdInfo) must succeed");
    unsafe { info.assume_init() }
}

#[test]
fn opened_windows_directory_identity_survives_path_replacement() {
    let (_root, project, _artifact) = create_project_with_target("windows-cargo-identity");
    let target = project.join("target");
    let detached = project.join("target.detached");

    let reviewed_handle = open_directory_for_identity(&target);
    let reviewed_identity = file_identity(&reviewed_handle);

    fs::rename(&target, &detached).expect("rename reviewed target while handle is open");
    fs::create_dir(&target).expect("create replacement target at original pathname");

    let replacement_handle = open_directory_for_identity(&target);
    let replacement_identity = file_identity(&replacement_handle);
    let reviewed_identity_after_replacement = file_identity(&reviewed_handle);

    assert_eq!(
        reviewed_identity_after_replacement, reviewed_identity,
        "an opened directory handle must retain the reviewed filesystem-object identity after rename"
    );
    assert_ne!(
        replacement_identity, reviewed_identity,
        "a new directory at the same pathname must have a different FILE_ID_INFO identity"
    );
    assert!(
        detached.join("artifact.bin").is_file(),
        "the reviewed artifact must remain attached to the opened object after pathname replacement"
    );
}

#[test]
fn idle_windows_target_reclaim_uses_native_authority_and_cleans() {
    let (_root, project, artifact) = create_project_with_target("windows-cargo-reclaim-idle");
    let output = run_reclaim(&project);

    assert!(
        output.status.success(),
        "an idle owned Windows target must reach the native deletion authority; status={:?}, stderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    let receipt: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("CLI emits JSON receipt");
    assert_eq!(receipt["executed"], true);
    assert!(
        receipt["observed_reduction_bytes"].as_u64().unwrap_or(0) >= 4096,
        "reclaim receipt must report the removed real artifact"
    );
    assert!(
        !artifact.exists(),
        "the reviewed target artifact must be removed after successful reclaim"
    );
}

#[test]
fn active_windows_target_holder_refuses_before_mutation() {
    let (_root, project, artifact) = create_project_with_target("windows-cargo-reclaim-active");
    let holder = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .open(&artifact)
        .expect("open artifact with no sharing");

    let output = run_reclaim(&project);
    drop(holder);

    assert!(
        !output.status.success(),
        "an actively held Windows target must be refused"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("cargo-target-active-holders-present"),
        "refusal must come from the native active-use authority, not a generic unsupported/error path; stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        artifact.is_file(),
        "active-holder refusal must preserve the reviewed artifact"
    );
}
