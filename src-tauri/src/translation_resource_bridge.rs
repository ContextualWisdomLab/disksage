//! Tauri bridge for immutable presentation resources and their admitted local ledger.
//!
//! Bundle path, release identity, digest, and SQLite path remain native authority. Resource
//! verification plus append-only SQLite installation complete once during application setup;
//! steady-state IPC accepts only an explicit locale and stable screen key, performs no locale
//! fallback, and never consults ontology vocabulary.

use crate::translation_ledger_store::{
    install_current_translation_resource, lookup_translation_message, open_translation_ledger,
};
use crate::translation_resource::{
    current_translation_resource_asset, is_supported_locale,
    load_current_translation_resource_file, valid_screen_key, TranslationResource,
};
use rusqlite::Connection;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

const MAX_TRANSLATION_MESSAGE_CACHE_ENTRIES: usize = 256;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct TranslationMessageCacheKey {
    resource_version: String,
    locale: String,
    screen_key: String,
}

#[derive(Debug, Default)]
struct TranslationMessageCache {
    entries: HashMap<TranslationMessageCacheKey, String>,
    insertion_order: VecDeque<TranslationMessageCacheKey>,
}

impl TranslationMessageCache {
    fn get(&self, key: &TranslationMessageCacheKey) -> Option<String> {
        self.entries.get(key).cloned()
    }

    fn insert(&mut self, key: TranslationMessageCacheKey, text: String) {
        if self.entries.contains_key(&key) {
            return;
        }
        if self.entries.len() >= MAX_TRANSLATION_MESSAGE_CACHE_ENTRIES {
            if let Some(oldest) = self.insertion_order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
        self.insertion_order.push_back(key.clone());
        self.entries.insert(key, text);
    }
}

/// Process-lifetime translation state admitted once during Tauri setup.
pub struct TranslationLedgerRuntime {
    resource_version: String,
    connection: Mutex<Connection>,
    cache: Mutex<TranslationMessageCache>,
}

impl TranslationLedgerRuntime {
    fn new(resource_version: String, connection: Connection) -> Self {
        Self {
            resource_version,
            connection: Mutex::new(connection),
            cache: Mutex::new(TranslationMessageCache::default()),
        }
    }
}

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

/// Loads, authenticates, and persists the build-owned translation release once at startup.
#[cfg(not(coverage))]
pub fn initialize_translation_ledger(
    app: &tauri::AppHandle,
) -> Result<TranslationLedgerRuntime, String> {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tauri::Manager;

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
    Ok(TranslationLedgerRuntime::new(
        asset.resource_version.to_string(),
        connection,
    ))
}

/// Projects one exact translation tuple from bounded process state and the admitted ledger.
#[cfg(not(coverage))]
#[tauri::command(async)]
pub fn get_translation_message(
    runtime: tauri::State<'_, TranslationLedgerRuntime>,
    locale: String,
    screen_key: String,
) -> Result<TranslationMessageView, String> {
    let runtime: &TranslationLedgerRuntime = runtime.inner();
    if !is_supported_locale(&locale) {
        return Err("translation-locale-unsupported".to_string());
    }
    if !valid_screen_key(&screen_key) {
        return Err("translation-screen-key-invalid".to_string());
    }

    let cache_key = TranslationMessageCacheKey {
        resource_version: runtime.resource_version.clone(),
        locale: locale.clone(),
        screen_key: screen_key.clone(),
    };
    let cached = runtime
        .cache
        .lock()
        .map_err(|_| "translation-cache-lock-failed".to_string())?
        .get(&cache_key);
    if let Some(text) = cached {
        return Ok(TranslationMessageView {
            resource_version: runtime.resource_version.clone(),
            locale,
            screen_key,
            text,
        });
    }

    // The cache mutex is not held while SQLite performs the exact tuple lookup.
    let text = {
        let connection = runtime
            .connection
            .lock()
            .map_err(|_| "translation-ledger-lock-failed".to_string())?;
        lookup_translation_message(&connection, &runtime.resource_version, &locale, &screen_key)?
    };
    runtime
        .cache
        .lock()
        .map_err(|_| "translation-cache-lock-failed".to_string())?
        .insert(cache_key, text.clone());

    Ok(TranslationMessageView {
        resource_version: runtime.resource_version.clone(),
        locale,
        screen_key,
        text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache_key(
        resource_version: &str,
        locale: &str,
        screen_key: &str,
    ) -> TranslationMessageCacheKey {
        TranslationMessageCacheKey {
            resource_version: resource_version.to_string(),
            locale: locale.to_string(),
            screen_key: screen_key.to_string(),
        }
    }

    #[test]
    fn cache_identity_includes_version_locale_and_screen_key() {
        let mut cache = TranslationMessageCache::default();
        let canonical = cache_key("2026.09.11.1", "en", "app.action.scan");
        cache.insert(canonical.clone(), "Scan".to_string());

        assert_eq!(cache.get(&canonical).as_deref(), Some("Scan"));
        assert_eq!(
            cache.get(&cache_key("2026.09.11.2", "en", "app.action.scan")),
            None
        );
        assert_eq!(
            cache.get(&cache_key("2026.09.11.1", "ko", "app.action.scan")),
            None
        );
        assert_eq!(
            cache.get(&cache_key("2026.09.11.1", "en", "app.action.cancel")),
            None
        );
    }

    #[test]
    fn cache_is_bounded_and_preserves_immutable_tuple_value() {
        let mut cache = TranslationMessageCache::default();
        let first = cache_key("2026.09.11.1", "en", "screen.000");
        cache.insert(first.clone(), "first".to_string());
        cache.insert(first.clone(), "mutated".to_string());
        assert_eq!(cache.get(&first).as_deref(), Some("first"));

        for index in 1..MAX_TRANSLATION_MESSAGE_CACHE_ENTRIES {
            let key = cache_key(
                "2026.09.11.1",
                "en",
                &format!("screen.{index:03}"),
            );
            cache.insert(key, index.to_string());
        }
        assert_eq!(cache.entries.len(), MAX_TRANSLATION_MESSAGE_CACHE_ENTRIES);
        assert_eq!(
            cache.insertion_order.len(),
            MAX_TRANSLATION_MESSAGE_CACHE_ENTRIES
        );

        let overflow = cache_key("2026.09.11.1", "en", "screen.overflow");
        cache.insert(overflow.clone(), "overflow".to_string());
        assert_eq!(cache.entries.len(), MAX_TRANSLATION_MESSAGE_CACHE_ENTRIES);
        assert_eq!(
            cache.insertion_order.len(),
            MAX_TRANSLATION_MESSAGE_CACHE_ENTRIES
        );
        assert_eq!(cache.get(&first), None);
        assert_eq!(cache.get(&overflow).as_deref(), Some("overflow"));
    }
}
