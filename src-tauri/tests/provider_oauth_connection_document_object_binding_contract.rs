//! Contract for object-bound OAuth connection-document reads.
//!
//! The durable authority file is attacker-replaceable within the same user session. A path-based
//! metadata check followed by `std::fs::read(path)` re-resolves the pathname and can therefore read
//! a different object (including a just-substituted symlink) than the object that passed admission.
//! The reader must open the final component without following links, validate metadata from that
//! handle, and bound bytes from the same handle.

#[test]
fn production_reader_is_bound_to_one_open_file_object() {
    let source = include_str!("../src/provider_oauth.rs");

    assert!(
        !source.contains("let bytes = std::fs::read(path)"),
        "connection-document admission and bytes must not be split across two pathname resolutions"
    );
    assert!(
        source.contains("libc::O_NOFOLLOW"),
        "Unix connection-document open must reject a substituted final-component symlink"
    );
    assert!(
        source.contains("FILE_FLAG_OPEN_REPARSE_POINT"),
        "Windows connection-document open must inspect the reparse-point object instead of following it"
    );
    assert!(
        source.contains("file.metadata()"),
        "regular-file, permission, and size admission must come from the opened object"
    );
    assert!(
        source.contains(".take(MAX_CONNECTION_DOCUMENT_BYTES + 1)"),
        "connection-document reads must remain bounded even if the opened file grows after metadata admission"
    );
}
