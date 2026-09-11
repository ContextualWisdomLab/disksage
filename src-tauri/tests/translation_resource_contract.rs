//! Integration contract for the native immutable translation-resource boundary.
//!
//! The presentation ledger is not sufficient by itself: packaged copy must be bound to a fixed
//! resource version, bundle-relative path and digest before any future IPC/UI adapter can consume
//! it. The integration oracle independently hashes the checked-in bundle asset and verifies that the
//! same fixed path is included in Tauri configuration; path-based production loading stays crate-only.

use disksage_lib::translation_resource::{
    current_translation_resource_asset, TranslationResource, CURRENT_TRANSLATION_RESOURCE_VERSION,
};
use sha2::{Digest, Sha256};
use std::path::Path;

#[test]
fn current_translation_resource_is_version_path_digest_and_bundle_bound() {
    let asset = current_translation_resource_asset();

    assert_eq!(
        asset.resource_version,
        CURRENT_TRANSLATION_RESOURCE_VERSION
    );
    assert_eq!(
        asset.relative_path,
        "resources/translation/releases/2026.09.11.1.json"
    );
    assert_eq!(asset.sha256.len(), 64);
    assert!(asset.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert!(asset.sha256.bytes().all(|byte| !byte.is_ascii_uppercase()));

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let resource_bytes =
        std::fs::read(manifest_dir.join(asset.relative_path)).expect("read bundled resource fixture");
    let digest = Sha256::digest(resource_bytes);
    let digest_hex = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(digest_hex, asset.sha256);

    let config: serde_json::Value = serde_json::from_slice(
        &std::fs::read(manifest_dir.join("tauri.conf.json")).expect("read Tauri configuration"),
    )
    .expect("Tauri configuration must be JSON");
    let resources = config["bundle"]["resources"]
        .as_array()
        .expect("Tauri bundle resources must be an array");
    assert!(
        resources.iter().any(|entry| entry.as_str() == Some(asset.relative_path)),
        "the digest-bound translation resource must be included in the application bundle"
    );
}

#[test]
fn schema_v1_rejects_unknown_top_level_fields() {
    let asset = current_translation_resource_asset();
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let resource_bytes =
        std::fs::read(manifest_dir.join(asset.relative_path)).expect("read bundled resource fixture");
    let mut candidate: serde_json::Value =
        serde_json::from_slice(&resource_bytes).expect("resource fixture must be JSON");
    candidate
        .as_object_mut()
        .expect("translation resource must be an object")
        .insert(
            "future_semantics_without_schema_bump".to_string(),
            serde_json::Value::Bool(true),
        );

    let parsed = serde_json::from_value::<TranslationResource>(candidate);
    assert!(
        parsed.is_err(),
        "schema_version=1 must reject undeclared top-level semantics rather than silently discard them"
    );
}
