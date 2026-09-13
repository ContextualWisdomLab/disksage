//! Native SQLite persistence for admitted presentation resources.
//!
//! Resource authentication stays in `translation_resource`. This module receives only an already
//! admitted in-memory resource from the native bridge, derives bounded presentation metadata before
//! opening a write transaction, and persists one immutable release atomically. It owns no ontology,
//! filesystem-classification, deletion, network, or LLM authority.

use crate::translation_resource::{
    current_translation_resource_asset, is_supported_locale, valid_screen_key, TranslationResource,
};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use std::path::Path;

const TRANSLATION_LEDGER_MIGRATION: &str =
    include_str!("../resources/translation/0001_translation_ledger.sql");
const CURRENT_SCHEMA_VERSION: u32 = 1;
const MAX_SCREEN_AREA_BYTES: usize = 80;

/// Opens the local presentation ledger and installs its immutable schema when absent.
pub(crate) fn open_translation_ledger(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open(path).map_err(|_| "translation-ledger-open-failed".to_string())?;
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|_| "translation-ledger-foreign-keys-failed".to_string())?;

    let resource_table_exists = sqlite_object_exists(&connection, "table", "translation_resource_versions")?;
    if !resource_table_exists {
        connection
            .execute_batch(TRANSLATION_LEDGER_MIGRATION)
            .map_err(|_| "translation-ledger-migration-failed".to_string())?;
    }
    verify_schema_objects(&connection)?;
    Ok(connection)
}

/// Installs the build-pinned resource as one immutable release.
///
/// The caller must pass the resource returned by the canonical native admission reader. All
/// potentially expensive file verification happens before this function is called. This function
/// performs only bounded in-memory validation and local SQLite reads/writes.
pub(crate) fn install_current_translation_resource(
    connection: &mut Connection,
    resource: &TranslationResource,
    installed_at_unix_ms: i64,
) -> Result<(), String> {
    if installed_at_unix_ms < 0 {
        return Err("translation-ledger-install-time-invalid".to_string());
    }

    let asset = current_translation_resource_asset();
    if resource.resource_version != asset.resource_version || resource.schema_version != CURRENT_SCHEMA_VERSION {
        return Err("translation-ledger-resource-identity-mismatch".to_string());
    }

    let prepared = prepare_resource(resource)?;
    if existing_release_matches(connection, resource, asset.sha256, &prepared)? {
        return Ok(());
    }

    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|_| "translation-ledger-transaction-start-failed".to_string())?;

    // Another process may have completed the same immutable install before this writer acquired the
    // SQLite write lock. Re-check only local ledger state after lock acquisition; no file or network
    // I/O occurs inside the transaction.
    if existing_release_matches(&transaction, resource, asset.sha256, &prepared)? {
        transaction
            .commit()
            .map_err(|_| "translation-ledger-transaction-commit-failed".to_string())?;
        return Ok(());
    }

    insert_release(
        &transaction,
        resource,
        asset.sha256,
        installed_at_unix_ms,
        &prepared,
    )?;
    transaction
        .commit()
        .map_err(|_| "translation-ledger-transaction-commit-failed".to_string())
}

/// Reads one exact immutable version/locale/screen-key tuple without locale fallback.
pub(crate) fn lookup_translation_message(
    connection: &Connection,
    resource_version: &str,
    locale: &str,
    screen_key: &str,
) -> Result<String, String> {
    if resource_version.is_empty() || resource_version.len() > 128 {
        return Err("translation-resource-version-invalid".to_string());
    }
    if !is_supported_locale(locale) {
        return Err("translation-locale-unsupported".to_string());
    }
    if !valid_screen_key(screen_key) {
        return Err("translation-screen-key-invalid".to_string());
    }

    connection
        .query_row(
            "SELECT text_value FROM translation_messages \
             WHERE resource_version = ?1 AND locale = ?2 AND screen_key = ?3",
            params![resource_version, locale, screen_key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| "translation-ledger-lookup-failed".to_string())?
        .ok_or_else(|| "translation-message-missing".to_string())
}

#[derive(Debug)]
struct PreparedScreenKey<'a> {
    screen_key: &'a str,
    screen_area: String,
    localized: &'a std::collections::BTreeMap<String, String>,
}

