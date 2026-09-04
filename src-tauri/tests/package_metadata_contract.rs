//! Regression contract for buyer-visible Rust package metadata.
//!
//! DiskSage is distributed as a desktop product rather than a crates.io library. The Cargo
//! manifest still forms part of acquisition, SBOM, provenance, and support evidence, so it must
//! identify the product without generator placeholders or deprecated attribution fields and must
//! refuse accidental registry publication.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Reads the authoritative Cargo manifest from the crate root without depending on process CWD.
fn cargo_manifest() -> String {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    fs::read_to_string(manifest_path).expect("Cargo.toml must be readable for metadata validation")
}

/// Asks Cargo to parse one manifest and returns its registry-publication policy.
///
/// Cargo's versioned metadata format represents unrestricted publication as `null`, complete
/// publication refusal (`publish = false`) as an empty array, and an allowlist as a non-empty
/// array. Consulting Cargo itself prevents comments, strings, or unrelated TOML tables from
/// masquerading as the authoritative `[package].publish` value.
fn cargo_publish_policy(manifest_path: &Path) -> Option<Vec<String>> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .arg("metadata")
        .arg("--format-version")
        .arg("1")
        .arg("--no-deps")
        .arg("--manifest-path")
        .arg(manifest_path)
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .expect("cargo metadata must execute for publication-policy validation");

    assert!(
        output.status.success(),
        "cargo metadata must parse the manifest successfully: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("cargo metadata must emit valid JSON format version 1");
    let canonical_manifest = fs::canonicalize(manifest_path)
        .expect("manifest path must canonicalize for exact package selection");
    let packages = metadata["packages"]
        .as_array()
        .expect("cargo metadata packages must be an array");
    let package = packages
        .iter()
        .find(|package| {
            package["manifest_path"]
                .as_str()
                .map(Path::new)
                .and_then(|path| fs::canonicalize(path).ok())
                .is_some_and(|path| path == canonical_manifest)
        })
        .expect("cargo metadata must contain the package for the requested manifest");

    match &package["publish"] {
        serde_json::Value::Null => None,
        serde_json::Value::Array(registries) => Some(
            registries
                .iter()
                .map(|registry| {
                    registry
                        .as_str()
                        .expect("Cargo publication allowlist entries must be strings")
                        .to_owned()
                })
                .collect(),
        ),
        unexpected => panic!("Cargo publication metadata has unexpected shape: {unexpected}"),
    }
}

#[test]
fn cargo_exposes_expected_acquisition_metadata_to_build_consumers() {
    assert_eq!(
        env!("CARGO_PKG_NAME"),
        "disksage",
        "the Cargo package name is part of DiskSage's acquisition and provenance identity"
    );
    assert_eq!(
        env!("CARGO_PKG_DESCRIPTION"),
        "Privacy-first desktop storage analysis and reclaim decision-support application."
    );
    assert_eq!(
        env!("CARGO_PKG_AUTHORS"),
        "",
        "Cargo's deprecated authors field must stay absent; ownership is established by repository, license, provenance, and release authority"
    );
    assert_eq!(env!("CARGO_PKG_LICENSE"), "MIT");
    assert_eq!(
        env!("CARGO_PKG_REPOSITORY"),
        "https://github.com/ContextualWisdomLab/disksage"
    );
}

#[test]
fn manifest_refuses_placeholders_deprecated_authors_and_registry_publication() {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let manifest = cargo_manifest();

    assert_eq!(
        cargo_publish_policy(&manifest_path),
        Some(Vec::new()),
        "Cargo's parsed package metadata must forbid every registry, not merely contain publish = false text"
    );
    assert!(
        !manifest
            .lines()
            .any(|line| line.trim_start().starts_with("authors =")),
        "Cargo's deprecated authors field must not be reintroduced"
    );

    for placeholder in ["description = \"A Tauri App\"", "authors = [\"you\"]"] {
        assert!(
            !manifest.contains(placeholder),
            "Cargo package metadata still contains generator placeholder: {placeholder}"
        );
    }
}

#[test]
fn commented_or_out_of_table_publish_text_cannot_fake_the_registry_guard() {
    let temporary_package = tempfile::tempdir().expect("temporary Cargo package must be created");
    let manifest_path = temporary_package.path().join("Cargo.toml");
    let source_dir = temporary_package.path().join("src");
    fs::create_dir(&source_dir).expect("temporary Cargo source directory must be created");
    fs::write(source_dir.join("lib.rs"), "pub fn marker() {}\n")
        .expect("temporary Cargo source must be written");
    fs::write(
        &manifest_path,
        r#"[package]
name = "publish-decoy"
version = "0.1.0"
edition = "2021"
# publish = false

[package.metadata.guard]
note = "publish = false"
"#,
    )
    .expect("temporary Cargo manifest must be written");

    assert_eq!(
        cargo_publish_policy(&manifest_path),
        None,
        "commented or unrelated publish text must remain unrestricted in Cargo metadata and therefore fail the DiskSage guard"
    );
}

#[test]
fn tauri_bin_directory_contains_only_rust_sources() {
    let bin_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/bin");
    for entry in fs::read_dir(&bin_dir).expect("Tauri bin directory must be readable") {
        let path = entry
            .expect("Tauri bin directory entries must be readable")
            .path();
        assert!(
            path.is_file() && path.extension().is_some_and(|extension| extension == "rs"),
            "Tauri scans every src/bin entry as a binary; keep non-Rust source fragments outside it: {}",
            path.display()
        );
    }
}

#[test]
fn colima_execution_target_is_opt_in_for_macos_packaging_only() {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .arg("metadata")
        .arg("--format-version")
        .arg("1")
        .arg("--no-deps")
        .arg("--manifest-path")
        .arg(&manifest_path)
        .output()
        .expect("cargo metadata must execute for target validation");
    assert!(output.status.success());
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let target = metadata["packages"][0]["targets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|target| target["name"] == "disksage-colima-disk-reclaim")
        .expect("Colima target must remain declared for supported macOS packaging");
    assert_eq!(
        target["required-features"],
        serde_json::json!(["colima-macos-cli"]),
        "default Windows and Linux package builds must not expose the macOS-only executable"
    );

    let workflow = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.github/workflows/release.yml"),
    )
    .expect("release workflow must be readable");
    assert_eq!(workflow.matches("tauri_features: llm-engine\n").count(), 2);
    assert_eq!(
        workflow
            .matches("tauri_features: llm-engine,colima-macos-cli\n")
            .count(),
        1,
        "only the macOS release matrix entry may opt into the Colima executable"
    );
    assert!(workflow.contains("if: matrix.os == 'macos-latest'\n        run: cargo check --manifest-path src-tauri/Cargo.toml --features colima-macos-cli --bin disksage-colima-disk-reclaim"));
}
