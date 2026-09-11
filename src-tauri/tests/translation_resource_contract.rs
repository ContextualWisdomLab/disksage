//! Contract for the native immutable translation-resource boundary.
//!
//! The presentation ledger is not sufficient by itself: packaged copy must be bound to a fixed
//! resource version, bundle-relative path and digest before any future IPC/UI adapter can consume
//! it. This integration test intentionally lands before the production module for a compile RED.

use disksage_lib::translation_resource::{
    current_translation_resource_asset, CURRENT_TRANSLATION_RESOURCE_VERSION,
};

#[test]
fn current_translation_resource_is_version_path_and_digest_bound() {
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
}
