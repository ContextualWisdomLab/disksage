use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

// Cache inventory is metadata-only, but a package cache can contain millions of entries. Keep
// the UI and cleanup planner responsive and fail closed when the bounded manifest is incomplete.
const CACHE_MANIFEST_BUDGET: Duration = Duration::from_secs(2);
const CACHE_MANIFEST_MAX_RECORDS: usize = 100_000;
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
    pub files: u64,
    pub skipped: u64,
    pub scan_complete: bool,
    /// Deterministic metadata manifest, not a content hash.
    pub fingerprint: String,
    pub exists: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct CacheCleanupRequest {
    pub id: String,
    pub path: String,
    pub bytes: u64,
    pub files: u64,
    pub skipped: u64,
    pub scan_complete: bool,
    pub fingerprint: String,
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

    #[cfg(windows)]
    let trivy = bases.local_data.join("trivy");
    #[cfg(target_os = "macos")]
    let trivy = bases.home.join("Library").join("Caches").join("trivy");
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let trivy = bases.local_data.join("trivy");

    #[cfg(windows)]
    let pnpm = bases.local_data.join("pnpm-cache");
    #[cfg(target_os = "macos")]
    let pnpm = bases.home.join("Library").join("Caches").join("pnpm");
    #[cfg(all(not(windows), not(target_os = "macos")))]
    let pnpm = bases.local_data.join("pnpm");

    #[cfg(windows)]
    let uv = bases.local_data.join("uv").join("cache");
    #[cfg(not(windows))]
    let uv = bases.home.join(".cache").join("uv");

    // Windows 전용 진단/트레이스 캐시는 아래 extend로 추가 — 다른 플랫폼선 그 라인이 cfg-absent라
    // mut가 미사용이므로 allow(unused_mut). (npm/pip와 같은 cfg 규율)
    #[allow(unused_mut)]
    let mut entries = vec![
        ("os-temp", "OS 임시 폴더", bases.temp.clone()),
        ("npm-cache", "npm 캐시", npm),
        ("pip-cache", "pip 캐시", pip),
        ("cargo-registry-cache", "cargo 레지스트리 캐시",
            bases.home.join(".cargo").join("registry").join("cache")),
        // 표준 개발 도구의 재생성 가능한 캐시만 노출한다. Codex·브라우저·프로젝트 데이터는
        // 사용 중이거나 작업 산출물일 수 있으므로 자동 정리 카탈로그에서 제외한다.
        ("trivy-cache", "Trivy 취약점 DB 캐시", trivy),
        ("pnpm-cache", "pnpm 패키지 캐시", pnpm),
        ("uv-cache", "uv 패키지 캐시", uv),
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
            let manifest = if exists {
                cache_manifest(&path)
            } else {
                CacheManifest::missing()
            };
            CacheCandidate {
                id: id.into(),
                label: label.into(),
                path: path.to_string_lossy().into_owned(),
                bytes: manifest.bytes,
                files: manifest.files,
                skipped: manifest.skipped,
                scan_complete: manifest.scan_complete,
                fingerprint: manifest.fingerprint,
                exists,
            }
        })
        .collect()
}

#[derive(Default)]
struct CacheManifest {
    bytes: u64,
    files: u64,
    skipped: u64,
    scan_complete: bool,
    records: Vec<String>,
    fingerprint: String,
}

impl CacheManifest {
    fn missing() -> Self {
        let mut manifest = Self::default();
        manifest.scan_complete = true;
        manifest.fingerprint = fingerprint(&["missing".to_string()]);
        manifest
    }
}

/// 캐시 디렉토리의 결정적 메타데이터 지문을 만든다. 파일 내용은 읽지 않으며 상대경로·종류·크기·mtime만
/// 포함한다. 읽기 오류가 있으면 skipped를 올려 불완전한 스캔을 정리 승인으로 오인하지 않게 한다.
fn cache_manifest(root: &Path) -> CacheManifest {
    let mut manifest = CacheManifest {
        scan_complete: true,
        ..CacheManifest::default()
    };
    let deadline = Instant::now() + CACHE_MANIFEST_BUDGET;
    collect_manifest(root, root, &mut manifest, deadline);
    if !manifest.scan_complete {
        manifest.records.push("!incomplete\0bounded-metadata-manifest".into());
    }
    manifest.records.sort_unstable();
    manifest.fingerprint = fingerprint(&manifest.records);
    manifest
}

