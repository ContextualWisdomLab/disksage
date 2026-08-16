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

#[cfg(unix)]
#[test]
fn bound_root_replacement_cannot_redirect_the_retained_traversal_namespace() {
    let parent = tempfile::tempdir().expect("temporary root parent");
    let selected = parent.path().join("selected");
    let moved = parent.path().join("moved");
    std::fs::create_dir(&selected).expect("create selected root");
    std::fs::write(selected.join("marker.txt"), b"original").expect("write original marker");

    let guard = bound_read_root::BoundReadRoot::open(&selected)
        .expect("real directory must bind before the replacement race");
    let stable = guard
        .stable_path()
        .expect("bound root must expose a traversal namespace");

    std::fs::rename(&selected, &moved).expect("move the authorized directory");
    std::fs::create_dir(&selected).expect("install replacement directory");
    std::fs::write(selected.join("marker.txt"), b"replacement")
        .expect("write replacement marker");

    assert_eq!(
        std::fs::read(stable.join("marker.txt")).expect("read through retained namespace"),
        b"original",
        "the retained traversal namespace must continue to name the authorized directory object"
    );
    assert!(
        guard.canonical_path().is_none(),
        "publication must fail closed after the caller pathname stops naming the bound object"
    );
}

#[test]
fn public_read_only_roots_bind_traversal_to_open_directory_identity() {
    assert!(
        DUPLICATE_AUDIT.contains("pub(crate) mod bound_read_root"),
        "duplicate_audit must register the shared bound-root module without touching lib.rs"
    );

    for (name, source, unsafe_error) in [
        (
            "duplicate_audit",
            DUPLICATE_AUDIT,
            "duplicate-audit-root-unsafe",
        ),
        (
            "multipart_archive",
            MULTIPART_ARCHIVE,
            "multipart-audit-root-unsafe",
        ),
        (
            "incomplete_download",
            INCOMPLETE_DOWNLOAD,
            "incomplete-download-audit-root-unsafe",
        ),
        (
            "incomplete_download_recovery",
            RECOVERY,
            "recovery-validation-root-unsafe",
        ),
        (
            "incomplete_download_materialization",
            MATERIALIZATION,
            "materialization-root-unsafe",
        ),
    ] {
        // Formatting-only rustfmt changes must not turn a valid identity-binding implementation RED.
        // Compact whitespace while retaining authority-bearing call and diagnostic ordering.
        let compact_source = source.split_whitespace().collect::<String>();
        let open_position = compact_source
            .find("BoundReadRoot::open(source_root)")
            .unwrap_or_else(|| panic!("{name} must bind the caller root with a no-follow handle"));
        let stable_position = compact_source
            .find("root_guard.stable_path()")
            .unwrap_or_else(|| panic!("{name} must obtain its I/O root from the bound handle"));
        assert!(
            stable_position > open_position,
            "{name} must bind the root before obtaining the traversal namespace"
        );

        let post_traversal = &compact_source[stable_position + "root_guard.stable_path()".len()..];
        let final_revalidation_position = post_traversal
            .find("root_guard.canonical_path()")
            .unwrap_or_else(|| {
                panic!("{name} must revalidate the caller pathname after traversal")
            });
        let final_revalidation_tail = &post_traversal[final_revalidation_position..];
        let unsafe_error_position = final_revalidation_tail
            .find(unsafe_error)
            .unwrap_or_else(|| {
                panic!("{name} must fail closed with {unsafe_error} after final root revalidation")
            });
        assert!(
            unsafe_error_position < 512,
            "{name} must bind the final root-identity mismatch directly to its fail-closed diagnostic"
        );

        assert!(
            !compact_source.contains("std::fs::canonicalize(source_root)")
                && !compact_source.contains("std::fs::canonicalize(&source_root)"),
            "{name} must not re-resolve the caller root outside the bound-root guard"
        );
    }
}
