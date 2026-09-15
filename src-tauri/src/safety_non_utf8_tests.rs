use std::ffi::OsString;
use std::os::unix::ffi::OsStringExt;

#[test]
fn non_utf8_sidecar_marker_still_vetoes_cleanup() {
    let tmp = tempfile::tempdir().expect("create safety fixture");
    let invalid_name = OsString::from_vec(vec![b'c', b'r', b'm', 0xff]);
    let protected_path = tmp.path().join(&invalid_name);
    match std::fs::create_dir(&protected_path) {
        Ok(()) => {}
        Err(error) if error.raw_os_error() == Some(libc::EILSEQ) => return,
        Err(error) => panic!("create non-UTF8 fixture directory: {error}"),
    }

    let mut marker_name = invalid_name;
    marker_name.push(crate::safety::PROTECTED_PATH_MARKER);
    std::fs::write(tmp.path().join(marker_name), []).expect("write sidecar keep marker");

    assert!(
        crate::safety::is_explicitly_protected(&protected_path),
        "a protection sidecar must remain authoritative even when the protected filename is not UTF-8"
    );
}