fn collect_manifest(root: &Path, dir: &Path, manifest: &mut CacheManifest, deadline: Instant) {
    if Instant::now() >= deadline || manifest.records.len() >= CACHE_MANIFEST_MAX_RECORDS {
        manifest.scan_complete = false;
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        manifest.skipped = manifest.skipped.saturating_add(1);
        return;
    };

    for entry in entries {
        if Instant::now() >= deadline || manifest.records.len() >= CACHE_MANIFEST_MAX_RECORDS {
            manifest.scan_complete = false;
            return;
        }
        let Ok(entry) = entry else {
            manifest.skipped = manifest.skipped.saturating_add(1);
            continue;
        };
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let Ok(file_type) = entry.file_type() else {
            manifest.skipped = manifest.skipped.saturating_add(1);
            continue;
        };

        if file_type.is_symlink() {
            let target = std::fs::read_link(&path)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| "<unreadable>".into());
            manifest.records.push(format!("S\0{relative}\0{target}"));
            continue;
        }
        if file_type.is_dir() {
            manifest.records.push(format!("D\0{relative}"));
            collect_manifest(root, &path, manifest, deadline);
            continue;
        }
        if !file_type.is_file() {
            manifest.records.push(format!("O\0{relative}"));
            continue;
        }

        let Ok(metadata) = entry.metadata() else {
            manifest.skipped = manifest.skipped.saturating_add(1);
            continue;
        };
        let modified = match metadata.modified() {
            Ok(time) => match time.duration_since(std::time::UNIX_EPOCH) {
                Ok(duration) => format!("{}:{}", duration.as_secs(), duration.subsec_nanos()),
                Err(_) => {
                    manifest.skipped = manifest.skipped.saturating_add(1);
                    "<unknown>".into()
                }
            },
            Err(_) => {
                manifest.skipped = manifest.skipped.saturating_add(1);
                "<unknown>".into()
            }
        };
        manifest.bytes = manifest.bytes.saturating_add(metadata.len());
        manifest.files = manifest.files.saturating_add(1);
        manifest
            .records
            .push(format!("F\0{relative}\0{}\0{modified}", metadata.len()));
    }
}

fn fingerprint(records: &[String]) -> String {
    let mut hasher = blake3::Hasher::new();
    for record in records {
        // Length-prefix each record so a filename containing a newline cannot collide
        // with a different sequence of manifest records.
        hasher.update(&(record.len() as u64).to_le_bytes());
        hasher.update(record.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
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
        // 표준 개발 캐시까지 카탈로그에 포함되며, 후보에는 파일 수와 메타데이터 지문이 있다.
        assert!(cands.len() >= 7);
        for id in ["trivy-cache", "pnpm-cache", "uv-cache"] {
            let c = cands.iter().find(|c| c.id == id).unwrap();
            assert!(!c.exists);
            assert_eq!(c.files, 0);
            assert_eq!(c.skipped, 0);
            assert_eq!(c.fingerprint.len(), 64);
        }
    }

    #[test]
    fn cache_fingerprint_changes_when_metadata_manifest_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let bases = fake_bases(tmp.path());
        let trivy = catalog(&bases)
            .into_iter()
            .find(|(id, _, _)| *id == "trivy-cache")
            .unwrap()
            .2;
        fs::create_dir_all(&trivy).unwrap();
        fs::write(trivy.join("db.bin"), vec![0u8; 4]).unwrap();

        let first = cache_candidates(&bases)
            .into_iter()
            .find(|c| c.id == "trivy-cache")
            .unwrap();
        assert_eq!(first.files, 1);
        assert_eq!(first.bytes, 4);
        assert_eq!(first.skipped, 0);

        fs::write(trivy.join("new.bin"), vec![0u8; 4]).unwrap();
        let second = cache_candidates(&bases)
            .into_iter()
            .find(|c| c.id == "trivy-cache")
            .unwrap();
        assert_ne!(first.fingerprint, second.fingerprint);
        assert_eq!(second.files, 2);
    }

    #[test]
    fn expired_manifest_budget_is_marked_incomplete() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("fixture.bin"), b"x").unwrap();
        let mut manifest = CacheManifest {
            scan_complete: true,
            ..CacheManifest::default()
        };
        collect_manifest(
            tmp.path(),
            tmp.path(),
            &mut manifest,
            Instant::now() - Duration::from_secs(1),
        );
        assert!(!manifest.scan_complete);
        assert_eq!(manifest.files, 0);
        assert_eq!(manifest.bytes, 0);
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
