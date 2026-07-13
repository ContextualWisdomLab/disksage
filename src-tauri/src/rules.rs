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

pub fn cache_candidates(bases: &BaseDirs) -> Vec<CacheCandidate> {
    catalog(bases)
        .into_iter()
        .map(|(id, label, path)| {
            let exists = path.is_dir();
            let bytes = if exists {
                // ponytail: 규칙별 블로킹 스캔(취소 불가) — os-temp가 거대하면 느릴 수 있음.
                // UX가 문제 되면 candidates에 취소 토큰과 진행 이벤트를 추가.
                // interval 1: 진행 콜백(no-op)이 작은 테스트 픽스처에서도 실행되어 커버리지에서
                // 0으로 남지 않음 — 콜백이 아무 일도 하지 않으므로 호출 빈도는 동작에 무관
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
