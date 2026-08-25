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
        // These are source-contract assertions, so formatting-only rustfmt line wrapping must not
        // turn a valid identity-binding implementation RED. Compact whitespace before matching
        // chained calls while keeping the exact authority-bearing symbols in the contract.
        let compact_source = source.split_whitespace().collect::<String>();
        assert!(
            compact_source.contains("BoundReadRoot::open(source_root)"),
            "{name} must bind the caller root with a no-follow directory handle"
        );
        assert!(
            compact_source.contains("root_guard.read_dir_names")
                || compact_source.contains("root_guard.entry_kind")
                || compact_source.contains("root_guard.open_file"),
            "{name} must perform filesystem I/O through the handle-bound root namespace"
        );
        assert!(
            compact_source
                .matches("root_guard.canonical_path()")
                .count()
                >= 2,
            "{name} must revalidate the caller pathname before publishing evidence"
        );
        assert!(
            !compact_source.contains("std::fs::canonicalize(source_root)"),
            "{name} must not re-resolve the caller root outside the bound-root guard"
        );
    }

    assert!(
        RECOVERY.contains("let active_use_path = match std::fs::canonicalize(&path)"),
        "recovery must canonicalize the bound child only for external active-use probes"
    );
    assert!(
        RECOVERY.contains("observe_path_active_use(&active_use_path)"),
        "recovery active-use probes must not receive the Linux proc namespace path"
    );
    assert!(
        MATERIALIZATION.contains("let active_use_path = std::fs::canonicalize(&path)"),
        "materialization must canonicalize the bound child only for external active-use probes"
    );
    assert!(
        MATERIALIZATION.contains("observe_path_active_use(&active_use_path)"),
        "materialization active-use probes must not receive the Linux proc namespace path"
    );
}
