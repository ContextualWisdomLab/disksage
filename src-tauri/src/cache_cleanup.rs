use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use same_file::Handle;

use crate::{commands::CleanResult, rules, safety, scanner};

#[cfg(windows)]
fn open_directory_handle(path: &Path) -> Option<Handle> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;

    let file = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .ok()?;
    Handle::from_file(file).ok()
}

#[cfg(target_os = "linux")]
const NOFOLLOW_DIRECTORY_FLAGS: i32 = 0o600000; // O_DIRECTORY | O_NOFOLLOW
#[cfg(target_os = "macos")]
const NOFOLLOW_DIRECTORY_FLAGS: i32 = 0x0010_0100; // O_DIRECTORY | O_NOFOLLOW

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn open_directory_handle(path: &Path) -> Option<Handle> {
    use std::os::unix::fs::OpenOptionsExt;

    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(NOFOLLOW_DIRECTORY_FLAGS)
        .open(path)
        .ok()?;
    Handle::from_file(file).ok()
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn open_directory_handle(_path: &Path) -> Option<Handle> {
    None
}

#[cfg(target_os = "linux")]
fn handle_namespace_path(handle: &Handle, _display_path: &Path) -> PathBuf {
    use std::os::fd::AsRawFd;
    PathBuf::from(format!("/proc/self/fd/{}", handle.as_file().as_raw_fd()))
}

#[cfg(target_os = "macos")]
fn handle_namespace_path(handle: &Handle, _display_path: &Path) -> PathBuf {
    use std::os::fd::AsRawFd;
    PathBuf::from(format!("/dev/fd/{}", handle.as_file().as_raw_fd()))
}

#[cfg(windows)]
fn handle_namespace_path(_handle: &Handle, display_path: &Path) -> PathBuf {
    display_path.to_path_buf()
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn handle_namespace_path(_handle: &Handle, display_path: &Path) -> PathBuf {
    display_path.to_path_buf()
}

struct CacheRootGuard {
    handle: Handle,
    display_path: PathBuf,
}

impl CacheRootGuard {
    fn open(bases: &rules::BaseDirs, path: &Path) -> Option<Self> {
        if !rules::is_catalog_path(bases, path) {
            return None;
        }
        let handle = open_directory_handle(path)?;
        let guard = Self {
            handle,
            display_path: path.to_path_buf(),
        };
        guard.still_current().then_some(guard)
    }

    fn still_current(&self) -> bool {
        open_directory_handle(&self.display_path)
            .is_some_and(|current| current == self.handle)
    }

    fn targets(&self) -> Option<Vec<(PathBuf, PathBuf)>> {
        let stable_root = handle_namespace_path(&self.handle, &self.display_path);
        let entries = std::fs::read_dir(stable_root).ok()?;
        let mut targets = Vec::new();
        for entry in entries.filter_map(Result::ok) {
            let io_path = entry.path();
            let metadata = std::fs::symlink_metadata(&io_path).ok()?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
                if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                    continue;
                }
            }
            targets.push((io_path, self.display_path.join(entry.file_name())));
        }
        Some(targets)
    }
}

fn target_bytes(path: &Path) -> u64 {
    if path.is_dir() {
        scanner::scan_dir_with_interval(path, &AtomicBool::new(false), 1, |_| {})
            .stats
            .bytes
    } else {
        path.metadata().map(|metadata| metadata.len()).unwrap_or(0)
    }
}

fn failure(path: &Path, error: &str) -> CleanResult {
    CleanResult {
        path: path.to_string_lossy().into_owned(),
        ok: false,
        error: error.into(),
    }
}

fn clean_cache_contents_inner(
    bases: &rules::BaseDirs,
    dir: &Path,
    journal_path: &Path,
    now_ms: u64,
) -> Result<Vec<CleanResult>, String> {
    let guard = CacheRootGuard::open(bases, dir).ok_or("cache-root-not-current-or-safe")?;
    let targets = guard.targets().ok_or("cache-root-enumeration-failed")?;
    let mut results = Vec::with_capacity(targets.len());

    for (io_path, display_path) in targets {
        let bytes = target_bytes(&io_path);
        if !guard.still_current() {
            results.push(failure(&display_path, "cache-root-changed-before-delete"));
            break;
        }
        results.push(match safety::trash_delete(&display_path, bytes, journal_path, now_ms) {
            Ok(()) => CleanResult {
                path: display_path.to_string_lossy().into_owned(),
                ok: true,
                error: String::new(),
            },
            Err(error) => failure(&display_path, &error.to_string()),
        });
    }

    Ok(results)
}

