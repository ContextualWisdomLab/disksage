use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
#[cfg(not(coverage))]
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
#[cfg(not(coverage))]
use std::time::{Duration, Instant};

#[cfg(not(coverage))]
use tauri::{AppHandle, Emitter, Manager, State};

#[cfg(not(coverage))]
use crate::scanner;
use crate::scanner::ScanResult;

// clean_paths_inner/execute_moves_inner/undo_last_moves_inner(순수 함수)가 쓰는 것은 무조건 import; 래퍼 전용은 cfg(not(coverage))
use crate::organize;
use crate::safety;
#[cfg(not(coverage))]
use crate::{
    brew_cleanup, cloud, cloud_adr, cloud_eviction, cloud_local_eviction, cloud_plan_view,
    cloud_review, cloud_transfer, dev_artifacts, dupes, git_worktree, icloud_sync_health,
    organization_lineage,
    podman_reclaim, provider_api_client, provider_api_write, provider_capacity,
    provider_client_runtime, provider_evidence, provider_global_sync, provider_oauth,
    provider_recovery, provider_sync, rules, orphan,
};

#[cfg(not(coverage))]
#[path = "home_resolution.rs"]
mod home_resolution;

#[path = "copy_headroom.rs"]
mod copy_headroom;

#[derive(Default)]
pub struct AppState {
    pub result: Arc<Mutex<Option<ScanResult>>>,
    pub cancel: Arc<AtomicBool>,
    pub scanning: Arc<AtomicBool>,
    /// Serialize review writes with review-gated copies so a later hold cannot race a copy.
    pub cloud_review: Arc<Mutex<()>>,
    /// The latest model judgment is process-local and consumed by one execution attempt.
    pub brew_cleanup_judgment: Arc<Mutex<Option<crate::brew_cleanup::BrewCleanupJudgment>>>,
    /// Latest binary/polytomous judge calibration. It is process-local and never grants authority
    /// without the separate human confirmation phrase.
    pub judge_calibration: Arc<Mutex<Option<crate::judge_calibration::JudgeCalibrationResult>>>,
    // 엔진은 최초 사용 시 한 번만 로드해 보관(모델 로드는 ~1GB — 호출마다 재로드 금지). feature off/coverage에서는 필드 자체가 없음.
    #[cfg(all(not(coverage), feature = "llm-engine"))]
    pub engine: Arc<Mutex<Option<crate::llm::LlamaEngine>>>,
    #[cfg(all(not(coverage), feature = "llm-engine"))]
    pub verdict_cache: Arc<Mutex<crate::llm::VerdictCache>>,
}

#[derive(serde::Serialize)]
pub struct EntryView {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub is_dir: bool,
}

#[derive(serde::Serialize)]
pub struct NodeView {
    pub path: String,
    pub size: u64,
    pub entries: Vec<EntryView>,
}

/// 스캔 결과 + 실시간 read_dir로 한 레벨을 조회 (순수 함수 — 테스트 대상)
pub fn node_view(res: &ScanResult, path: &Path) -> Result<NodeView, String> {
    // '..'는 lexical starts_with를 우회해 루트 밖을 열람할 수 있음 — 컴포넌트 단위로 거부
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("path outside scanned root".into());
    }
    if !path.starts_with(&res.root) {
        return Err("path outside scanned root".into());
    }
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(path).map_err(|e| e.to_string())? {
        let Ok(entry) = entry else { continue };
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        let p = entry.path();
        let (size, is_dir) = if ft.is_dir() {
            (res.dir_sizes.get(&p).copied().unwrap_or(0), true)
        } else {
            (entry.metadata().map(|m| m.len()).unwrap_or(0), false)
        };
        entries.push(EntryView {
            name: entry.file_name().to_string_lossy().into_owned(),
            path: p.to_string_lossy().into_owned(),
            size,
            is_dir,
        });
    }
    entries.sort_by(|a, b| b.size.cmp(&a.size));
    Ok(NodeView {
        path: path.to_string_lossy().into_owned(),
        size: res.dir_sizes.get(path).copied().unwrap_or(0),
        entries,
    })
}

#[derive(serde::Serialize)]
pub struct CleanResult {
    pub path: String,
    pub ok: bool,
    pub error: String,
}

/// 정리 실행의 순수 코어 — 결과는 항목별, 하나가 실패해도 나머지는 진행 (스펙 §8)
pub fn clean_paths_inner(paths: &[PathBuf], journal_path: &Path, now_ms: u64) -> Vec<CleanResult> {
    paths
        .iter()
        .map(|p| {
            // 저널의 bytes는 감사 추적용 — 디렉토리는 재귀 합산 (metadata.len()은 dir 엔트리 자체 크기라 무의미).
            // 보호된 경로는 trash_delete가 저널링 전에 거부해 bytes를 쓰지 않으므로, 그런 경로(예: C:\Windows
            // 전체)를 재귀 스캔하는 낭비를 미리 걸러낸다 — 최종 판정은 여전히 trash_delete가 내린다.
            let bytes = if safety::is_protected(p) {
                0
            } else if p.is_dir() {
                // interval 1: 진행 콜백(no-op)이 작은 대상에서도 실행되어 커버리지에서 0으로
                // 남지 않음 — 콜백이 아무 일도 하지 않으므로 호출 빈도는 동작에 무관
                crate::scanner::scan_dir_with_interval(
                    p,
                    &std::sync::atomic::AtomicBool::new(false),
                    1,
                    |_| {},
                )
                .stats
                .bytes
            } else {
                p.metadata().map(|m| m.len()).unwrap_or(0)
            };
            match safety::trash_delete(p, bytes, journal_path, now_ms) {
                Ok(()) => CleanResult {
                    path: p.to_string_lossy().into_owned(),
                    ok: true,
                    error: String::new(),
                },
                Err(e) => CleanResult {
                    path: p.to_string_lossy().into_owned(),
                    ok: false,
                    error: e.to_string(),
                },
            }
        })
        .collect()
}

