const DUPLICATE_AUDIT: &str = include_str!("../src/duplicate_audit.rs");
const MULTIPART_ARCHIVE: &str = include_str!("../src/multipart_archive.rs");
const INCOMPLETE_DOWNLOAD: &str = include_str!("../src/incomplete_download.rs");
const RECOVERY: &str = include_str!("../src/incomplete_download_recovery.rs");
const MATERIALIZATION: &str = include_str!("../src/incomplete_download_materialization.rs");

#[test]
fn public_read_only_roots_bind_traversal_to_open_directory_identity() {
    for required in [
        "pub(crate) struct BoundReadRoot",
        "pub(crate) fn open",
        "pub(crate) fn stable_path",
        "pub(crate) fn canonical_path",
    ] {
        assert!(
            DUPLICATE_AUDIT.contains(required),
            "duplicate_audit.rs missing {required}"
        );
    }

    for (name, source) in [
        ("duplicate_audit", DUPLICATE_AUDIT),
        ("multipart_archive", MULTIPART_ARCHIVE),
        ("incomplete_download", INCOMPLETE_DOWNLOAD),
        ("incomplete_download_recovery", RECOVERY),
        ("incomplete_download_materialization", MATERIALIZATION),
    ] {
        assert!(
            source.contains("BoundReadRoot::open(source_root)"),
            "{name} must bind the caller root with a no-follow directory handle"
        );
        assert!(
            source.contains("root_guard.stable_path()"),
            "{name} must perform filesystem I/O through the handle-bound root namespace"
        );
        assert!(
            !source.contains("std::fs::canonicalize(source_root)"),
            "{name} must not re-resolve the caller root after admission"
        );
    }
}
