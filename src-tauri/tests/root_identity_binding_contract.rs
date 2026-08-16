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
    std::fs::write(selected.join("original-only.txt"), b"authorized")
        .expect("write authorized-only marker");

    let guard = bound_read_root::BoundReadRoot::open(&selected)
        .expect("real directory must bind before the replacement race");

    std::fs::rename(&selected, &moved).expect("move the authorized directory");
    std::fs::create_dir(&selected).expect("install replacement directory");
    std::fs::write(selected.join("marker.txt"), b"replacement")
        .expect("write replacement marker");
    std::fs::write(selected.join("replacement-only.txt"), b"unauthorized")
        .expect("write replacement-only marker");

    let names = guard
        .read_dir_names(Path::new(""))
        .expect("descriptor-relative enumeration must retain the authorized directory object");
    assert!(
        names.iter().any(|name| name == "marker.txt"),
        "authorized child must remain visible through the bound descriptor"
    );
    assert!(
        names.iter().any(|name| name == "original-only.txt"),
        "children from the original bound directory must remain visible"
    );
    assert!(
        !names.iter().any(|name| name == "replacement-only.txt"),
        "replacement-path children must never enter the bound traversal namespace"
    );
    assert_eq!(
        guard.entry_kind(Path::new("marker.txt")).expect("bound entry kind"),
        bound_read_root::BoundEntryKind::File,
        "child type inspection must remain descriptor-relative after root replacement"
    );

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
fn not_yet_migrated_consumers_keep_ordered_identity_comparison_after_io() {
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
        let canonical_position = compact_source
            .find("letcanonical_root=root_guard.canonical_path()")
            .unwrap_or_else(|| panic!("{name} must capture the initially bound canonical identity"));
        let stable_position = compact_source
            .find("letstable_root=root_guard.stable_path()")
            .unwrap_or_else(|| panic!("{name} legacy path traversal must be explicit until migrated"));
        assert!(
            open_position < canonical_position && canonical_position < stable_position,
            "{name} must bind the root, capture its identity, then derive compatibility I/O authority"
        );

        let final_identity_check = format!(
            "ifroot_guard.canonical_path().as_ref()!=Some(&canonical_root){{returnErr(\"{unsafe_error}\".into());}}"
        );
        let final_revalidation_position = compact_source
            .rfind(&final_identity_check)
            .unwrap_or_else(|| {
                panic!(
                    "{name} must compare the post-traversal bound identity with canonical_root and fail closed with {unsafe_error}"
                )
            });
        assert!(
            final_revalidation_position > stable_position,
            "{name} must perform the identity comparison after compatibility-path traversal"
        );
        assert!(
            compact_source[stable_position..final_revalidation_position].contains("stable_root"),
            "{name} must actually use stable_root for I/O before the final identity comparison"
        );
        assert!(
            !compact_source.contains("std::fs::canonicalize(source_root)")
                && !compact_source.contains("std::fs::canonicalize(&source_root)"),
            "{name} must not re-resolve the caller root outside the bound-root guard"
        );
    }
}
