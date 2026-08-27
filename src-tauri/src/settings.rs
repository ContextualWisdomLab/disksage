//! 사용자 설정(현재 online_mode 하나) — app_config_dir/settings.json에 영속. 파싱 실패는 안전측(offline) 기본값.

use std::io::Read;
use std::path::Path;

const MAX_SETTINGS_DOCUMENT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    #[serde(default)]
    pub online_mode: bool,
}

impl Default for Settings {
    fn default() -> Self { Settings { online_mode: false } }
}

/// JSON → Settings. 손상/부분/과대 JSON은 기본값(offline)으로 fail-safe — 설정 파일이 앱을 깨지 않게.
pub fn parse_settings(json: &str) -> Settings {
    if json.len() > MAX_SETTINGS_DOCUMENT_BYTES {
        return Settings::default();
    }
    serde_json::from_str(json).unwrap_or_default()
}

/// 설정 파일을 최대 64KiB + 1 byte까지만 읽고 초과/손상 입력은 offline 기본값으로 처리한다.
pub fn load_settings_file(path: &Path) -> Settings {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return Settings::default(),
    };
    let mut bytes = Vec::with_capacity(MAX_SETTINGS_DOCUMENT_BYTES + 1);
    let mut bounded = file.take((MAX_SETTINGS_DOCUMENT_BYTES + 1) as u64);
    if bounded.read_to_end(&mut bytes).is_err() || bytes.len() > MAX_SETTINGS_DOCUMENT_BYTES {
        return Settings::default();
    }
    let json = match std::str::from_utf8(&bytes) {
        Ok(json) => json,
        Err(_) => return Settings::default(),
    };
    parse_settings(json)
}

/// Settings → JSON(영속용).
pub fn serialize_settings(s: &Settings) -> String {
    // ponytail: to_string() can't fail for a bool-only struct (no maps/NaN); unwrap() avoids an unreachable fallback branch that coverage can't exercise.
    serde_json::to_string(s).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_offline() {
        assert!(!Settings::default().online_mode);
    }
    #[test]
    fn parse_roundtrip() {
        let s = Settings { online_mode: true };
        assert_eq!(parse_settings(&serialize_settings(&s)), s);
    }
    #[test]
    fn parse_corrupt_is_default_offline() {
        assert_eq!(parse_settings("not json"), Settings::default());
        assert_eq!(parse_settings(""), Settings::default());
        // 부분 JSON(필드 없음)도 기본값
        assert_eq!(parse_settings("{}"), Settings { online_mode: false });
    }
    #[test]
    fn parse_explicit_true() {
        assert!(parse_settings(r#"{"online_mode":true}"#).online_mode);
    }
    #[test]
    fn unknown_keys_fail_closed_even_when_online_true() {
        assert_eq!(
            parse_settings(r#"{"online_mode":true,"unexpected_remote_setting":true}"#),
            Settings::default()
        );
    }
    #[test]
    fn duplicate_online_mode_fails_closed_instead_of_accepting_ambiguous_authority() {
        for ambiguous in [
            r#"{"online_mode":true,"online_mode":false}"#,
            r#"{"online_mode":false,"online_mode":true}"#,
        ] {
            assert_eq!(parse_settings(ambiguous), Settings::default());
        }
    }
    #[test]
    fn non_boolean_online_mode_fails_closed_instead_of_coercing_network_authority() {
        for malformed in [
            r#"{"online_mode":null}"#,
            r#"{"online_mode":1}"#,
            r#"{"online_mode":"true"}"#,
        ] {
            assert_eq!(parse_settings(malformed), Settings::default());
        }
    }
    #[test]
    fn oversized_document_fails_closed_instead_of_enabling_network_authority() {
        let mut oversized = String::from(r#"{"online_mode":true}"#);
        oversized.push_str(&" ".repeat(MAX_SETTINGS_DOCUMENT_BYTES));
        assert!(oversized.len() > MAX_SETTINGS_DOCUMENT_BYTES);
        assert_eq!(parse_settings(&oversized), Settings::default());
    }
    #[test]
    fn load_settings_file_bounds_disk_read_before_parsing() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("settings.json");
        let mut oversized = vec![b' '; MAX_SETTINGS_DOCUMENT_BYTES + 1];
        let prefix = br#"{"online_mode":true}"#;
        oversized[..prefix.len()].copy_from_slice(prefix);
        std::fs::write(&path, oversized).unwrap();

        assert_eq!(load_settings_file(&path), Settings::default());
    }
    #[test]
    fn load_settings_file_preserves_in_limit_settings() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("settings.json");
        std::fs::write(&path, br#"{"online_mode":true}"#).unwrap();

        assert_eq!(load_settings_file(&path), Settings { online_mode: true });
    }
}
