//! Native admission boundary for immutable bundled presentation resources.
//!
//! Translation copy is presentation data, not ontology vocabulary or filesystem authority. The
//! checked-in asset is accepted only when its fixed release identity, SHA-256 digest, structural
//! screen keys and complete supported-locale set all match. Future Tauri IPC code may resolve this
//! fixed asset through `BaseDirectory::Resource`; it must not accept a frontend-selected path.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

pub const CURRENT_TRANSLATION_RESOURCE_VERSION: &str = "2026.09.11.1";
const CURRENT_TRANSLATION_RESOURCE_PATH: &str =
    "resources/translation/releases/2026.09.11.1.json";
const CURRENT_TRANSLATION_RESOURCE_SHA256: &str =
    "a58edf6fc547f9bee6411ac0ec7788fb092037d3e97e406755834cea9e5b691b";
const TRANSLATION_RESOURCE_SCHEMA_VERSION: u32 = 1;
const MAX_TRANSLATION_RESOURCE_BYTES: usize = 4 * 1024 * 1024;
const MAX_TRANSLATION_MESSAGE_BYTES: usize = 8 * 1024;
const SUPPORTED_LOCALES: [&str; 8] = ["ko", "en", "ja", "zh", "vi", "es", "de", "fr"];

/// Compile-time identity of the one translation resource this application build accepts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TranslationResourceAsset {
    pub resource_version: &'static str,
    pub relative_path: &'static str,
    pub sha256: &'static str,
}

/// Validated immutable presentation resource. It contains no ontology or mutation authority.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct TranslationResource {
    pub resource_version: String,
    pub schema_version: u32,
    pub messages: BTreeMap<String, BTreeMap<String, String>>,
}

/// Returns the packaged resource identity pinned by this binary.
pub fn current_translation_resource_asset() -> TranslationResourceAsset {
    TranslationResourceAsset {
        resource_version: CURRENT_TRANSLATION_RESOURCE_VERSION,
        relative_path: CURRENT_TRANSLATION_RESOURCE_PATH,
        sha256: CURRENT_TRANSLATION_RESOURCE_SHA256,
    }
}

/// Loads the current resource from an already-resolved fixed bundle path.
///
/// This stays crate-visible so a future Tauri adapter can resolve only the compile-time asset through
/// `BaseDirectory::Resource`; external library consumers cannot turn an arbitrary path into product
/// translation authority. The bytes remain authoritative only after digest and structure validation.
pub(crate) fn load_current_translation_resource_file(
    path: &Path,
) -> Result<TranslationResource, String> {
    let path_metadata = fs::symlink_metadata(path)
        .map_err(|_| "translation-resource-metadata-unavailable".to_string())?;
    if path_metadata.file_type().is_symlink() {
        return Err("translation-resource-symlink-rejected".to_string());
    }
    if !path_metadata.is_file() {
        return Err("translation-resource-non-regular-rejected".to_string());
    }
    if path_metadata.len() > MAX_TRANSLATION_RESOURCE_BYTES as u64 {
        return Err("translation-resource-too-large".to_string());
    }

    let mut file = File::open(path).map_err(|_| "translation-resource-open-failed".to_string())?;
    let bytes = read_bounded_translation_resource(&mut file, path_metadata.len() as usize)?;
    load_translation_resource_bytes(
        &bytes,
        CURRENT_TRANSLATION_RESOURCE_VERSION,
        CURRENT_TRANSLATION_RESOURCE_SHA256,
    )
}

fn read_bounded_translation_resource(
    reader: impl Read,
    initial_capacity: usize,
) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::with_capacity(initial_capacity.min(MAX_TRANSLATION_RESOURCE_BYTES));
    reader
        .take((MAX_TRANSLATION_RESOURCE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "translation-resource-read-failed".to_string())?;
    Ok(bytes)
}

fn load_translation_resource_bytes(
    bytes: &[u8],
    expected_version: &str,
    expected_sha256: &str,
) -> Result<TranslationResource, String> {
    if bytes.len() > MAX_TRANSLATION_RESOURCE_BYTES {
        return Err("translation-resource-too-large".to_string());
    }
    if !is_lower_hex_sha256(expected_sha256) {
        return Err("translation-resource-expected-digest-invalid".to_string());
    }
    if sha256_hex(bytes) != expected_sha256 {
        return Err("translation-resource-digest-mismatch".to_string());
    }

    let resource: TranslationResource = serde_json::from_slice(bytes)
        .map_err(|_| "translation-resource-json-invalid".to_string())?;
    validate_translation_resource(&resource, expected_version)?;
    Ok(resource)
}

