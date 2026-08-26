#[cfg(unix)]
#[path = "../src/bound_read_root.rs"]
mod bound_read_root;

#[cfg(unix)]
use bound_read_root::{BoundEntryKind, BoundReadRoot};

#[cfg(unix)]
#[test]
fn descriptor_relative_reads_stay_on_original_root_after_path_replacement() {
    use std::io::Read;
    use std::path::Path;

    let sandbox = tempfile::tempdir().unwrap();
    let selected = sandbox.path().join("selected");
    let moved = sandbox.path().join("moved");
    std::fs::create_dir(&selected).unwrap();
    std::fs::create_dir(selected.join("nested")).unwrap();
    std::fs::write(selected.join("nested/marker.txt"), b"original").unwrap();

    let guard = BoundReadRoot::open(&selected).expect("real directory must bind");
    assert!(guard.canonical_path().is_some());

    std::fs::rename(&selected, &moved).unwrap();
    std::fs::create_dir(&selected).unwrap();
    std::fs::create_dir(selected.join("nested")).unwrap();
    std::fs::write(selected.join("nested/marker.txt"), b"replacement").unwrap();

    let names = guard.read_dir_names(Path::new("")).unwrap();
    assert_eq!(names, vec![std::ffi::OsString::from("nested")]);
    assert_eq!(
        guard.entry_kind(Path::new("nested")).unwrap(),
        BoundEntryKind::Directory
    );

    let mut marker = guard.open_file(Path::new("nested/marker.txt")).unwrap();
    let mut bytes = Vec::new();
    marker.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"original");
    assert_ne!(bytes, b"replacement");

    assert!(
        guard.canonical_path().is_none(),
        "the replaced caller pathname must never regain publication authority"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn stable_namespace_is_resolvable_by_child_process_for_external_evidence_probes() {
    let sandbox = tempfile::tempdir().unwrap();
    let selected = sandbox.path().join("selected");
    std::fs::create_dir(&selected).unwrap();
    std::fs::write(selected.join("marker.txt"), b"external-probe").unwrap();

    let guard = BoundReadRoot::open(&selected).expect("real directory must bind");
    let stable = guard.stable_path().expect("bound root must expose a stable namespace");

    let status = std::process::Command::new("test")
        .arg("-f")
        .arg(stable.join("marker.txt"))
        .status()
        .expect("external test process must start");

    assert!(
        status.success(),
        "the stable namespace must remain resolvable when an external evidence probe runs in a child process"
    );
}