fn prepare_resource(resource: &TranslationResource) -> Result<Vec<PreparedScreenKey<'_>>, String> {
    if resource.messages.is_empty() {
        return Err("translation-ledger-resource-empty".to_string());
    }

    resource
        .messages
        .iter()
        .map(|(screen_key, localized)| {
            if !valid_screen_key(screen_key) {
                return Err("translation-screen-key-invalid".to_string());
            }
            if localized.len() != 8 || localized.keys().any(|locale| !is_supported_locale(locale)) {
                return Err("translation-resource-locale-set-incomplete".to_string());
            }
            let screen_area = screen_area_for_key(screen_key)?;
            Ok(PreparedScreenKey {
                screen_key,
                screen_area,
                localized,
            })
        })
        .collect()
}

fn screen_area_for_key(screen_key: &str) -> Result<String, String> {
    let (screen_area, _) = screen_key
        .rsplit_once('.')
        .ok_or_else(|| "translation-screen-area-invalid".to_string())?;
    if screen_area.is_empty()
        || screen_area.len() > MAX_SCREEN_AREA_BYTES
        || !screen_area.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b'_' | b'-')
        })
    {
        return Err("translation-screen-area-invalid".to_string());
    }
    Ok(screen_area.to_string())
}

fn existing_release_matches(
    connection: &Connection,
    resource: &TranslationResource,
    content_sha256: &str,
    prepared: &[PreparedScreenKey<'_>],
) -> Result<bool, String> {
    let existing: Option<(u32, String)> = connection
        .query_row(
            "SELECT schema_version, content_sha256 FROM translation_resource_versions \
             WHERE resource_version = ?1",
            params![resource.resource_version],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|_| "translation-ledger-existing-version-read-failed".to_string())?;

    let Some((schema_version, existing_sha256)) = existing else {
        return Ok(false);
    };
    if schema_version != resource.schema_version || existing_sha256 != content_sha256 {
        return Err("translation-ledger-existing-version-mismatch".to_string());
    }

    let expected_message_count: i64 = prepared
        .iter()
        .map(|entry| entry.localized.len() as i64)
        .sum();
    let stored_message_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM translation_messages WHERE resource_version = ?1",
            params![resource.resource_version],
            |row| row.get(0),
        )
        .map_err(|_| "translation-ledger-existing-version-read-failed".to_string())?;
    if stored_message_count != expected_message_count {
        return Err("translation-ledger-existing-version-mismatch".to_string());
    }

    for entry in prepared {
        let stored_area: Option<String> = connection
            .query_row(
                "SELECT screen_area FROM translation_screen_keys WHERE screen_key = ?1",
                params![entry.screen_key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| "translation-ledger-existing-version-read-failed".to_string())?;
        if stored_area.as_deref() != Some(entry.screen_area.as_str()) {
            return Err("translation-ledger-existing-version-mismatch".to_string());
        }

        for (locale, expected_text) in entry.localized {
            let stored_text: Option<String> = connection
                .query_row(
                    "SELECT text_value FROM translation_messages \
                     WHERE resource_version = ?1 AND locale = ?2 AND screen_key = ?3",
                    params![resource.resource_version, locale, entry.screen_key],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|_| "translation-ledger-existing-version-read-failed".to_string())?;
            if stored_text.as_deref() != Some(expected_text.as_str()) {
                return Err("translation-ledger-existing-version-mismatch".to_string());
            }
        }
    }

    Ok(true)
}

fn insert_release(
    transaction: &Transaction<'_>,
    resource: &TranslationResource,
    content_sha256: &str,
    installed_at_unix_ms: i64,
    prepared: &[PreparedScreenKey<'_>],
) -> Result<(), String> {
    transaction
        .execute(
            "INSERT INTO translation_resource_versions \
             (resource_version, schema_version, created_at_unix_ms, content_sha256) \
             VALUES (?1, ?2, ?3, ?4)",
            params![
                resource.resource_version,
                resource.schema_version,
                installed_at_unix_ms,
                content_sha256
            ],
        )
        .map_err(|_| "translation-ledger-version-insert-failed".to_string())?;

    for entry in prepared {
        let stored_area: Option<String> = transaction
            .query_row(
                "SELECT screen_area FROM translation_screen_keys WHERE screen_key = ?1",
                params![entry.screen_key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| "translation-ledger-screen-key-read-failed".to_string())?;
        match stored_area {
            Some(existing) if existing != entry.screen_area => {
                return Err("translation-ledger-screen-area-mismatch".to_string())
            }
            Some(_) => {}
            None => {
                transaction
                    .execute(
                        "INSERT INTO translation_screen_keys (screen_key, screen_area) VALUES (?1, ?2)",
                        params![entry.screen_key, entry.screen_area],
                    )
                    .map_err(|_| "translation-ledger-screen-key-insert-failed".to_string())?;
            }
        }

        for (locale, text) in entry.localized {
            transaction
                .execute(
                    "INSERT INTO translation_messages \
                     (resource_version, locale, screen_key, text_value) VALUES (?1, ?2, ?3, ?4)",
                    params![resource.resource_version, locale, entry.screen_key, text],
                )
                .map_err(|_| "translation-ledger-message-insert-failed".to_string())?;
        }
    }
    Ok(())
}

fn sqlite_object_exists(connection: &Connection, kind: &str, name: &str) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = ?1 AND name = ?2",
            params![kind, name],
            |_| Ok(()),
        )
        .optional()
        .map(|value| value.is_some())
        .map_err(|_| "translation-ledger-schema-read-failed".to_string())
}

fn verify_schema_objects(connection: &Connection) -> Result<(), String> {
    for (kind, name) in [
        ("table", "translation_resource_versions"),
        ("table", "translation_screen_keys"),
        ("table", "translation_messages"),
        ("trigger", "translation_resource_versions_no_update"),
        ("trigger", "translation_resource_versions_no_delete"),
        ("trigger", "translation_screen_keys_no_update"),
        ("trigger", "translation_screen_keys_no_delete"),
        ("trigger", "translation_messages_no_update"),
        ("trigger", "translation_messages_no_delete"),
    ] {
        if !sqlite_object_exists(connection, kind, name)? {
            return Err("translation-ledger-schema-incomplete".to_string());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checked_in_resource() -> TranslationResource {
        serde_json::from_slice(include_bytes!(
            "../resources/translation/releases/2026.09.11.1.json"
        ))
        .expect("checked-in translation resource JSON")
    }

    #[test]
    fn native_ledger_installs_idempotently_and_serves_exact_lookup() {
        let directory = tempfile::tempdir().expect("temporary ledger directory");
        let database_path = directory.path().join("translation-ledger.sqlite3");
        let mut connection = open_translation_ledger(&database_path).expect("open native ledger");
        let resource = checked_in_resource();

        install_current_translation_resource(&mut connection, &resource, 1_789_344_000_000)
            .expect("first install");
        install_current_translation_resource(&mut connection, &resource, 1_789_344_000_001)
            .expect("idempotent install");

        assert_eq!(
            lookup_translation_message(
                &connection,
                current_translation_resource_asset().resource_version,
                "ko",
                "app.action.scan",
            )
            .expect("persisted exact lookup"),
            "스캔"
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM translation_resource_versions", [], |row| row.get::<_, i64>(0))
                .expect("version count"),
            1
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT screen_area FROM translation_screen_keys WHERE screen_key = 'app.action.scan'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("screen area"),
            "app.action"
        );
    }

    #[test]
    fn existing_release_content_cannot_mutate_in_place() {
        let directory = tempfile::tempdir().expect("temporary ledger directory");
        let mut connection = open_translation_ledger(&directory.path().join("ledger.sqlite3"))
            .expect("open native ledger");
        let resource = checked_in_resource();
        install_current_translation_resource(&mut connection, &resource, 1)
            .expect("initial install");

        let mut changed = resource.clone();
        changed
            .messages
            .get_mut("app.action.scan")
            .expect("scan message")
            .insert("ko".to_string(), "변조된 스캔".to_string());
        assert_eq!(
            install_current_translation_resource(&mut connection, &changed, 2),
            Err("translation-ledger-existing-version-mismatch".to_string())
        );
    }

    #[test]
    fn screen_area_is_namespace_before_the_final_key_segment() {
        assert_eq!(screen_area_for_key("app.action.scan"), Ok("app.action".to_string()));
        assert_eq!(
            screen_area_for_key("no_namespace"),
            Err("translation-screen-area-invalid".to_string())
        );
        assert_eq!(
            screen_area_for_key(&format!("{}.message", "a".repeat(81))),
            Err("translation-screen-area-invalid".to_string())
        );
    }

    #[cfg(unix)]
    #[test]
    fn native_ledger_rejects_symlink_database_path() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().expect("temporary ledger directory");
        let target = directory.path().join("target.sqlite3");
        std::fs::File::create(&target).expect("target file");
        let database_path = directory.path().join("translation-ledger.sqlite3");
        symlink(&target, &database_path).expect("database symlink");

        assert_eq!(
            open_translation_ledger(&database_path).unwrap_err(),
            "translation-ledger-path-symlink-rejected"
        );
    }
}
