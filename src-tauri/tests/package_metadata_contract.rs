//! Regression contract for buyer-visible Rust package metadata.
//!
//! DiskSage is distributed as a desktop product rather than a crates.io library. The Cargo
//! manifest still forms part of acquisition, SBOM, provenance, and support evidence, so it must
//! identify the product and owner without generator placeholders and must refuse accidental
//! registry publication.

use std::fs;
use std::path::PathBuf;

/// Reads the authoritative Cargo manifest from the crate root without depending on process CWD.
fn cargo_manifest() -> String {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    fs::read_to_string(manifest_path).expect("Cargo.toml must be readable for metadata validation")
}

#[test]
fn cargo_exposes_expected_acquisition_metadata_to_build_consumers() {
    assert_eq!(
        env!("CARGO_PKG_DESCRIPTION"),
        "Privacy-first desktop storage analysis and reclaim decision-support application."
    );
    assert_eq!(env!("CARGO_PKG_AUTHORS"), "Contextual Wisdom Lab");
    assert_eq!(env!("CARGO_PKG_LICENSE"), "MIT");
    assert_eq!(
        env!("CARGO_PKG_REPOSITORY"),
        "https://github.com/ContextualWisdomLab/disksage"
    );
}

#[test]
fn manifest_refuses_placeholders_and_registry_publication() {
    let manifest = cargo_manifest();

    assert!(
        manifest.contains("publish = false"),
        "Cargo registry publication must remain explicitly disabled for the desktop package"
    );

    for placeholder in ["description = \"A Tauri App\"", "authors = [\"you\"]"] {
        assert!(
            !manifest.contains(placeholder),
            "Cargo package metadata still contains generator placeholder: {placeholder}"
        );
    }
}
