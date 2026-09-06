#![cfg(unix)]

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};

#[test]
fn append_after_length_snapshot_is_bounded_to_expected_bytes_plus_one() {
    let fixture = tempfile::tempdir().unwrap();
    let path = fixture.path().join("private-record.json");
    let expected = b"private-record";
    fs::write(&path, expected).unwrap();

    let mut visible = File::open(&path).unwrap();
    let admitted_len = visible.metadata().unwrap().len();
    assert_eq!(admitted_len, expected.len() as u64);

    let appended = vec![b'x'; 4096];
    let mut writer = OpenOptions::new().append(true).open(&path).unwrap();
    writer.write_all(&appended).unwrap();
    assert_eq!(
        fs::metadata(&path).unwrap().len(),
        expected.len() as u64 + appended.len() as u64,
        "fixture must grow after the metadata-length admission"
    );

    visible.seek(SeekFrom::Start(0)).unwrap();
    let mut observed = Vec::with_capacity(expected.len());
    Read::by_ref(&mut visible)
        .take((expected.len() as u64).saturating_add(1))
        .read_to_end(&mut observed)
        .unwrap();

    assert_eq!(
        observed.len(),
        expected.len() + 1,
        "final validation must observe only the expected bytes plus one drift sentinel"
    );
    assert_ne!(
        observed.as_slice(),
        expected,
        "the appended byte must make exact-byte validation fail closed"
    );
    assert!(
        fs::metadata(&path).unwrap().len() > observed.len() as u64,
        "the bounded read must not consume the remainder of a concurrently enlarged record"
    );
}
