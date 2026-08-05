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
}