/// 개발 아티팩트는 목록 시점의 bounded metadata manifest와 일치할 때만 휴지통으로 보낸다.
/// 선택 후 재생성·변경된 target/node_modules는 경로가 같아도 재스캔을 요구한다.
pub fn clean_dev_artifacts_inner(
    requests: &[dev_artifacts::DevArtifact],
    root: &Path,
    min_age_days: u64,
    journal_path: &Path,
    now_ms: u64,
) -> Vec<CleanResult> {
    dev_artifacts::clean_artifacts(requests, root, min_age_days, journal_path, now_ms)
        .into_iter()
        .map(|result| CleanResult {
            path: result.path,
            ok: result.ok,
            error: if result
                .error
                .starts_with("development artifact changed or its bounded manifest is incomplete")
            {
                "개발 아티팩트가 변경되었거나 bounded manifest가 불완전합니다. 다시 스캔하세요".into()
            } else {
                result.error
            },
        })
        .collect()
}

/// 저널의 move 경로 필드 "src -> dst"를 분리 (순수 함수 — 테스트 대상). 구분자 없으면 None.
pub fn parse_move_entry(path_field: &str) -> Option<(String, String)> {
    path_field
        .split_once(" -> ")
        .map(|(s, d)| (s.to_string(), d.to_string()))
}

/// MovePlan을 safety::move_file로 실행하는 순수 코어 — 항목별 결과, 하나 실패해도 나머지는 진행 (M2와 동일 원칙)
pub fn execute_moves_inner(
    plans: &[organize::MovePlan],
    journal_path: &Path,
    now_ms: u64,
) -> Vec<CleanResult> {
    plans
        .iter()
        .map(|p| {
            match organize::validate_move_source(p).and_then(|_| {
                safety::move_file(Path::new(&p.src), Path::new(&p.dst), journal_path, now_ms)
                    .map_err(|error| error.to_string())
            }) {
                Ok(()) => CleanResult {
                    path: p.src.clone(),
                    ok: true,
                    error: String::new(),
                },
                Err(e) => CleanResult {
                    path: p.src.clone(),
                    ok: false,
                    error: e.to_string(),
                },
            }
        })
        .collect()
}

/// 최근 저널에서 op=="move"·outcome=="ok" 항목을 찾아 역이동(dst→src)하는 순수 코어
pub fn undo_last_moves_inner(limit: usize, journal_path: &Path, now_ms: u64) -> Vec<CleanResult> {
    // 저널은 move당 pending+ok 두 줄을 남긴다 — limit을 raw 줄 수로 쓰면 pending 잡음에
    // 밀려 실제 undo 가능한 항목이 limit보다 적게 잡힐 수 있다. 전체를 읽어 outcome=="ok"로
    // 거른 뒤에 limit을 적용해야 "최근 성공한 이동 limit개"라는 의미가 정확해진다.
    let entries = safety::journal_recent(journal_path, usize::MAX);
    entries
        .iter()
        .filter(|e| e.op == "move" && e.outcome == "ok")
        .take(limit)
        .filter_map(|e| parse_move_entry(&e.path))
        .map(|(src, dst)| {
            match safety::move_file(Path::new(&dst), Path::new(&src), journal_path, now_ms) {
                Ok(()) => CleanResult {
                    path: src,
                    ok: true,
                    error: String::new(),
                },
                Err(e) => CleanResult {
                    path: src,
                    ok: false,
                    error: e.to_string(),
                },
            }
        })
        .collect()
}

#[tauri::command]
pub fn list_roots() -> Vec<String> {
    #[cfg(windows)]
    {
        ('A'..='Z')
            .filter_map(|c| {
                let d = format!("{c}:\\");
                Path::new(&d).exists().then_some(d)
            })
            .collect()
    }
    #[cfg(not(windows))]
    {
        let mut roots = vec!["/".to_string()];
        roots.extend(std::env::var("HOME").ok());
        roots
    }
}

/// 순수: TTL 문자열 → Ontology (테스트 대상). 잘못된 TTL은 Err.
pub fn load_ontology_from(ttl: &str) -> Result<crate::ontology::Ontology, String> {
    crate::ontology::parse_ttl(ttl)
}

/// 사용자 규칙 JSON 오버라이드 로드 — app_config_dir/userrules.json, 없으면 빈 배열. 파싱은 호출부(에러 표면화).
#[cfg(not(coverage))]
fn user_rules_json(app: &AppHandle) -> String {
    use tauri::Manager;
    if let Ok(dir) = app.path().app_config_dir() {
        if let Ok(s) = std::fs::read_to_string(dir.join("userrules.json")) {
            return s;
        }
    }
    "[]".to_string()
}

#[cfg(not(coverage))]
fn bundled_ontology_ttl(app: &AppHandle) -> Result<String, String> {
    use tauri::Manager;
    if let Ok(dir) = app.path().app_config_dir() {
        let user_ttl = dir.join("ontology.ttl");
        if let Ok(s) = std::fs::read_to_string(&user_ttl) {
            return Ok(s);
        }
    }
    let res = app
        .path()
        .resolve(
            "resources/ontology/default.ttl",
            tauri::path::BaseDirectory::Resource,
        )
        .map_err(|e| e.to_string())?;
    std::fs::read_to_string(&res).map_err(|e| e.to_string())
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn get_ontology(app: AppHandle) -> Result<crate::ontology::Ontology, String> {
    load_ontology_from(&bundled_ontology_ttl(&app)?)?;
    load_ontology_from(&bundled_ontology_ttl(&app)?)
}
