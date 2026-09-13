#[test]
fn macos_only_orphan_io_imports_stay_platform_scoped() {
    let source = include_str!("../src/orphan.rs");
    assert!(source.contains(
        "#[cfg(target_os = \"macos\")]\nuse std::fs::File;\n#[cfg(target_os = \"macos\")]\nuse std::io::Read;"
    ));
    assert!(!source.contains("use std::fs::File;\nuse std::io::Read;"));
}

#[test]
fn archive_hashers_share_one_digest_trait_import() {
    let source = include_str!("../src/archive_git_tree.rs");
    assert!(source.contains("use sha1::{Digest, Sha1};"));
    assert!(source.contains("use sha2::Sha256;"));
    assert!(!source.contains("Digest as Sha2Digest"));
}
