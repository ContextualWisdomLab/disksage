#[path = "../src/bound_read_root.rs"]
mod bound_read_root;

use std::io::Read;
use std::path::Path;

const BOUND_READ_ROOT: &str = include_str!("../src/bound_read_root.rs");
const DUPLICATE_AUDIT: &str = include_str!("../src/duplicate_audit.rs");
const MULTIPART_ARCHIVE: &str = include_str!("../src/multipart_archive.rs");
const INCOMPLETE_DOWNLOAD: &str = include_str!("../src/incomplete_download.rs");
const RECOVERY: &str = include_str!("../src/incomplete_download_recovery.rs");
const MATERIALIZATION: &str = include_str!("../src/incomplete_download_materialization.rs");
const RULES: &str = include_str!("../src/rules.rs");

#[test]
fn shared_root_guard_exposes_identity_bound_contract() {
    for required in [
        "pub(crate) struct BoundReadRoot",
        "pub(crate) fn open",
        "pub(crate) fn canonical_path",
        "pub(crate) fn read_dir_names",
        "pub(crate) fn entry_kind",
        "pub(crate) fn open_file",
    ] {
        assert!(
            BOUND_READ_ROOT.contains(required),
            "bound_read_root.rs missing {required}"
        );
    }
}

#[cfg(unix)]
#[test]
fn bound_root_replacement_cannot_redirect_descriptor_relative_traversal() {
    let parent = tempfile::tempdir().expect("temporary root parent");
    let selected = parent.path().join("selected");
    let moved = parent.path().join("moved");
    std::fs::create_dir(&selected).expect("create selected root");
    std::fs::write(selected.join("marker.txt"), b"original").expect("write original marker");

    let guard = bound_read_root::BoundReadRoot::open(&selected)
        .expect("real directory must bind before the replacement race");

    std::fs::rename(&selected, &moved).expect("move the authorized directory");
    std::fs::create_dir(&selected).expect("install replacement directory");
    std::fs::write(selected.join("marker.txt"), b"replacement")
        .expect("write replacement marker");

    let mut file = guard
        .open_file(Path::new("marker.txt"))
        .expect("descriptor-relative open must retain the authorized directory object");
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)
        .expect("descriptor-relative read must succeed");
    assert_eq!(
        contents, b"original",
        "descriptor-relative traversal must continue to name the authorized directory object"
    );
    assert!(
        guard.canonical_path().is_none(),
        "publication must fail closed after the caller pathname stops naming the bound object"
    );
}

#[test]
fn migrated_read_only_consumers_use_descriptor_relative_child_io() {
    for (name, source, unsafe_error) in [
        (
            "multipart_archive",
            MULTIPART_ARCHIVE,
            "multipart-audit-root-unsafe",
        ),
        ("rules", RULES, ""),
    ] {
        let compact_source = source.split_whitespace().collect::<String>();
        assert!(
            compact_source.contains("root_guard.read_dir_names(")
                || compact_source.contains("self.guard.read_dir_names("),
            "{name} must enumerate children from the opened root descriptor"
        );
        assert!(
            compact_source.contains("root_guard.entry_kind(")
                || compact_source.contains("self.guard.entry_kind("),
            "{name} must inspect child type without following symlinks"
        );
        assert!(
            !compact_source.contains("root_guard.stable_path()"),
            "{name} must not derive child traversal authority from a pathname snapshot"
        );
        if !unsafe_error.is_empty() {
            let final_revalidation_position = compact_source
                .rfind("root_guard.canonical_path()")
                .unwrap_or_else(|| panic!("{name} must revalidate caller-root identity after traversal"));
            assert!(
                compact_source[final_revalidation_position..].contains(unsafe_error),
                "{name} must fail closed with {unsafe_error} after final root revalidation"
            );
        }
    }
}

#[test]
fn not_yet_migrated_consumers_keep_explicit_root_revalidation() {
    assert!(
        DUPLICATE_AUDIT.contains("pub(crate) mod bound_read_root"),
        "duplicate_audit remains the temporary registration point until all legacy consumers migrate"
    );

    for (name, source, unsafe_error) in [
        (
            "duplicate_audit",
            DUPLICATE_AUDIT,
            "duplicate-audit-root-unsafe",
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
        let compact_source = source.split_whitespace().collect::<String>();
        let open_position = compact_source
            .find("BoundReadRoot::open(source_root)")
            .unwrap_or_else(|| panic!("{name} must bind the caller root with a no-follow handle"));
        let stable_position = compact_source
            .find("root_guard.stable_path()")
            .unwrap_or_else(|| panic!("{name} legacy path traversal must be explicit until migrated"));
        assert!(
            stable_position > open_position,
            "{name} must bind the root before obtaining any compatibility pathname"
        );
        let final_revalidation_position = compact_source
            .rfind("root_guard.canonical_path()")
            .unwrap_or_else(|| panic!("{name} must revalidate the caller pathname after traversal"));
        assert!(
            final_revalidation_position > stable_position,
            "{name} must revalidate after compatibility-path traversal"
        );
        assert!(
            compact_source[final_revalidation_position..].contains(unsafe_error),
            "{name} must fail closed with {unsafe_error} after final root revalidation"
        );
        assert!(
            !compact_source.contains("std::fs::canonicalize(source_root)")
                && !compact_source.contains("std::fs::canonicalize(&source_root)"),
            "{name} must not re-resolve the caller root outside the bound-root guard"
        );
    }
}
