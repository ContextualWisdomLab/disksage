//! User settings and their local durable persistence boundary.
//!
//! Missing settings are safe to interpret as offline defaults. Existing settings that cannot be
//! read are different: silently treating an I/O failure as a successful default would hide the
//! failure from the UI and could misrepresent what is durably configured.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

#[cfg(not(coverage))]
use tauri::AppHandle;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub online_mode: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { online_mode: false }
    }
}

/// Parses persisted JSON, falling back to the offline default for malformed user content.
pub fn parse_settings(json: &str) -> Settings {
    serde_json::from_str(json).unwrap_or_default()
}

/// Serializes the bool-only settings value for durable persistence.
pub fn serialize_settings(settings: &Settings) -> String {
    // A bool-only struct has no fallible JSON value shapes such as non-finite numbers or map keys.
    serde_json::to_string(settings).expect("bool-only Settings serialization is infallible")
}

/// Loads settings from a regular file, treating only a genuinely missing file as first-run state.
pub fn load_settings(path: &Path) -> Result<Settings, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Settings::default()),
        Err(_) => return Err("settings-read-failed".into()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("settings-file-unsafe".into());
    }
    fs::read_to_string(path)
        .map(|json| parse_settings(&json))
        .map_err(|_| "settings-read-failed".into())
}

fn save_settings_with_persist<F>(
    path: &Path,
    settings: &Settings,
    persist: F,
) -> Result<(), String>
where
    F: FnOnce(tempfile::NamedTempFile, &Path) -> Result<(), String>,
{
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| "settings-parent-unavailable".to_string())?;
    fs::create_dir_all(parent).map_err(|_| "settings-parent-unavailable".to_string())?;

    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err("settings-file-unsafe".into());
        }
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(_) => return Err("settings-write-failed".into()),
    }

    let serialized = serialize_settings(settings);
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|_| "settings-write-failed".to_string())?;
    temporary
        .write_all(serialized.as_bytes())
        .map_err(|_| "settings-write-failed".to_string())?;
    temporary
        .flush()
        .map_err(|_| "settings-write-failed".to_string())?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|_| "settings-write-failed".to_string())?;
    persist(temporary, path)
}

/// Atomically replaces the settings file after the complete new value has been written and synced.
pub fn save_settings(path: &Path, settings: &Settings) -> Result<(), String> {
    save_settings_with_persist(path, settings, |temporary, destination| {
        temporary
            .persist(destination)
            .map(|_| ())
            .map_err(|_| "settings-replace-failed".to_string())
    })
}

#[cfg(not(coverage))]
fn settings_file_path(app: &AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    app.path()
        .app_config_dir()
        .map(|directory| directory.join("settings.json"))
        .map_err(|_| "settings-directory-unavailable".to_string())
}

/// Tauri read adapter for the canonical settings persistence owner.
#[cfg(not(coverage))]
#[tauri::command]
pub fn get_settings(app: AppHandle) -> Result<Settings, String> {
    load_settings(&settings_file_path(&app)?)
}

/// Tauri write adapter that returns the value only after atomic persistence succeeds.
#[cfg(not(coverage))]
#[tauri::command]
pub fn set_settings(online_mode: bool, app: AppHandle) -> Result<Settings, String> {
    let settings = Settings { online_mode };
    save_settings(&settings_file_path(&app)?, &settings)?;
    Ok(settings)
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
        let settings = Settings { online_mode: true };
        assert_eq!(parse_settings(&serialize_settings(&settings)), settings);
    }

    #[test]
    fn parse_corrupt_is_default_offline() {
        assert_eq!(parse_settings("not json"), Settings::default());
        assert_eq!(parse_settings(""), Settings::default());
        assert_eq!(parse_settings("{}"), Settings { online_mode: false });
    }

    #[test]
    fn parse_explicit_true() {
        assert!(parse_settings(r#"{"online_mode":true}"#).online_mode);
    }

    #[test]
    fn missing_file_is_first_run_offline_default() {
        let temporary = tempfile::tempdir().unwrap();
        assert_eq!(
            load_settings(&temporary.path().join("settings.json")).unwrap(),
            Settings::default()
        );
    }

    #[test]
    fn existing_unreadable_content_is_not_silently_defaulted() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("settings.json");
        fs::write(&path, [0xff, 0xfe, 0xfd]).unwrap();
        assert_eq!(load_settings(&path), Err("settings-read-failed".into()));
    }

    #[test]
    fn non_regular_settings_path_fails_closed() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("settings.json");
        fs::create_dir(&path).unwrap();
        assert_eq!(load_settings(&path), Err("settings-file-unsafe".into()));
        assert_eq!(
            save_settings(&path, &Settings { online_mode: true }),
            Err("settings-file-unsafe".into())
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlink_settings_path_fails_closed() {
        let temporary = tempfile::tempdir().unwrap();
        let target = temporary.path().join("target.json");
        fs::write(&target, r#"{"online_mode":true}"#).unwrap();
        let path = temporary.path().join("settings.json");
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert_eq!(load_settings(&path), Err("settings-file-unsafe".into()));
        assert_eq!(
            save_settings(&path, &Settings { online_mode: false }),
            Err("settings-file-unsafe".into())
        );
        assert!(load_settings(&target).unwrap().online_mode);
    }

    #[test]
    fn save_roundtrip_replaces_complete_value() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("settings.json");
        save_settings(&path, &Settings { online_mode: true }).unwrap();
        assert!(load_settings(&path).unwrap().online_mode);
        save_settings(&path, &Settings { online_mode: false }).unwrap();
        assert!(!load_settings(&path).unwrap().online_mode);
    }

    #[test]
    fn failed_replace_preserves_previous_durable_value() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("settings.json");
        save_settings(&path, &Settings { online_mode: true }).unwrap();
        let before = fs::read(&path).unwrap();

        let result = save_settings_with_persist(
            &path,
            &Settings { online_mode: false },
            |_temporary, _destination| Err("injected-replace-failure".into()),
        );

        assert_eq!(result, Err("injected-replace-failure".into()));
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(load_settings(&path).unwrap().online_mode);
    }
}
