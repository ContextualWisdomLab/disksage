use disksage_lib::translation_ledger_store::{
    install_current_translation_resource, lookup_translation_message, open_translation_ledger,
};
use disksage_lib::translation_resource::{
    current_translation_resource_asset, TranslationResource,
};
use std::fs;

fn checked_in_resource() -> TranslationResource {
    serde_json::from_slice(include_bytes!(
        "../resources/translation/releases/2026.09.11.1.json"
    ))
    .expect("checked-in translation resource JSON")
}

#[test]
fn native_ledger_installs_current_resource_idempotently_and_serves_exact_lookup() {
    let directory = tempfile::tempdir().expect("temporary ledger directory");
    let database_path = directory.path().join("translation-ledger.sqlite3");
    let mut connection = open_translation_ledger(&database_path).expect("open native ledger");
    let resource = checked_in_resource();

    install_current_translation_resource(&mut connection, &resource, 1_789_344_000_000)
        .expect("first immutable resource install");
    install_current_translation_resource(&mut connection, &resource, 1_789_344_000_001)
        .expect("idempotent immutable resource install");

    let asset = current_translation_resource_asset();
    assert_eq!(
        lookup_translation_message(
            &connection,
            asset.resource_version,
            "ko",
            "app.action.scan",
        )
        .expect("exact persisted lookup"),
        "스캔"
    );

    let version_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM translation_resource_versions", [], |row| row.get(0))
        .expect("version count");
    assert_eq!(version_count, 1, "idempotent install must not duplicate releases");

    let screen_area: String = connection
        .query_row(
            "SELECT screen_area FROM translation_screen_keys WHERE screen_key = 'app.action.scan'",
            [],
            |row| row.get(0),
        )
        .expect("persisted screen area");
    assert_eq!(screen_area, "app.action");
}

#[test]
fn native_ledger_rejects_mutation_of_an_existing_release_and_missing_lookup() {
    let directory = tempfile::tempdir().expect("temporary ledger directory");
    let database_path = directory.path().join("translation-ledger.sqlite3");
    let mut connection = open_translation_ledger(&database_path).expect("open native ledger");
    let resource = checked_in_resource();
    install_current_translation_resource(&mut connection, &resource, 1_789_344_000_000)
        .expect("initial install");

    let mut changed = resource.clone();
    changed
        .messages
        .get_mut("app.action.scan")
        .expect("scan message")
        .insert("ko".to_string(), "변조된 스캔".to_string());

    assert_eq!(
        install_current_translation_resource(&mut connection, &changed, 1_789_344_000_002),
        Err("translation-ledger-existing-version-mismatch".to_string())
    );

    assert_eq!(
        lookup_translation_message(
            &connection,
            current_translation_resource_asset().resource_version,
            "ko",
            "app.action.missing",
        ),
        Err("translation-message-missing".to_string())
    );

    drop(connection);
    assert!(fs::metadata(database_path).expect("ledger metadata").is_file());
}