/// Empty one approved cache directory while retaining a no-follow handle to the validated root.
/// The command never accepts arbitrary roots and stops before the next deletion if the catalog
/// path no longer resolves to the exact directory handle established for this operation.
#[cfg(not(coverage))]
#[tauri::command]
pub fn clean_cache_contents(dir: String, app: tauri::AppHandle) -> Result<Vec<CleanResult>, String> {
    use tauri::Manager;

    let bases = rules::BaseDirs::from_env().ok_or("cache-base-directories-unavailable")?;
    let journal_dir = app.path().app_data_dir().map_err(|error| error.to_string())?;
    std::fs::create_dir_all(&journal_dir).map_err(|error| error.to_string())?;
    let journal_path = journal_dir.join("journal.jsonl");
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    clean_cache_contents_inner(&bases, Path::new(&dir), &journal_path, now_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fake_bases(root: &Path) -> rules::BaseDirs {
        rules::BaseDirs {
            temp: root.join("cache"),
            local_data: root.join("local"),
            home: root.join("home"),
        }
    }

    #[test]
    fn guard_lists_only_real_children_under_catalog_root() {
        let tmp = tempfile::tempdir().unwrap();
        let bases = fake_bases(tmp.path());
        fs::create_dir(&bases.temp).unwrap();
        fs::write(bases.temp.join("real.bin"), b"abc").unwrap();
        let guard = CacheRootGuard::open(&bases, &bases.temp).unwrap();
        let targets = guard.targets().unwrap();
        assert_eq!(targets.len(), 1);
        assert!(targets[0].0.exists());
        assert_eq!(targets[0].1, bases.temp.join("real.bin"));
        assert!(guard.still_current());
    }

    #[cfg(unix)]
    #[test]
    fn guard_detects_catalog_root_replacement() {
        let tmp = tempfile::tempdir().unwrap();
        let bases = fake_bases(tmp.path());
        let moved = tmp.path().join("cache-original");
        let outside = tmp.path().join("outside");
        fs::create_dir(&bases.temp).unwrap();
        fs::create_dir(&outside).unwrap();
        let guard = CacheRootGuard::open(&bases, &bases.temp).unwrap();
        fs::rename(&bases.temp, &moved).unwrap();
        std::os::unix::fs::symlink(&outside, &bases.temp).unwrap();
        assert!(!guard.still_current());
    }

    #[test]
    fn cleanup_rejects_non_catalog_root() {
        let tmp = tempfile::tempdir().unwrap();
        let bases = fake_bases(tmp.path());
        fs::create_dir(&bases.temp).unwrap();
        let journal = tmp.path().join("journal.jsonl");
        let result = clean_cache_contents_inner(&bases, tmp.path(), &journal, 1);
        assert_eq!(result.unwrap_err(), "cache-root-not-current-or-safe");
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn cleanup_moves_catalog_children_to_trash_and_preserves_root() {
        let tmp = tempfile::tempdir().unwrap();
        let bases = fake_bases(tmp.path());
        fs::create_dir(&bases.temp).unwrap();
        let victim = bases.temp.join("disksage-cache-cleanup-fixture.bin");
        fs::write(&victim, vec![0u8; 12]).unwrap();
        let journal = tmp.path().join("journal.jsonl");

        let results = clean_cache_contents_inner(&bases, &bases.temp, &journal, 7).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].ok);
        assert!(!victim.exists());
        assert!(bases.temp.exists());
        let recent = safety::journal_recent(&journal, 2);
        assert_eq!(recent[0].path, victim.to_string_lossy());
        assert_eq!(recent[0].bytes, 12);

        let items: Vec<_> = trash::os_limited::list()
            .unwrap()
            .into_iter()
            .filter(|item| item.name.to_string_lossy().contains("disksage-cache-cleanup-fixture"))
            .collect();
        trash::os_limited::purge_all(items).unwrap();
    }
}
