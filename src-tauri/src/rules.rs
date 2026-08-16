use crate::duplicate_audit::bound_read_root::{BoundEntryKind, BoundReadRoot};
use std::path::{Path, PathBuf};

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
        // 플랫폼별 분기를 #[cfg]로 걸어 각 타겟 빌드에 다른 쪽 arm이 아예 존재하지 않게 한다
        // (런타임 cfg!()였다면 리눅스 게이트에서 windows arm이 컴파일은 되지만 죽은 채로 남아
        // 라인 커버리지 갭이 된다 — catalog()의 npm/pip와 동일한 이유)
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

/// 정적 캐시 카탈로그 (스펙 §4 rules). 항목 = (id, 라벨, 베이스 기준 상대경로).
/// ponytail: 브라우저 캐시는 프로필 글롭이 필요해 M2 범위 밖 — 카탈로그에 추가만 하면 확장됨
fn catalog(bases: &BaseDirs) -> Vec<(&'static str, &'static str, PathBuf)> {
    // #[cfg]로 걸어 각 타겟 빌드엔 자신의 arm만 존재 — cfg!()런타임 분기였다면 리눅스 게이트에서
    // windows/macOS arm이 컴파일은 되지만 죽은 채로 남아 라인 커버리지 갭이 된다
    #[cfg(windows)]
    let npm = bases.local_data.join("npm-cache");
    #[cfg(not(windows))]
    let npm = bases.home.join(".npm"); // npm 실제 기본값 (linux/macOS)

    #[cfg(windows)]
    let pip = bases.local_data.join("pip").join("cache");
    #[cfg(target_os = "macos")]
    let pip = bases.home.join("Library").join("Caches").join("pip");
    #[cfg(not(any(windows, target_os = "macos")))]
    let pip = bases.local_data.join("pip"); // linux: ~/.cache/pip

    // Windows 전용 진단/트레이스 캐시는 아래 extend로 추가 — 다른 플랫폼선 그 라인이 cfg-absent라
    // mut가 미사용이므로 allow(unused_mut). (npm/pip와 같은 cfg 규율)
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

    // Windows 진단 캐시 — 조용히 수십 GB로 자라는 것들. RDP 자동 추적(RdClientAutoTrace)의 .etl 로그가
    // 대표적: 원격 접속 세션마다 쌓여 재발하므로, os-temp에 묻어두지 않고 명명 항목으로 노출해
    // 사용자가 크기를 보고 그것만 콕 집어 정리하게 한다. WER/CrashDumps도 동류의 진단 산출물.
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

/// Cache-catalog authority bound to one opened directory object.
///
/// Read-only sizing is descriptor-relative on Unix. Paths returned for later destructive cleanup
/// are published only if the caller pathname still names the same canonical object after child
/// enumeration, preventing a root replacement from redirecting a later cleanup operation.
struct CatalogRoot {
    guard: BoundReadRoot,
    display_path: PathBuf,
    canonical_path: PathBuf,
}

impl CatalogRoot {
    fn open(path: &Path) -> Option<Self> {
        let guard = BoundReadRoot::open(path)?;
        let canonical_path = guard.canonical_path()?;
        Some(Self {
            guard,
            display_path: path.to_path_buf(),
            canonical_path,
        })
    }

    fn directory_size_at(&self, relative: &Path) -> u64 {
        let Ok(names) = self.guard.read_dir_names(relative) else {
            return 0;
        };
        let mut bytes = 0u64;

        for name in names {
            let child = if relative.as_os_str().is_empty() {
                PathBuf::from(name)
            } else {
                relative.join(name)
            };
            match self.guard.entry_kind(&child) {
                Ok(BoundEntryKind::File) => {
                    let Ok(file) = self.guard.open_file(&child) else {
                        continue;
                    };
                    let Ok(metadata) = file.metadata() else {
                        continue;
                    };
                    if metadata.is_file() {
                        bytes = bytes.saturating_add(metadata.len());
                    }
                }
                Ok(BoundEntryKind::Directory) => {
                    bytes = bytes.saturating_add(self.directory_size_at(&child));
                }
                Ok(BoundEntryKind::Symlink | BoundEntryKind::Other) | Err(_) => {}
            }
        }

        bytes
    }

    fn directory_size(&self) -> u64 {
        self.directory_size_at(Path::new(""))
    }

    fn child_paths(&self) -> Vec<PathBuf> {
        let Ok(mut names) = self.guard.read_dir_names(Path::new("")) else {
            return Vec::new();
        };
        names.sort();
        let children = names
            .into_iter()
            .filter(|name| {
                matches!(
                    self.guard.entry_kind(Path::new(name)),
                    Ok(BoundEntryKind::File | BoundEntryKind::Directory)
                )
            })
            .map(|name| self.display_path.join(name))
            .collect::<Vec<_>>();

        if self.guard.canonical_path().as_ref() != Some(&self.canonical_path) {
            return Vec::new();
        }
        children
    }
}

pub fn cache_candidates(bases: &BaseDirs) -> Vec<CacheCandidate> {
    catalog(bases)
        .into_iter()
        .map(|(id, label, path)| {
            let root = CatalogRoot::open(&path);
            let exists = root.is_some();
            let bytes = root.as_ref().map(CatalogRoot::directory_size).unwrap_or(0);
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
    catalog(bases).iter().any(|(_, _, p)| p == dir) && CatalogRoot::open(dir).is_some()
}

/// 캐시 디렉토리 자체는 보존하고 내용물만 비우기 위한 직계 자식 열거.
/// 루트는 열린 핸들에 고정하고 직계 자식 symlink/reparse point도 제외한다. 반환 직전 루트
/// pathname을 재검증해, 열거 도중 루트가 교체되면 파괴적 후속 작업에 경로를 게시하지 않는다.
pub fn clean_targets(dir: &Path) -> Vec<PathBuf> {
    CatalogRoot::open(dir)
        .map(|root| root.child_paths())
        .unwrap_or_default()
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
        // 데스크톱 앱은 항상 사용자 세션에서 실행되므로 HOME/USERPROFILE·LOCALAPPDATA는
        // 테스트 러너에도 항상 설정돼 있다 (win/linux 공통)
        assert!(BaseDirs::from_env().is_some());
    }

    #[test]
    fn catalog_reports_sizes_and_existence() {
        let tmp = tempfile::tempdir().unwrap();
        let bases = fake_bases(tmp.path());
        // npm 캐시만 실제로 만들어 둔다 (한 줄: 각 arm이 별도 라인이면 플랫폼별로 반대쪽이
        // 영구 미커버로 남는다 — is_protected의 home 변수명 선택과 동일한 관례)
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

    #[cfg(windows)]
    #[test]
    fn catalog_includes_windows_diagnostic_caches() {
        // RDP 추적/크래시 덤프/WER를 명명 항목으로 노출 — extend arm 커버(Windows)
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
        fs::create_dir(&bases.temp).unwrap();
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

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn unix_directory_handle_open_rejects_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join("real");
        let linked = tmp.path().join("linked");
        fs::create_dir(&real).unwrap();
        std::os::unix::fs::symlink(&real, &linked).unwrap();

        assert!(BoundReadRoot::open(&linked).is_none());
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
        let candidate = cache_candidates(&bases)
            .into_iter()
            .find(|candidate| candidate.id == "os-temp")
            .unwrap();
        assert!(!candidate.exists);
        assert_eq!(candidate.bytes, 0);
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

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn opened_catalog_root_reads_bound_object_but_does_not_publish_stale_cleanup_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let catalog = tmp.path().join("catalog");
        let moved = tmp.path().join("catalog-original");
        let outside = tmp.path().join("outside");
        fs::create_dir(&catalog).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(catalog.join("inside.bin"), vec![0u8; 7]).unwrap();
        fs::write(outside.join("outside.bin"), vec![0u8; 101]).unwrap();

        let root = CatalogRoot::open(&catalog).expect("catalog root should open");
        fs::rename(&catalog, &moved).unwrap();
        std::os::unix::fs::symlink(&outside, &catalog).unwrap();

        assert_eq!(root.directory_size(), 7);
        assert!(
            root.child_paths().is_empty(),
            "mutation paths must fail closed after the caller root pathname is replaced"
        );
    }
}