fn validate_translation_resource(
    resource: &TranslationResource,
    expected_version: &str,
) -> Result<(), String> {
    if resource.resource_version != expected_version {
        return Err("translation-resource-version-mismatch".to_string());
    }
    if resource.schema_version != TRANSLATION_RESOURCE_SCHEMA_VERSION {
        return Err("translation-resource-schema-version-unsupported".to_string());
    }
    if resource.messages.is_empty() {
        return Err("translation-resource-messages-empty".to_string());
    }

    for (screen_key, localized) in &resource.messages {
        if !valid_screen_key(screen_key) {
            return Err("translation-resource-screen-key-invalid".to_string());
        }
        if localized.len() != SUPPORTED_LOCALES.len()
            || localized
                .keys()
                .any(|locale| !SUPPORTED_LOCALES.contains(&locale.as_str()))
            || SUPPORTED_LOCALES
                .iter()
                .any(|locale| !localized.contains_key(*locale))
        {
            return Err("translation-resource-locale-set-incomplete".to_string());
        }
        if localized.values().any(|text| {
            text.trim().is_empty()
                || text.len() > MAX_TRANSLATION_MESSAGE_BYTES
                || text.as_bytes().contains(&0)
        }) {
            return Err("translation-resource-message-invalid".to_string());
        }
    }
    Ok(())
}

fn valid_screen_key(screen_key: &str) -> bool {
    (1..=160).contains(&screen_key.len())
        && screen_key.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b'_' | b'-')
        })
}

