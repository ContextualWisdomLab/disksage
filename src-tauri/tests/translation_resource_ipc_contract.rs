//! Contract for the read-only native translation projection owned by issue #397.
//!
//! The UI may choose only an explicit locale and stable screen key. Resource path, digest and
//! version remain native authority, and missing data must fail rather than falling back silently.

use disksage_lib::translation_resource::TranslationResource;
use disksage_lib::translation_resource_bridge::resolve_translation_message;
use std::collections::BTreeMap;

fn resource() -> TranslationResource {
    let scan = BTreeMap::from([
        ("ko".to_string(), "스캔".to_string()),
        ("en".to_string(), "Scan".to_string()),
        ("ja".to_string(), "スキャン".to_string()),
        ("zh".to_string(), "扫描".to_string()),
        ("vi".to_string(), "Quét".to_string()),
        ("es".to_string(), "Escanear".to_string()),
        ("de".to_string(), "Scannen".to_string()),
        ("fr".to_string(), "Analyser".to_string()),
    ]);

    TranslationResource {
        resource_version: "2026.09.11.1".to_string(),
        schema_version: 1,
        messages: BTreeMap::from([("app.action.scan".to_string(), scan)]),
    }
}

#[test]
fn exact_locale_and_screen_key_are_projected_without_extra_authority() {
    let view = resolve_translation_message(&resource(), "fr", "app.action.scan")
        .expect("known locale and screen key must resolve");

    assert_eq!(view.resource_version, "2026.09.11.1");
    assert_eq!(view.locale, "fr");
    assert_eq!(view.screen_key, "app.action.scan");
    assert_eq!(view.text, "Analyser");
}

#[test]
fn unsupported_locale_fails_instead_of_falling_back() {
    assert_eq!(
        resolve_translation_message(&resource(), "it", "app.action.scan"),
        Err("translation-locale-unsupported".to_string())
    );
}

#[test]
fn missing_screen_key_fails_instead_of_inventing_copy() {
    assert_eq!(
        resolve_translation_message(&resource(), "en", "app.action.missing"),
        Err("translation-message-missing".to_string())
    );
}

#[test]
fn missing_locale_in_an_admitted_shape_is_fail_closed() {
    let mut incomplete = resource();
    incomplete
        .messages
        .get_mut("app.action.scan")
        .expect("scan fixture")
        .remove("fr");

    assert_eq!(
        resolve_translation_message(&incomplete, "fr", "app.action.scan"),
        Err("translation-message-locale-missing".to_string())
    );
}
