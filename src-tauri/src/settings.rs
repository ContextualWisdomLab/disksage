//! 사용자 설정(현재 online_mode 하나) — app_config_dir/settings.json에 영속. 파싱 실패는 안전측(offline) 기본값.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub online_mode: bool,
}

impl Default for Settings {
    fn default() -> Self { Settings { online_mode: false } }
}

/// JSON → Settings. 손상/부분 JSON은 기본값(offline)으로 fail-safe — 설정 파일이 앱을 깨지 않게.
pub fn parse_settings(json: &str) -> Settings {
    serde_json::from_str(json).unwrap_or_default()
}

/// Settings → JSON(영속용).
pub fn serialize_settings(s: &Settings) -> String {
    // ponytail: to_string() can't fail for a bool-only struct (no maps/NaN); unwrap() avoids an unreachable fallback branch that coverage can't exercise.
    serde_json::to_string(s).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

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
    fn persist_settings_roundtrips_via_regular_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        let expected = Settings { online_mode: true };

        persist_settings(&path, &expected).unwrap();

        let encoded = fs::read_to_string(path).unwrap();
        assert_eq!(parse_settings(&encoded), expected);
    }

    #[cfg(unix)]
    #[test]
    fn persist_settings_replaces_symlink_entry_without_touching_target() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("do-not-overwrite.txt");
        let settings_path = tmp.path().join("settings.json");
        fs::write(&target, b"sentinel").unwrap();
        std::os::unix::fs::symlink(&target, &settings_path).unwrap();

        persist_settings(&settings_path, &Settings { online_mode: true }).unwrap();

        assert_eq!(fs::read_to_string(&target).unwrap(), "sentinel");
        assert!(!fs::symlink_metadata(&settings_path).unwrap().file_type().is_symlink());
        assert!(parse_settings(&fs::read_to_string(settings_path).unwrap()).online_mode);
    }
}
