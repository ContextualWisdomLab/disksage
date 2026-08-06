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
fn acquisition_metadata_is_complete_and_non_placeholder() {
    let manifest = cargo_manifest();

    for required in [
        "description = \"Privacy-first desktop storage analysis and reclaim decision-support application.\"",
        "authors = [\"Contextual Wisdom Lab\"]",
        "license = \"MIT\"",
        "repository = \"https://github.com/ContextualWisdomLab/disksage\"",
        "publish = false",
    ] {
        assert!(
            manifest.contains(required),
            "Cargo package metadata is missing required acquisition field: {required}"
        );
    }

    for placeholder in ["description = \"A Tauri App\"", "authors = [\"you\"]"] {
        assert!(
            !manifest.contains(placeholder),
            "Cargo package metadata still contains generator placeholder: {placeholder}"
        );
    }
}
