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
        let local_data = if cfg!(windows) {
            std::env::var("LOCALAPPDATA").map(PathBuf::from).ok()?
        } else {
            home.join(".cache")
        };
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

/// 정적 캐시 카탈로그 (스펙 §4 rules). 항목 = (id, 라벨, 베이스 기준 상대경로).
/// ponytail: 브라우저 캐시는 프로필 글롭이 필요해 M2 범위 밖 — 카탈로그에 추가만 하면 확장됨
fn catalog(bases: &BaseDirs) -> Vec<(&'static str, &'static str, PathBuf)> {
    let npm = if cfg!(windows) {
        bases.local_data.join("npm-cache")
    } else {
        bases.home.join(".npm") // npm 실제 기본값 (linux/macOS)
    };
    let pip = if cfg!(windows) {
        bases.local_data.join("pip").join("cache")
    } else if cfg!(target_os = "macos") {
        bases.home.join("Library").join("Caches").join("pip")
    } else {
        bases.local_data.join("pip") // linux: ~/.cache/pip
    };
    vec![
        ("os-temp", "OS 임시 폴더", bases.temp.clone()),
        ("npm-cache", "npm 캐시", npm),
        ("pip-cache", "pip 캐시", pip),
        ("cargo-registry-cache", "cargo 레지스트리 캐시",
            bases.home.join(".cargo").join("registry").join("cache")),
    ]
}

pub fn cache_candidates(bases: &BaseDirs) -> Vec<CacheCandidate> {
    catalog(bases)
        .into_iter()
        .map(|(id, label, path)| {
            let exists = path.is_dir();
            let bytes = if exists {
                // ponytail: 규칙별 블로킹 스캔(취소 불가) — os-temp가 거대하면 느릴 수 있음.
                // UX가 문제 되면 candidates에 취소 토큰과 진행 이벤트를 추가
                scanner::scan_dir(&path, &AtomicBool::new(false), |_| {}).stats.bytes
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

/// dir이 현재 카탈로그가 가리키는 경로인지 (expand_clean_targets의 스코프 검증용 — 크기 계산 없음)
pub fn is_catalog_path(bases: &BaseDirs, dir: &Path) -> bool {
    catalog(bases).iter().any(|(_, _, p)| p == dir)
}

/// 캐시 디렉토리 자체는 보존하고 내용물만 비우기 위한 직계 자식 열거.
/// 심링크는 제외 — 이 코드베이스의 모든 순회와 동일한 방어 (scanner keep_entry, node_view 참조)
pub fn clean_targets(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
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
    fn catalog_reports_sizes_and_existence() {
        let tmp = tempfile::tempdir().unwrap();
        let bases = fake_bases(tmp.path());
        // npm 캐시만 실제로 만들어 둔다
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
        // 카탈로그에 최소 4개 규칙
        assert!(cands.len() >= 4);
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
}
