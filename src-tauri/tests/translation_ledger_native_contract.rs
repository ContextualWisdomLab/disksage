#[test]
fn native_ledger_remains_private_and_resource_admission_precedes_persistence() {
    let lib_source = include_str!("../src/lib.rs");
    let bridge_source = include_str!("../src/translation_resource_bridge.rs");
    let store_source = include_str!("../src/translation_ledger_store.rs");
    let cargo_manifest = include_str!("../Cargo.toml");

    assert!(lib_source.contains("mod translation_ledger_store;"));
    assert!(!lib_source.contains("pub mod translation_ledger_store;"));

    let admission = bridge_source
        .find("load_current_translation_resource_file(&resource_path)?")
        .expect("canonical immutable-resource admission must remain in the bridge");
    let ledger_open = bridge_source
        .find("open_translation_ledger(&database_path)?")
        .expect("native ledger must be opened by the bridge");
    assert!(
        admission < ledger_open,
        "resource file identity/digest/schema admission must finish before ledger writes"
    );

    assert!(store_source.contains("TransactionBehavior::Immediate"));
    assert!(store_source.contains("0001_translation_ledger.sql"));
    assert!(!store_source.contains("INSERT OR REPLACE"));
    assert!(!store_source.contains("UPDATE translation_"));
    assert!(!store_source.contains("DELETE FROM translation_"));

    assert!(cargo_manifest.contains(
        "rusqlite = { version = \"=0.37.0\", features = [\"bundled\"] }"
    ));
}

#[test]
fn write_lock_recheck_is_constant_scope_and_full_verification_happens_without_it() {
    let store_source = include_str!("../src/translation_ledger_store.rs");

    assert!(store_source.contains("existing_release_identity_matches(&transaction"));
    assert!(!store_source.contains("existing_release_matches(&transaction"));
    assert!(store_source.contains(
        "transaction.commit()\n            .map_err(|_| \"translation-ledger-transaction-commit-failed\".to_string())?;\n        return existing_release_matches(connection"
    ));
}

#[test]
fn presentation_ipc_does_not_accept_database_resource_or_digest_authority() {
    let bridge_source = include_str!("../src/translation_resource_bridge.rs");
    let command_start = bridge_source
        .find("pub fn get_translation_message(")
        .expect("translation command");
    let command_body = &bridge_source[command_start..];
    let signature_end = command_body
        .find(") -> Result<TranslationMessageView, String>")
        .expect("bounded translation command signature");
    let signature = &command_body[..signature_end];

    assert!(signature.contains("locale: String"));
    assert!(signature.contains("screen_key: String"));
    for forbidden in ["database", "path", "digest", "resource_version", "fallback"] {
        assert!(
            !signature.contains(forbidden),
            "frontend IPC must not choose {forbidden} authority"
        );
    }
}