fn is_lower_hex_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::fs;
    use std::io;

    fn checked_in_bytes() -> &'static [u8] {
        include_bytes!("../resources/translation/releases/2026.09.11.1.json")
    }

    fn checked_in_path() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join(CURRENT_TRANSLATION_RESOURCE_PATH)
    }

    fn mutated_resource(mutator: impl FnOnce(&mut Value)) -> Vec<u8> {
        let mut value: Value = serde_json::from_slice(checked_in_bytes()).expect("fixture JSON");
        mutator(&mut value);
        serde_json::to_vec_pretty(&value).expect("fixture serialization")
    }

    fn load_mutated(bytes: &[u8]) -> Result<TranslationResource, String> {
        let digest = sha256_hex(bytes);
        load_translation_resource_bytes(bytes, CURRENT_TRANSLATION_RESOURCE_VERSION, &digest)
    }

    #[test]
    fn checked_in_resource_matches_pinned_digest_and_locale_contract() {
        let resource = load_translation_resource_bytes(
            checked_in_bytes(),
            CURRENT_TRANSLATION_RESOURCE_VERSION,
            CURRENT_TRANSLATION_RESOURCE_SHA256,
        )
        .expect("checked-in translation resource must validate");

        assert_eq!(resource.schema_version, 1);
        assert_eq!(resource.messages.len(), 2);
        assert_eq!(resource.messages["app.action.scan"]["ko"], "스캔");
        assert_eq!(resource.messages["app.action.scan"]["fr"], "Analyser");
    }

    #[test]
    fn checked_in_resource_file_passes_native_file_admission() {
        let resource = load_current_translation_resource_file(&checked_in_path())
            .expect("checked-in translation resource file must validate");
        assert_eq!(resource.resource_version, CURRENT_TRANSLATION_RESOURCE_VERSION);
    }

    #[test]
    fn digest_and_expected_digest_must_be_exact() {
        assert_eq!(
            load_translation_resource_bytes(
                b"{}",
                CURRENT_TRANSLATION_RESOURCE_VERSION,
                CURRENT_TRANSLATION_RESOURCE_SHA256,
            ),
            Err("translation-resource-digest-mismatch".to_string())
        );
        assert_eq!(
            load_translation_resource_bytes(
                checked_in_bytes(),
                CURRENT_TRANSLATION_RESOURCE_VERSION,
                "A58EDF6FC547F9BEE6411AC0EC7788FB092037D3E97E406755834CEA9E5B691B",
            ),
            Err("translation-resource-expected-digest-invalid".to_string())
        );
    }

    #[test]
    fn version_schema_and_message_collection_are_fail_closed() {
        assert_eq!(
            load_translation_resource_bytes(
                checked_in_bytes(),
                "2026.09.11.2",
                CURRENT_TRANSLATION_RESOURCE_SHA256,
            ),
            Err("translation-resource-version-mismatch".to_string())
        );

        let unsupported_schema = mutated_resource(|value| value["schema_version"] = 2.into());
        assert_eq!(
            load_mutated(&unsupported_schema),
            Err("translation-resource-schema-version-unsupported".to_string())
        );

        let no_messages = mutated_resource(|value| {
            value["messages"] = serde_json::json!({});
        });
        assert_eq!(
            load_mutated(&no_messages),
            Err("translation-resource-messages-empty".to_string())
        );
    }

    #[test]
    fn screen_keys_and_locale_sets_are_exact() {
        let invalid_key = mutated_resource(|value| {
            let messages = value["messages"].as_object_mut().expect("messages object");
            let message = messages.remove("app.action.scan").expect("scan message");
            messages.insert("App Action Scan".to_string(), message);
        });
        assert_eq!(
            load_mutated(&invalid_key),
            Err("translation-resource-screen-key-invalid".to_string())
        );
        assert!(!valid_screen_key(""));
        assert!(!valid_screen_key(&"a".repeat(161)));
        assert!(valid_screen_key("app.scan_action-v1"));

        let missing_locale = mutated_resource(|value| {
            value["messages"]["app.action.scan"]
                .as_object_mut()
                .expect("locale object")
                .remove("fr");
        });
        assert_eq!(
            load_mutated(&missing_locale),
            Err("translation-resource-locale-set-incomplete".to_string())
        );

        let extra_locale = mutated_resource(|value| {
            value["messages"]["app.action.scan"]["it"] = "Analizza".into();
        });
        assert_eq!(
            load_mutated(&extra_locale),
            Err("translation-resource-locale-set-incomplete".to_string())
        );
    }

    #[test]
    fn empty_nul_and_oversized_messages_are_rejected() {
        let empty = mutated_resource(|value| {
            value["messages"]["app.action.scan"]["de"] = "   ".into();
        });
        assert_eq!(
            load_mutated(&empty),
            Err("translation-resource-message-invalid".to_string())
        );

        let nul = mutated_resource(|value| {
            value["messages"]["app.action.scan"]["de"] = "Scan\u{0000}".into();
        });
        assert_eq!(
            load_mutated(&nul),
            Err("translation-resource-message-invalid".to_string())
        );

        let oversized = mutated_resource(|value| {
            value["messages"]["app.action.scan"]["de"] =
                "x".repeat(MAX_TRANSLATION_MESSAGE_BYTES + 1).into();
        });
        assert_eq!(
            load_mutated(&oversized),
            Err("translation-resource-message-invalid".to_string())
        );
    }

    #[test]
    fn malformed_json_and_oversized_resource_are_rejected() {
        let malformed = b"{";
        let malformed_digest = sha256_hex(malformed);
        assert_eq!(
            load_translation_resource_bytes(
                malformed,
                CURRENT_TRANSLATION_RESOURCE_VERSION,
                &malformed_digest,
            ),
            Err("translation-resource-json-invalid".to_string())
        );

        let oversized = vec![b'x'; MAX_TRANSLATION_RESOURCE_BYTES + 1];
        assert_eq!(
            load_translation_resource_bytes(
                &oversized,
                CURRENT_TRANSLATION_RESOURCE_VERSION,
                CURRENT_TRANSLATION_RESOURCE_SHA256,
            ),
            Err("translation-resource-too-large".to_string())
        );
    }

    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("synthetic read boundary failure"))
        }
    }

    #[test]
    fn bounded_reader_preserves_read_failure_and_caps_capacity() {
        assert_eq!(
            read_bounded_translation_resource(FailingReader, usize::MAX),
            Err("translation-resource-read-failed".to_string())
        );
    }

    #[test]
    fn file_loader_accepts_regular_pinned_resource_and_rejects_directory() {
        let directory = tempfile::tempdir().expect("temporary directory");
        let resource_path = directory.path().join("resource.json");
        fs::write(&resource_path, checked_in_bytes()).expect("write resource fixture");

        let resource = load_current_translation_resource_file(&resource_path)
            .expect("regular pinned resource must load");
        assert_eq!(resource.resource_version, CURRENT_TRANSLATION_RESOURCE_VERSION);
        assert_eq!(
            load_current_translation_resource_file(directory.path()),
            Err("translation-resource-non-regular-rejected".to_string())
        );
    }

    #[test]
    fn file_loader_rejects_missing_and_oversized_resource() {
        let directory = tempfile::tempdir().expect("temporary directory");
        assert_eq!(
            load_current_translation_resource_file(&directory.path().join("missing.json")),
            Err("translation-resource-metadata-unavailable".to_string())
        );

        let oversized_path = directory.path().join("oversized.json");
        let file = File::create(&oversized_path).expect("oversized fixture file");
        file.set_len((MAX_TRANSLATION_RESOURCE_BYTES + 1) as u64)
            .expect("extend oversized fixture");
        assert_eq!(
            load_current_translation_resource_file(&oversized_path),
            Err("translation-resource-too-large".to_string())
        );
    }

    #[cfg(unix)]
    #[test]
    fn file_loader_rejects_symlink_and_unreadable_resource() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let directory = tempfile::tempdir().expect("temporary directory");
        let target = directory.path().join("target.json");
        let link = directory.path().join("resource.json");
        fs::write(&target, checked_in_bytes()).expect("write symlink target");
        symlink(&target, &link).expect("create symlink fixture");
        assert_eq!(
            load_current_translation_resource_file(&link),
            Err("translation-resource-symlink-rejected".to_string())
        );

        let unreadable = directory.path().join("unreadable.json");
        fs::write(&unreadable, checked_in_bytes()).expect("write unreadable fixture");
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000))
            .expect("remove fixture read permission");
        let result = load_current_translation_resource_file(&unreadable);
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o600))
            .expect("restore fixture read permission for cleanup");
        assert_eq!(
            result,
            Err("translation-resource-open-failed".to_string())
        );
    }
}
