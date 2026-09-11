//! Read-only Tauri bridge for the immutable presentation resource.
//!
//! Callers choose only an explicit locale and stable screen key. Bundle path, release identity and
//! digest remain native authority in `translation_resource`, and this bridge never performs locale
//! fallback or consults ontology vocabulary.

use crate::translation_resource::{
    current_translation_resource_asset, load_current_translation_resource_file, TranslationResource,
};
use serde::Serialize;

/// Presentation value returned to the frontend after native resource admission.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TranslationMessageView {
    pub resource_version: String,
    pub locale: String,
    pub screen_key: String,
    pub text: String,
}

/// Selects one exact locale/key pair without fallback or invented copy.
pub fn resolve_translation_message(
    resource: &TranslationResource,
    locale: &str,
    screen_key: &str,
) -> Result<TranslationMessageView, String> {
    let locale_is_supported = resource
        .messages
        .values()
        .any(|localized| localized.contains_key(locale));
    if !locale_is_supported {
        return Err("translation-locale-unsupported".to_string());
    }

    let localized = resource
        .messages
        .get(screen_key)
        .ok_or_else(|| "translation-message-missing".to_string())?;
    let text = localized
        .get(locale)
        .ok_or_else(|| "translation-message-locale-missing".to_string())?;

    Ok(TranslationMessageView {
        resource_version: resource.resource_version.clone(),
        locale: locale.to_string(),
        screen_key: screen_key.to_string(),
        text: text.clone(),
    })
}

/// Loads one message from the compile-time translation asset resolved inside the application bundle.
#[cfg(not(coverage))]
#[tauri::command(async)]
pub fn get_translation_message(
    app: tauri::AppHandle,
    locale: String,
    screen_key: String,
) -> Result<TranslationMessageView, String> {
    use tauri::Manager;

    let asset = current_translation_resource_asset();
    let resource_path = app
        .path()
        .resolve(asset.relative_path, tauri::path::BaseDirectory::Resource)
        .map_err(|_| "translation-resource-path-resolution-failed".to_string())?;
    let resource = load_current_translation_resource_file(&resource_path)?;
    resolve_translation_message(&resource, &locale, &screen_key)
}
