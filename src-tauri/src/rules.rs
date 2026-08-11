use std::path::{Path, PathBuf};

use same_file::Handle;

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
        ("cargo-registry-cache", "cargo 레지스트리 캐시",
            bases.home.join(".cargo").join("registry").join("cache")),
    ];

    // Windows 진단 캐시 — 조용히 수십 GB로 자라는 것들. RDP 자동 추적(RdClientAutoTrace)의 .etl 로그가
    // 대표적: 원격 접속 세션마다 쌓여 재발하므로, os-temp에 묻어두지 않고 명명 항목으로 노출해
    // 사용자가 크기를 보고 그것만 콕 집어 정리하게 한다. WER/CrashDumps도 동류의 진단 산출물.
    #[cfg(windows)]
    entries.extend([
        ("rdp-autotrace", "원격 데스크톱 추적 로그",
            bases.temp.join("DiagOutputDir").join("RdClientAutoTrace")),
        ("windows-crashdumps", "앱 크래시 덤프",
            bases.local_data.join("CrashDumps")),
        ("windows-wer", "Windows 오류 보고 (WER)",
            bases.local_data.join("Microsoft").join("Windows").join("WER")),
    ]);

    entries
}

fn metadata_is_real_directory(metadata: &std::fs::Metadata) -> bool {
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return false;
        }
    }
    true
}

fn path_is_real_directory(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|metadata| metadata_is_real_directory(&metadata))
        .unwrap_or(false)
}

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
fn handle_namespace_path(handle: &Handle, _display_path: &Path) -> Option<PathBuf> {
    use std::os::fd::AsRawFd;
    Some(PathBuf::from(format!(
        "/proc/self/fd/{}",
        handle.as_file().as_raw_fd()
    )))
}

#[cfg(target_os = "macos")]
fn handle_namespace_path(handle: &Handle, _display_path: &Path) -> Option<PathBuf> {
    use std::os::fd::AsRawFd;
    Some(PathBuf::from(format!(
        "/dev/fd/{}",
        handle.as_file().as_raw_fd()
    )))
}

#[cfg(windows)]
fn handle_namespace_path(_handle: &Handle, display_path: &Path) -> Option<PathBuf> {
    Some(display_path.to_path_buf())
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn handle_namespace_path(_handle: &Handle, _display_path: &Path) -> Option<PathBuf> {
    None
}

/// 카탈로그 루트의 경로명과 열린 디렉터리 핸들을 한 번의 권한 경계로 묶는다.
/// Unix에서는 이후 I/O를 열린 fd namespace 경로로 수행하므로 원래 경로가 rename/symlink로
/// 교체돼도 다른 디렉터리로 리다이렉트되지 않는다. Windows에서는 DELETE 공유를 제외한
/// 디렉터리 핸들을 유지해 같은 기간 루트 rename/delete 교체를 차단한다.
struct CatalogRoot {
    handle: Handle,
    display_path: PathBuf,
}

impl CatalogRoot {
    fn open(path: &Path) -> Option<Self> {
        // 1차 lstat: 명시적 symlink/reparse root를 즉시 거부.
        if !path_is_real_directory(path) {
            return None;
        }

        // 경로를 연 뒤 다시 lstat+open하고 두 핸들의 파일 ID를 비교한다. 이 순서로
        // 검사 중 경로가 바뀌는 check/use 경합도 fail-closed 한다.
        let handle = open_directory_handle(path)?;
        if !path_is_real_directory(path) {
            return None;
        }
        let current = open_directory_handle(path)?;
        if handle != current {
            return None;
        }

        Some(Self {
            handle,
            display_path: path.to_path_buf(),
        })
    }

    fn stable_path(&self) -> Option<PathBuf> {
        let stable = handle_namespace_path(&self.handle, &self.display_path)?;
        let expected = Handle::from_file(self.handle.as_file().try_clone().ok()?).ok()?;
        #[cfg(windows)]
        let observed = open_directory_handle(&stable)?;
        #[cfg(not(windows))]
        let observed = Handle::from_path(&stable).ok()?;
        (expected == observed).then_some(stable)
    }

    fn directory_size(&self) -> u64 {
        let Some(stable) = self.stable_path() else { return 0 };
        let Ok(entries) = std::fs::read_dir(stable) else { return 0 };
        let mut bytes = 0u64;

        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let Ok(metadata) = std::fs::symlink_metadata(&path) else { continue };
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

            if metadata.is_file() {
                bytes = bytes.saturating_add(metadata.len());
            } else if metadata.is_dir() {
                if let Some(child) = CatalogRoot::open(&path) {
                    bytes = bytes.saturating_add(child.directory_size());
                }
            }
        }

        bytes
    }

    fn child_paths(&self) -> Vec<PathBuf> {
        let Some(stable) = self.stable_path() else { return Vec::new() };
        let Ok(entries) = std::fs::read_dir(stable) else { return Vec::new() };

        entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let stable_child = entry.path();
                let metadata = std::fs::symlink_metadata(&stable_child).ok()?;
                if metadata.file_type().is_symlink() {
                    return None;
                }
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
                    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                        return None;
                    }
                }
                Some(self.display_path.join(entry.file_name()))
            })
            .collect()
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
/// 루트는 열린 핸들에 고정하고 직계 자식 symlink/reparse point도 제외한다.
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
        let npm = if cfg!(windows) { bases.local_data.join("npm-cache") } else { bases.home.join(".npm") };
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

        assert!(open_directory_handle(&linked).is_none());
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
    fn opened_catalog_root_cannot_be_redirected_by_path_replacement() {
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
        let names: Vec<String> = root
            .child_paths()
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["inside.bin"]);
    }
}
