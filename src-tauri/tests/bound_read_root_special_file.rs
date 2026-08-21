#![cfg(unix)]

#[path = "../src/bound_read_root.rs"]
mod bound_read_root;

use bound_read_root::{BoundEntryKind, BoundReadRoot};
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

#[test]
fn bound_read_root_rejects_fifo_when_regular_file_open_is_requested() {
    let root = tempfile::tempdir().expect("temporary bound-root fixture");
    let fifo = root.path().join("candidate.pipe");
    let fifo_name = CString::new(fifo.as_os_str().as_bytes()).expect("fifo path must be NUL-free");
    let created = unsafe {
        // SAFETY: fifo_name is a live NUL-terminated path and the mode grants only owner access.
        libc::mkfifo(fifo_name.as_ptr(), 0o600)
    };
    assert_eq!(created, 0, "mkfifo must create the special-file regression fixture");

    let guard = BoundReadRoot::open(root.path()).expect("real directory must bind");
    assert_eq!(
        guard.entry_kind(Path::new("candidate.pipe")).expect("inspect FIFO kind"),
        BoundEntryKind::Other,
        "the authority layer must classify a FIFO as non-regular evidence"
    );

    let error = guard
        .open_file(Path::new("candidate.pipe"))
        .expect_err("open_file must never return a readable handle for a non-regular entry");
    assert_eq!(
        error.kind(),
        std::io::ErrorKind::InvalidData,
        "non-regular entries must fail with a deterministic typed I/O category"
    );
}
