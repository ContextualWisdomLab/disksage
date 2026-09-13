//! Read-only Tauri bridge for the immutable presentation resource.
//!
//! Callers choose only an explicit locale and stable screen key. Bundle path, release identity,
//! digest, and SQLite path remain native authority. Resource verification completes before the
//! bounded ledger write transaction, and this bridge never performs locale fallback or consults
//! ontology vocabulary.

use crate::translation_ledger_store::{
    install_current_translation_resource, lookup_translation_message, open_translation_ledger,
};
use crate::translation_resource::{
    current_translation_resource_asset, is_supported_locale, load_current_translation_resource_file,
    valid_screen_key, TranslationResource,
};
use serde::Serialize;

/// Presentation value returned to the frontend after native resource admission.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TranslationMessageView {
    /// Immutable translation release that supplied the projected text.
    pub resource_version: String,
    /// Exact admitted locale requested by presentation code.
    pub locale: String,
    /// Stable presentation key used for this lookup.
    pub screen_key: String,
    /// Localized presentation copy associated with the exact version/locale/key tuple.
    pub text: String,
}

/// Selects one exact locale/key pair without fallback or invented copy.
pub fn resolve_translation_message(
    resource: &TranslationResource,
    locale: &str,
    screen_key: &str,
) -> Result<TranslationMessageView, String> {
    if !is_supported_locale(locale) {
        return Err("translation-locale-unsupported".to_string());
    }
    if !valid_screen_key(screen_key) {
        return Err("translation-screen-key-invalid".to_string());
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

/// Loads, authenticates, persists, and projects one build-owned translation message.
#[cfg(not(coverage))]
#[tauri::command(async)]
pub fn get_translation_message(
    app: tauri::AppHandle,
    locale: String,
    screen_key: String,
) -> Result<TranslationMessageView, String> {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tauri::Manager;

    if !is_supported_locale(&locale) {
        return Err("translation-locale-unsupported".to_string());
    }
    if !valid_screen_key(&screen_key) {
        return Err("translation-screen-key-invalid".to_string());
    }

    let asset = current_translation_resource_asset();
    let resource_path = app
        .path()
        .resolve(asset.relative_path, tauri::path::BaseDirectory::Resource)
        .map_err(|_| "translation-resource-path-resolution-failed".to_string())?;

    // Digest, schema, file identity, and locale/key admission finish before any SQLite write lock.
    let resource = load_current_translation_resource_file(&resource_path)?;

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "translation-ledger-app-data-path-failed".to_string())?;
    fs::create_dir_all(&app_data_dir)
        .map_err(|_| "translation-ledger-app-data-create-failed".to_string())?;
    let database_path = app_data_dir.join("translation-ledger.sqlite3");
    let mut connection = open_translation_ledger(&database_path)?;

    let installed_at_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "translation-ledger-clock-invalid".to_string())?
        .as_millis();
    let installed_at_unix_ms = i64::try_from(installed_at_unix_ms)
        .map_err(|_| "translation-ledger-clock-invalid".to_string())?;

    install_current_translation_resource(&mut connection, &resource, installed_at_unix_ms)?;
    let text = lookup_translation_message(
        &connection,
        asset.resource_version,
        &locale,
        &screen_key,
    )?;

    Ok(TranslationMessageView {
        resource_version: asset.resource_version.to_string(),
        locale,
        screen_key,
        text,
    })
}
