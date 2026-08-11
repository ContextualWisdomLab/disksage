use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::scanner;

pub struct BaseDirs {
    pub temp: PathBuf,
    pub local_data: PathBuf,
    pub home: PathBuf,
}

impl BaseDirs {
    pub fn from_env() -> Option<BaseDirs> {
        let home = std::env::var(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).ok()?;
        let home = PathBuf::from(home);
        let temp = std::env::temp_dir();
        #[cfg(windows)]
        let local_data = std::env::var("LOCALAPPDATA").map(PathBuf::from).ok()?;
        #[cfg(not(windows))]
        let local_data = home.join(".cache");
        Some(BaseDirs { temp, local_data, home })
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CacheCandidate {
    pub id: String,
    pub label: String,
    pub path: String,
    pub bytes: u64,
    pub exists: bool,
}

fn catalog(bases: &BaseDirs) -> Vec<(&'static str, &'static str, PathBuf)> {
    #[cfg(windows)]
    let npm = bases.local_data.join("npm-cache");
    #[cfg(not(windows))]
    let npm = bases.home.join(".npm");

    #[cfg(windows)]
    let pip = bases.local_data.join("pip").join("cache");
    #[cfg(target_os = "macos")]
    let pip = bases.home.join("Library").join("Caches").join("pip");
    #[cfg(not(any(windows, target_os = "macos")))]
    let pip = bases.local_data.join("pip");

    #[allow(unused_mut)]
    let mut entries = vec![
        ("os-temp", "OS 임시 폴더", bases.temp.clone()),
        ("npm-cache", "npm 캐시", npm),
        ("pip-cache", "pip 캐시", pip),
        (
            "cargo-registry-cache",
            "cargo 레지스트리 캐시",
            bases.home.join(".cargo").join("registry").join("cache"),
        ),
    ];

    #[cfg(windows)]
    entries.extend([
        (
            "rdp-autotrace",
            "원격 데스크톱 추적 로그",
            bases.temp.join("DiagOutputDir").join("RdClientAutoTrace"),
        ),
        (
            "windows-crashdumps",
            "앱 크래시 덤프",
            bases.local_data.join("CrashDumps"),
        ),
        (
            "windows-wer",
            "Windows 오류 보고 (WER)",
            bases.local_data.join("Microsoft").join("Windows").join("WER"),
        ),
    ]);

    entries
}

pub fn cache_candidates(bases: &BaseDirs) -> Vec<CacheCandidate> {
    catalog(bases)
        .into_iter()
        .map(|(id, label, path)| {
            let exists = path.is_dir();
            let bytes = if exists {
                scanner::scan_dir_with_interval(&path, &AtomicBool::new(false), 1, |_| {}).stats.bytes
            } else {
                0
            };
            CacheCandidate {
                id: id.into(),
                label: label.into(),
                path: path.to_string_lossy().into_owned(),
                bytes,
                exists,
            }
        })
        .collect()
}

pub fn is_catalog_path(bases: &BaseDirs, dir: &Path) -> bool {
    catalog(bases).iter().any(|(_, _, p)| p == dir)
}

pub fn clean_targets(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    rd.filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| !t.is_symlink()).unwrap_or(false))
        .map(|e| e.path())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fake_bases(root: &std::path::Path) -> BaseDirs {
        BaseDirs {
            temp: root.join("tmp"),
            local_data: root.join("local"),
            home: root.join("home"),
        }
    }

    #[test]
    fn from_env_uses_real_environment() {
        assert!(BaseDirs::from_env().is_some());
    }

    #[test]
    fn catalog_reports_sizes_and_existence() {
        let tmp = tempfile::tempdir().unwrap();
        let bases = fake_bases(tmp.path());
        let npm = if cfg!(windows) {
            bases.local_data.join("npm-cache")
        } else {
            bases.home.join(".npm")
        };
        fs::create_dir_all(&npm).unwrap();
        fs::write(npm.join("blob.bin"), vec![0u8; 128]).unwrap();

        let cands = cache_candidates(&bases);
        let npm_c = cands.iter().find(|c| c.id == "npm-cache").unwrap();
        assert!(npm_c.exists);
        assert_eq!(npm_c.bytes, 128);
        let temp_c = cands.iter().find(|c| c.id == "os-temp").unwrap();
        assert!(!temp_c.exists);
        assert_eq!(temp_c.bytes, 0);
        assert!(cands.len() >= 4);
    }

    #[cfg(windows)]
    #[test]
    fn catalog_includes_windows_diagnostic_caches() {
        let tmp = tempfile::tempdir().unwrap();
        let bases = fake_bases(tmp.path());
        let cands = cache_candidates(&bases);
        for id in ["rdp-autotrace", "windows-crashdumps", "windows-wer"] {
            assert!(cands.iter().any(|c| c.id == id), "{id} 항목 누락");
        }
        let rdp = cands.iter().find(|c| c.id == "rdp-autotrace").unwrap();
        assert!(rdp.path.contains("RdClientAutoTrace"));
        assert!(cands.len() >= 7);
    }

    #[test]
    fn is_catalog_path_scopes_to_catalog() {
        let tmp = tempfile::tempdir().unwrap();
        let bases = fake_bases(tmp.path());
        assert!(is_catalog_path(&bases, &bases.temp));
        assert!(!is_catalog_path(&bases, tmp.path()));
    }

    #[test]
    fn clean_targets_lists_immediate_children_only() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("a")).unwrap();
        fs::write(tmp.path().join("a").join("deep.bin"), b"x").unwrap();
        fs::write(tmp.path().join("b.bin"), b"y").unwrap();

        let mut names: Vec<String> = clean_targets(tmp.path())
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, vec!["a", "b.bin"]);
    }

    #[test]
    fn clean_targets_missing_dir_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(clean_targets(&tmp.path().join("nope")).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn clean_targets_excludes_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("real.bin"), b"x").unwrap();
        std::os::unix::fs::symlink(tmp.path().join("real.bin"), tmp.path().join("link.bin")).unwrap();
        let names: Vec<String> = clean_targets(tmp.path())
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["real.bin"]);
    }

    #[cfg(unix)]
    #[test]
    fn catalog_scope_rejects_symlinked_cache_root() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tmp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        let linked_cache = tmp.path().join("linked-cache");
        std::os::unix::fs::symlink(&outside, &linked_cache).unwrap();
        let bases = BaseDirs {
            temp: linked_cache.clone(),
            local_data: tmp.path().join("local"),
            home: tmp.path().join("home"),
        };

        assert!(!is_catalog_path(&bases, &linked_cache));
    }

    #[cfg(unix)]
    #[test]
    fn clean_targets_rejects_symlinked_cache_root() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tmp.path().join("outside");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("keep.bin"), b"keep").unwrap();
        let linked_cache = tmp.path().join("linked-cache");
        std::os::unix::fs::symlink(&outside, &linked_cache).unwrap();

        assert!(clean_targets(&linked_cache).is_empty());
    }
}
