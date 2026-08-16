#[path = "../src/bound_read_root.rs"]
mod bound_read_root;

const BOUND_READ_ROOT: &str = include_str!("../src/bound_read_root.rs");
const DUPLICATE_AUDIT: &str = include_str!("../src/duplicate_audit.rs");
const MULTIPART_ARCHIVE: &str = include_str!("../src/multipart_archive.rs");
const INCOMPLETE_DOWNLOAD: &str = include_str!("../src/incomplete_download.rs");
const RECOVERY: &str = include_str!("../src/incomplete_download_recovery.rs");
const MATERIALIZATION: &str = include_str!("../src/incomplete_download_materialization.rs");

#[test]
fn shared_root_guard_exposes_identity_bound_contract() {
    for required in [
        "pub(crate) struct BoundReadRoot",
        "pub(crate) fn open",
        "pub(crate) fn stable_path",
        "pub(crate) fn canonical_path",
    ] {
        assert!(
            BOUND_READ_ROOT.contains(required),
            "bound_read_root.rs missing {required}"
        );
    }
}

#[test]
fn public_read_only_roots_bind_traversal_to_open_directory_identity() {
    assert!(
        DUPLICATE_AUDIT.contains("pub(crate) mod bound_read_root"),
        "duplicate_audit must register the shared bound-root module without touching lib.rs"
    );

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
            source.matches("root_guard.canonical_path()").count() >= 2,
            "{name} must revalidate the caller pathname before publishing evidence"
        );
        assert!(
            !source.contains("std::fs::canonicalize(source_root)"),
            "{name} must not re-resolve the caller root outside the bound-root guard"
        );
    }
}
