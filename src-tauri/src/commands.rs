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
use crate::dev_artifacts;
#[cfg(not(coverage))]
use crate::{
    brew_cleanup, cloud, cloud_adr, cloud_eviction, cloud_local_eviction, cloud_plan_view,
    cloud_review, cloud_transfer, dupes, git_worktree, icloud_sync_health,
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
    /// One serialized native copy cancellation token. It is reset at each command boundary.
    pub cloud_copy_cancel: Arc<AtomicBool>,
    /// Candidate fingerprint for the one native copy that may be cancelled.
    pub cloud_copy_operation: Arc<Mutex<Option<String>>>,
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
    load_ontology_from(&bundled_ontology_ttl(&app)?)
}

#[cfg(not(coverage))]
#[tauri::command(async)]
pub fn disk_inventory(
    root: String,
    app: AppHandle,
) -> Result<crate::inventory::InventoryReport, String> {
    let onto = load_ontology_from(&bundled_ontology_ttl(&app)?)?;
    let files = crate::dupes::collect_files(std::path::Path::new(&root));
    Ok(crate::inventory::build_inventory(&files, &onto))
}

/// 번들/오버라이드 온톨로지의 정합성 검사(advisory) — 불충족 클래스 목록. 로직은 Task 2의 Reasoner::check_coherence에 이미 있음.
#[cfg(not(coverage))]
#[tauri::command]
pub fn ontology_coherence(app: AppHandle) -> Result<Vec<crate::ontology::Issue>, String> {
    let onto = load_ontology_from(&bundled_ontology_ttl(&app)?)?;
    Ok(crate::ontology::Reasoner::build(&onto).check_coherence())
}

#[cfg(not(coverage))]
fn settings_file_path(app: &AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn get_settings(app: AppHandle) -> Result<crate::settings::Settings, String> {
    let path = settings_file_path(&app)?;
    match std::fs::read_to_string(&path) {
        Ok(s) => Ok(crate::settings::parse_settings(&s)),
        Err(_) => Ok(crate::settings::Settings::default()),
    }
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn set_settings(
    online_mode: bool,
    app: AppHandle,
) -> Result<crate::settings::Settings, String> {
    let s = crate::settings::Settings { online_mode };
    let path = settings_file_path(&app)?;
    std::fs::write(&path, crate::settings::serialize_settings(&s)).map_err(|e| e.to_string())?;
    Ok(s)
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn start_scan(root: String, app: AppHandle, state: State<AppState>) -> Result<(), String> {
    if state.scanning.swap(true, Ordering::SeqCst) {
        return Err("scan already running".into());
    }
    state.cancel.store(false, Ordering::SeqCst);
    let cancel = state.cancel.clone();
    let slot = state.result.clone();
    let scanning = state.scanning.clone();
    std::thread::spawn(move || {
        struct ScanningReset(Arc<AtomicBool>);
        impl Drop for ScanningReset {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let _reset = ScanningReset(scanning);
        let res = scanner::scan_dir(Path::new(&root), &cancel, |s| {
            let _ = app.emit("scan://progress", s.clone());
        });
        let stats = res.stats.clone();
        *slot.lock().unwrap() = Some(res);
        drop(_reset);
        let _ = app.emit("scan://done", stats);
    });
    Ok(())
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn cancel_scan(state: State<AppState>) {
    state.cancel.store(true, Ordering::SeqCst);
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn cancel_cloud_copy(
    metadata_fingerprint: String,
    state: State<AppState>,
) -> Result<(), String> {
    let active = state
        .cloud_copy_operation
        .lock()
        .map_err(|_| "cloud-copy-operation-lock-poisoned".to_string())?;
    if active.as_deref() != Some(metadata_fingerprint.as_str()) {
        return Err("cloud-copy-not-active".into());
    }
    state.cloud_copy_cancel.store(true, Ordering::SeqCst);
    Ok(())
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn get_node(path: String, state: State<AppState>) -> Result<NodeView, String> {
    let guard = state.result.lock().unwrap();
    let res = guard.as_ref().ok_or("no scan result")?;
    node_view(res, &PathBuf::from(path))
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn top_files(limit: usize, state: State<AppState>) -> Result<Vec<EntryView>, String> {
    let guard = state.result.lock().unwrap();
    let res = guard.as_ref().ok_or("no scan result")?;
    Ok(res
        .top_files
        .iter()
        .take(limit)
        .map(|(p, size)| EntryView {
            name: p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path: p.to_string_lossy().into_owned(),
            size: *size,
            is_dir: false,
        })
        .collect())
}

#[cfg(not(coverage))]
pub(crate) fn journal_file_path(app: &AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("journal.jsonl"))
}

#[cfg(not(coverage))]
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Returns a read-only, path-free plan for capacity held by deleted files.
#[tauri::command]
pub async fn inspect_deleted_open_files() -> Result<crate::deleted_open::DeletedOpenActionPlan, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let audit = crate::deleted_open::collect_deleted_open_audit()?;
        Ok(crate::deleted_open::plan_from_audit(audit, now_ms()))
    })
    .await
    .map_err(|_| "deleted-open-audit-worker-failed".to_string())?
}

/// Reports whether this platform can inspect deleted files held open by applications.
#[tauri::command]
pub fn deleted_open_audit_supported() -> bool {
    cfg!(unix)
}

fn valid_brew_fingerprint(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_brew_rationale(value: &str) -> bool {
    let trimmed = value.trim();
    value == trimmed
        && !trimmed.is_empty()
        && trimmed.chars().count() <= 1_000
        && !trimmed.chars().any(char::is_control)
}

#[cfg(not(coverage))]
#[tauri::command(async)]
pub fn plan_brew_cleanup() -> Result<brew_cleanup::BrewCleanupPlan, String> {
    brew_cleanup::plan(now_ms())
}

fn podman_binary() -> PathBuf {
    [
        "/opt/homebrew/bin/podman",
        "/usr/local/bin/podman",
        "/usr/bin/podman",
    ]
    .into_iter()
    .map(PathBuf::from)
    .find(|path| {
        std::fs::symlink_metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
    })
    .unwrap_or_else(|| PathBuf::from("podman"))
}

/// Read-only Podman VM/store evidence. The command never prunes, removes, trims, or stops.
#[cfg(not(coverage))]
#[tauri::command(async)]
pub fn inspect_podman_reclaim() -> podman_reclaim::PodmanReclaimPlan {
    podman_reclaim::probe_podman_reclaim(
        &podman_binary(),
        podman_reclaim::DEFAULT_PODMAN_MACHINE,
        podman_reclaim::DEFAULT_PROBE_TIMEOUT,
    )
}

/// Freshly revalidates and removes only untagged, unreferenced Podman images.
#[cfg(not(coverage))]
#[tauri::command(async)]
pub fn execute_podman_dangling_image_prune(
    confirmation_phrase: String,
    rationale: String,
) -> Result<podman_reclaim::PodmanDanglingImagePruneExecution, String> {
    if !valid_brew_rationale(&rationale) {
        return Err("podman-prune-rationale-invalid".into());
    }
    podman_reclaim::prune_dangling_images(
        &podman_binary(),
        podman_reclaim::DEFAULT_PODMAN_MACHINE,
        &confirmation_phrase,
        &rationale,
        now_ms(),
    )
}

#[cfg(not(coverage))]
#[cfg_attr(not(feature = "llm-engine"), allow(unused_variables))]
#[tauri::command(async)]
pub fn judge_brew_cleanup(
    app: AppHandle,
    state: State<AppState>,
) -> Result<brew_cleanup::BrewCleanupJudgment, String> {
    let plan = brew_cleanup::plan(now_ms())?;

    #[cfg(feature = "llm-engine")]
    {
        use tauri::Manager;
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        if !model_status_for(&model_file_path(&dir)).present {
            return Err("brew-cleanup-llm-model-unavailable".into());
        }
        let mut guard = state
            .engine
            .lock()
            .map_err(|_| "brew-cleanup-llm-engine-lock-poisoned".to_string())?;
        if guard.is_none() {
            let engine = crate::llm::LlamaEngine::new(&model_file_path(&dir))
                .map_err(|_| "brew-cleanup-llm-engine-init-failed".to_string())?;
            *guard = Some(engine);
        }
        let engine = guard
            .as_ref()
            .ok_or_else(|| "brew-cleanup-llm-engine-unavailable".to_string())?;
        let judgment = brew_cleanup::judge(engine, &plan, now_ms());
        let mut judgment = judgment;
        judgment.calibration = state
            .judge_calibration
            .lock()
            .map_err(|_| "brew-cleanup-calibration-lock-poisoned".to_string())?
            .as_ref()
            .filter(|calibration| calibration.judgment_id == judgment.judgment_id)
            .cloned();
        drop(guard);
        *state
            .brew_cleanup_judgment
            .lock()
            .map_err(|_| "brew-cleanup-judgment-lock-poisoned".to_string())? =
            (judgment.verdict == crate::llm::Verdict::Safe).then_some(judgment.clone());
        return Ok(judgment);
    }

    #[cfg(not(feature = "llm-engine"))]
    {
        let _ = (app, state);
        Err("brew-cleanup-llm-engine-disabled".into())
    }
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn validate_judge_calibration(
    evidence: crate::judge_calibration::JudgeCalibrationEvidence,
    state: State<AppState>,
) -> Result<crate::judge_calibration::JudgeCalibrationResult, String> {
    let result = crate::judge_calibration::validate(&evidence)?;
    *state
        .judge_calibration
        .lock()
        .map_err(|_| "judge-calibration-lock-poisoned".to_string())? = Some(result.clone());
    if let Some(judgment) = state
        .brew_cleanup_judgment
        .lock()
        .map_err(|_| "brew-cleanup-judgment-lock-poisoned".to_string())?
        .as_mut()
        .filter(|judgment| judgment.judgment_id == result.judgment_id)
    {
        judgment.calibration = Some(result.clone());
    }
    Ok(result)
}

#[cfg(not(coverage))]
#[tauri::command(async)]
pub fn execute_brew_cleanup(
    app: AppHandle,
    state: State<AppState>,
    plan_fingerprint: String,
    judgment_id: String,
    confirmation_phrase: String,
    rationale: String,
) -> Result<brew_cleanup::BrewCleanupExecution, String> {
    if !valid_brew_fingerprint(&plan_fingerprint) || !valid_brew_fingerprint(&judgment_id) {
        return Err("brew-cleanup-fingerprint-invalid".into());
    }
    if !valid_brew_rationale(&rationale) {
        return Err("brew-cleanup-rationale-invalid".into());
    }
    let plan = brew_cleanup::plan(now_ms())?;
    if plan.plan_fingerprint != plan_fingerprint {
        return Err("brew-cleanup-plan-stale".into());
    }
    if plan.approval_phrase() != confirmation_phrase {
        return Err("brew-cleanup-confirmation-mismatch".into());
    }

    let mut stored = state
        .brew_cleanup_judgment
        .lock()
        .map_err(|_| "brew-cleanup-judgment-lock-poisoned".to_string())?;
    let judgment = stored
        .as_ref()
        .ok_or_else(|| "brew-cleanup-llm-judgment-missing".to_string())?
        .clone();
    if judgment.judgment_id != judgment_id
        || judgment.plan_fingerprint != plan_fingerprint
        || judgment.exact_approval_phrase != plan.exact_approval_phrase
        || judgment.verdict != crate::llm::Verdict::Safe
        || !judgment.has_successful_calibration()
        || now_ms().saturating_sub(judgment.judged_at_ms) > brew_cleanup::MAX_JUDGMENT_AGE_MS
    {
        return Err("brew-cleanup-llm-judgment-stale-or-not-safe".into());
    }

    let executed_at_ms = now_ms();
    let mut execution = match brew_cleanup::execute(&plan, &judgment, executed_at_ms) {
        Ok(execution) => execution,
        Err(error) => {
            *stored = None;
            drop(stored);
            return Err(error);
        }
    };
    *stored = None;
    drop(stored);

    let audit = brew_cleanup::BrewCleanupAuditRecord {
        schema_version: brew_cleanup::SCHEMA_VERSION,
        plan,
        judgment_id: judgment.judgment_id,
        verdict: judgment.verdict,
        reason: judgment.reason,
        model_name: judgment.model_name,
        judged_at_ms: judgment.judged_at_ms,
        executed_at_ms,
        approved_by: local_human_reviewer(),
        command: execution.command.clone(),
        status_code: execution.status_code,
        stdout: execution.stdout.clone(),
        stderr: execution.stderr.clone(),
        output_truncated: execution.output_truncated,
        rationale,
    };
    let audit_result = (|| -> Result<PathBuf, String> {
        use tauri::Manager;
        let app_data_dir = app
            .path()
            .app_data_dir()
            .map_err(|_| "app-data-directory-unavailable".to_string())?;
        brew_cleanup::write_audit_record(&app_data_dir, &audit)
    })();
    match audit_result {
        Ok(path) => execution.record_path = Some(path.to_string_lossy().into_owned()),
        Err(error) => execution.record_error = Some(error),
    }
    Ok(execution)
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn list_cache_candidates() -> Result<Vec<rules::CacheCandidate>, String> {
    let bases = rules::BaseDirs::from_env().ok_or("환경변수에서 기본 경로를 찾지 못함")?;
    Ok(rules::cache_candidates(&bases))
}

#[cfg(not(coverage))]
fn clean_regenerable_caches_inner(
    bases: &rules::BaseDirs,
    journal_path: &Path,
    now_ms: u64,
) -> Vec<CleanResult> {
    crate::cache_cleanup::clean_regenerable_caches_inner(bases, journal_path, now_ms)
}

/// Move only observed, regenerable macOS cache children to Trash without an extra approval step.
/// Identity and active-use checks remain mandatory for every child, and the cache roots remain.
#[cfg(not(coverage))]
#[tauri::command]
pub fn clean_regenerable_caches(app: AppHandle) -> Result<Vec<CleanResult>, String> {
    let bases = rules::BaseDirs::from_env().ok_or("환경변수에서 기본 경로를 찾지 못함")?;
    let journal_path = journal_file_path(&app)?;
    Ok(clean_regenerable_caches_inner(
        &bases,
        &journal_path,
        now_ms(),
    ))
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn list_dev_artifacts(
    root: String,
    min_age_days: u64,
) -> Result<Vec<dev_artifacts::DevArtifact>, String> {
    Ok(dev_artifacts::find_artifacts(
        Path::new(&root),
        min_age_days,
        now_ms(),
    ))
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn clean_paths(paths: Vec<String>, app: AppHandle) -> Result<Vec<CleanResult>, String> {
    let jp = journal_file_path(&app)?;
    let pbufs: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    Ok(clean_paths_inner(&pbufs, &jp, now_ms()))
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn clean_dev_artifacts(
    root: String,
    min_age_days: u64,
    artifacts: Vec<dev_artifacts::DevArtifact>,
    app: AppHandle,
) -> Result<Vec<CleanResult>, String> {
    let jp = journal_file_path(&app)?;
    Ok(clean_dev_artifacts_inner(
        &artifacts,
        Path::new(&root),
        min_age_days,
        &jp,
        now_ms(),
    ))
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn recent_operations(
    limit: usize,
    app: AppHandle,
) -> Result<Vec<safety::JournalEntry>, String> {
    Ok(safety::journal_recent(&journal_file_path(&app)?, limit))
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn expand_clean_targets(dir: String) -> Vec<String> {
    let Some(bases) = rules::BaseDirs::from_env() else {
        return Vec::new();
    };
    let d = Path::new(&dir);
    if !rules::is_catalog_path(&bases, d) {
        return Vec::new();
    }
    rules::clean_targets(d)
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

#[cfg(not(coverage))]
#[tauri::command(async)]
pub fn find_duplicate_files(root: String) -> Result<Vec<dupes::DupeGroup>, String> {
    let files = dupes::collect_files(Path::new(&root));
    Ok(dupes::find_duplicates(files, 4096))
}

/// Resolve a real absolute home directory or fail closed. Relative environment values are never
/// accepted as path authority because they would make `~/...` destinations depend on the process
/// working directory.
#[cfg(not(coverage))]
fn resolve_home(app: &AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    let app_home = app.path().home_dir().ok();
    let home_env = std::env::var_os("HOME").map(PathBuf::from);
    let user_profile = std::env::var_os("USERPROFILE").map(PathBuf::from);
    #[cfg(windows)]
    let drive_home = home_resolution::windows_home_drive_path();
    #[cfg(not(windows))]
    let drive_home: Option<PathBuf> = None;

    home_resolution::select_absolute_home([app_home, home_env, user_profile, drive_home])
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn list_cloud_roots(app: AppHandle) -> Result<Vec<cloud::CloudRoot>, String> {
    let home = resolve_home(&app)?;
    Ok(cloud::discover_cloud_roots(&home))
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn inspect_cloud_roots(app: AppHandle) -> Result<cloud::CloudRootDiscoveryReport, String> {
    let home = resolve_home(&app)?;
    Ok(cloud::discover_cloud_roots_report(&home))
}

#[cfg(not(coverage))]
fn selected_cloud_root(app: &AppHandle, cloud_root: &str) -> Result<cloud::CloudRoot, String> {
    let home = resolve_home(app)?;
    let matches: Vec<_> = cloud::discover_cloud_roots(&home)
        .into_iter()
        .filter(|candidate| {
            cloud::cloud_root_path_matches(Path::new(&candidate.path), Path::new(cloud_root))
        })
        .collect();
    match matches.as_slice() {
        [only] => Ok(only.clone()),
        [] => Err("탐지된 클라우드 루트가 아님".into()),
        _ => Err("정규화 후 클라우드 루트가 여러 개와 일치함".into()),
    }
}

#[cfg(not(coverage))]
#[tauri::command(async)]
pub async fn plan_icloud_local_copy_eviction(
    cloud_root: String,
    path: String,
    app: AppHandle,
) -> Result<cloud_local_eviction::IcloudLocalEvictionPlan, String> {
    let selected = selected_cloud_root(&app, &cloud_root)?;
    if selected.provider != cloud::CloudProvider::Icloud {
        return Err("icloud-local-eviction-root-required".into());
    }
    cloud::validate_cloud_root_readable(&selected)?;
    let path = PathBuf::from(path);
    tauri::async_runtime::spawn_blocking(move || {
        cloud_local_eviction::plan_icloud_local_eviction(&selected, &path, cloud::system_now_ms())
    })
    .await
    .map_err(|_| "icloud-local-eviction-plan-task-failed".to_string())?
}

#[cfg(not(coverage))]
#[derive(serde::Serialize)]
pub struct IcloudLocalCopyEvictionOutput {
    pub action: &'static str,
    pub plan: cloud_local_eviction::IcloudLocalEvictionPlan,
    pub approval: cloud_local_eviction::IcloudLocalEvictionApproval,
    pub approval_path: String,
    pub result: cloud_local_eviction::IcloudLocalEvictionResult,
    pub result_path: Option<String>,
    pub result_record_error: Option<String>,
}

#[cfg(not(coverage))]
#[tauri::command(async)]
pub async fn evict_icloud_local_copy(
    cloud_root: String,
    path: String,
    approved_plan_fingerprint: String,
    confirm_plan_fingerprint: String,
    rationale: String,
    app: AppHandle,
) -> Result<IcloudLocalCopyEvictionOutput, String> {
    if approved_plan_fingerprint != confirm_plan_fingerprint {
        return Err("icloud-local-eviction-double-confirmation-mismatch".into());
    }
    let selected = selected_cloud_root(&app, &cloud_root)?;
    if selected.provider != cloud::CloudProvider::Icloud {
        return Err("icloud-local-eviction-root-required".into());
    }
    cloud::validate_cloud_root_readable(&selected)?;
    let path = PathBuf::from(path);
    use tauri::Manager;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "app-data-directory-unavailable".to_string())?;
    let record_dir = app_data_dir.join("icloud-local-evictions");
    if record_dir.starts_with(Path::new(&selected.path)) || path.starts_with(&record_dir) {
        return Err("icloud-local-eviction-record-dir-overlaps-cloud-data".into());
    }
    let approved_by = local_human_reviewer();
    tauri::async_runtime::spawn_blocking(move || {
        let record_dir = cloud_local_eviction::prepare_immutable_record_directory(
            &app_data_dir,
            Path::new(&selected.path),
            "icloud-local-evictions",
        )?;
        let plan = cloud_local_eviction::plan_icloud_local_eviction(
            &selected,
            &path,
            cloud::system_now_ms(),
        )?;
        let approval = cloud_local_eviction::approve_icloud_local_eviction(
            &plan,
            &approved_plan_fingerprint,
            cloud::system_now_ms(),
            &approved_by,
            &rationale,
        )?;
        let approval_path = cloud_local_eviction::write_immutable_record(
            &record_dir,
            &format!("{}.approval.json", approval.approval_id),
            &approval,
        )?;
        let result = cloud_local_eviction::execute_icloud_local_eviction(
            &selected,
            &plan,
            &approval,
            &confirm_plan_fingerprint,
            cloud::system_now_ms(),
        )?;
        let result_record = cloud_local_eviction::write_immutable_record(
            &record_dir,
            &format!("{}.result.json", result.result_id),
            &result,
        );
        let (result_path, result_record_error) = match result_record {
            Ok(path) => (Some(path.to_string_lossy().into_owned()), None),
            Err(error) => (None, Some(error)),
        };
        Ok(IcloudLocalCopyEvictionOutput {
            action: "evict-icloud-local-copy",
            plan,
            approval,
            approval_path: approval_path.to_string_lossy().into_owned(),
            result,
            result_path,
            result_record_error,
        })
    })
    .await
    .map_err(|_| "icloud-local-eviction-task-failed".to_string())?
}

#[cfg(not(coverage))]
#[tauri::command(async)]
pub async fn plan_stale_git_worktrees(
    repository_root: String,
    retention_references: Vec<String>,
) -> Result<git_worktree::GitWorktreeAuditReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        git_worktree::audit_git_worktrees(
            Path::new(&repository_root),
            &retention_references,
            git_worktree::GitWorktreeAuditOptions::default(),
            cloud::system_now_ms(),
        )
    })
    .await
    .map_err(|_| "git-worktree-audit-task-failed".to_string())?
}

#[cfg(not(coverage))]
#[derive(serde::Serialize)]
pub struct StaleGitWorktreeRemovalOutput {
    pub action: &'static str,
    pub report: git_worktree::GitWorktreeAuditReport,
    pub approval: git_worktree::GitWorktreeRemovalApproval,
    pub approval_path: String,
    pub result: git_worktree::GitWorktreeRemovalResult,
    pub result_path: Option<String>,
    pub result_record_error: Option<String>,
}

#[cfg(not(coverage))]
#[tauri::command(async)]
pub async fn remove_stale_git_worktrees(
    repository_root: String,
    retention_references: Vec<String>,
    approved_removal_plan_fingerprint: String,
    confirmation_exact_approval_phrase: String,
    rationale: String,
    app: AppHandle,
) -> Result<StaleGitWorktreeRemovalOutput, String> {
    use tauri::Manager;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "app-data-directory-unavailable".to_string())?;
    let approved_by = local_human_reviewer();
    tauri::async_runtime::spawn_blocking(move || {
        let options = git_worktree::GitWorktreeAuditOptions::default();
        let report = git_worktree::audit_git_worktrees(
            Path::new(&repository_root),
            &retention_references,
            options,
            cloud::system_now_ms(),
        )?;
        if report.removal_plan_fingerprint != approved_removal_plan_fingerprint {
            return Err("git-worktree-removal-plan-fingerprint-mismatch".into());
        }
        let approval = git_worktree::approve_stale_worktree_removal(
            &report,
            &confirmation_exact_approval_phrase,
            cloud::system_now_ms(),
            &approved_by,
            &rationale,
        )?;
        let record_dir = git_worktree::prepare_worktree_record_directory(
            &app_data_dir,
            &report,
            "git-worktree-removals",
        )?;
        let approval_path = git_worktree::write_immutable_worktree_record(
            &record_dir,
            &format!("{}.approval.json", approval.approval_id),
            &approval,
        )?;
        let result = git_worktree::execute_stale_worktree_removal(
            &report,
            &approval,
            &confirmation_exact_approval_phrase,
            options,
            cloud::system_now_ms(),
        )?;
        let result_record = git_worktree::write_immutable_worktree_record(
            &record_dir,
            &format!("{}.result.json", result.result_id),
            &result,
        );
        let (result_path, result_record_error) = match result_record {
            Ok(path) => (Some(path.to_string_lossy().into_owned()), None),
            Err(error) => (None, Some(error)),
        };
        Ok(StaleGitWorktreeRemovalOutput {
            action: "remove-stale-git-worktrees",
            report,
            approval,
            approval_path: approval_path.to_string_lossy().into_owned(),
            result,
            result_path,
            result_record_error,
        })
    })
    .await
    .map_err(|_| "git-worktree-removal-task-failed".to_string())?
}

/// Build a bounded, path-free ontology plan for uninstalled macOS application data.
#[cfg(not(coverage))]
#[tauri::command(async)]
pub async fn plan_orphan_cleanup(app: AppHandle) -> Result<orphan::OrphanPlan, String> {
    let home = resolve_home(&app)?;
    tauri::async_runtime::spawn_blocking(move || orphan::plan(&home, now_ms()))
        .await
        .map_err(|_| "orphan-plan-task-failed".to_string())?
}

/// Re-plan immediately before moving only fully scanned, unused cache candidates to OS Trash.
#[cfg(not(coverage))]
#[tauri::command(async)]
pub async fn clean_orphan_candidates(
    plan_fingerprint: String,
    requests: Vec<orphan::OrphanCleanupRequest>,
    confirmation_phrase: String,
    rationale: String,
    app: AppHandle,
) -> Result<orphan::OrphanCleanupResult, String> {
    if !valid_brew_fingerprint(&plan_fingerprint) {
        return Err("orphan-plan-fingerprint-invalid".into());
    }
    let home = resolve_home(&app)?;
    let plan = tauri::async_runtime::spawn_blocking({
        let home = home.clone();
        move || orphan::plan(&home, now_ms())
    })
    .await
    .map_err(|_| "orphan-clean-plan-task-failed".to_string())??;
    if plan.plan_fingerprint != plan_fingerprint {
        return Err("orphan-plan-stale".into());
    }
    let journal = journal_file_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        orphan::move_to_trash(
            &plan,
            &requests,
            &confirmation_phrase,
            &rationale,
            &journal,
            now_ms(),
        )
    })
    .await
    .map_err(|_| "orphan-clean-task-failed".to_string())?
}

#[cfg(not(coverage))]
fn oauth_connections_path(app: &AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    app.path()
        .app_data_dir()
        .map(|directory| provider_oauth::connections_path(&directory))
        .map_err(|_| "app-data-directory-unavailable".to_string())
}

#[cfg(not(coverage))]
fn cloud_review_directory(app: &AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    app.path()
        .app_data_dir()
        .map(|directory| directory.join("cloud-review-decisions"))
        .map_err(|_| "app-data-directory-unavailable".to_string())
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn list_cloud_provider_connections(
    app: AppHandle,
) -> Result<Vec<provider_oauth::OAuthConnection>, String> {
    provider_oauth::load_connections(&oauth_connections_path(&app)?)
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn list_cloud_review_decisions(
    app: AppHandle,
) -> Result<Vec<cloud_review::CloudReviewDecision>, String> {
    cloud_review::load_latest_decisions(&cloud_review_directory(&app)?)
}

#[cfg(not(coverage))]
#[tauri::command(async)]
pub async fn connect_cloud_provider(
    cloud_root: String,
    client_id: String,
    write_access: bool,
    app: AppHandle,
) -> Result<provider_oauth::OAuthConnection, String> {
    let selected = selected_cloud_root(&app, &cloud_root)?;
    cloud::validate_cloud_root_readable(&selected)?;
    if selected.provider == cloud::CloudProvider::Icloud {
        return Err("icloud-oauth-not-supported".into());
    }
    let pending = provider_oauth::prepare_authorization_with_write_access(
        selected.provider,
        &client_id,
        write_access,
    )?;
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(pending.authorization_url(), None::<&str>)
        .map_err(|_| "oauth-system-browser-open-failed".to_string())?;
    let connection_path = oauth_connections_path(&app)?;
    let connected_at_ms = cloud::system_now_ms();
    tauri::async_runtime::spawn_blocking(move || {
        provider_oauth::finish_authorization(pending, &selected, &connection_path, connected_at_ms)
    })
    .await
    .map_err(|_| "provider-oauth-task-failed".to_string())?
}

#[cfg(not(coverage))]
#[tauri::command(async)]
pub async fn disconnect_cloud_provider(cloud_root: String, app: AppHandle) -> Result<(), String> {
    let selected = selected_cloud_root(&app, &cloud_root)?;
    if selected.provider == cloud::CloudProvider::Icloud {
        return Err("icloud-oauth-not-supported".into());
    }
    let connection_path = oauth_connections_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        provider_oauth::disconnect(&connection_path, &selected)
    })
    .await
    .map_err(|_| "provider-oauth-task-failed".to_string())?
}

#[cfg(not(coverage))]
#[tauri::command(async)]
pub async fn verify_cloud_provider_capacity(
    cloud_root: String,
    app: AppHandle,
) -> Result<provider_capacity::CloudCapacitySnapshot, String> {
    let selected = selected_cloud_root(&app, &cloud_root)?;
    cloud::validate_cloud_root_readable(&selected)?;
    let observed_at_ms = cloud::system_now_ms();
    if selected.provider == cloud::CloudProvider::Icloud {
        let result = tauri::async_runtime::spawn_blocking(move || {
            provider_capacity::collect_icloud_native_capacity(observed_at_ms)
        })
        .await
        .map_err(|_| "icloud-native-quota-task-failed".to_string());
        return Ok(match result {
            Ok(Ok(snapshot)) => snapshot,
            Ok(Err(error)) | Err(error) => provider_capacity::unavailable_capacity_from_error(
                cloud::CloudProvider::Icloud,
                observed_at_ms,
                &error,
            ),
        });
    }
    let provider = selected.provider;
    let connection_path = match oauth_connections_path(&app) {
        Ok(path) => path,
        Err(error) => {
            return Ok(provider_capacity::unavailable_capacity_from_error(
                provider,
                observed_at_ms,
                &error,
            ))
        }
    };
    let result = tauri::async_runtime::spawn_blocking(move || {
        let access_token = provider_oauth::refreshed_access_token(&connection_path, &selected)?;
        provider_capacity::collect_authenticated_capacity(
            provider,
            access_token.as_str(),
            observed_at_ms,
            &provider_capacity::FixedHostProviderCapacityClient::default(),
        )
    })
    .await
    .map_err(|_| "provider-oauth-task-failed".to_string());
    let snapshot = match result {
        Ok(Ok(snapshot)) => snapshot,
        Ok(Err(error)) | Err(error) => {
            provider_capacity::unavailable_capacity_from_error(provider, observed_at_ms, &error)
        }
    };
    Ok(snapshot)
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn inspect_cloud_provider_client_runtime(
    cloud_root: String,
    app: AppHandle,
) -> Result<provider_client_runtime::ProviderClientRuntimeSnapshot, String> {
    let selected = selected_cloud_root(&app, &cloud_root)?;
    // Runtime observation must remain available while a File Provider root is temporarily
    // disconnected; this command reads the fixed provider client state, not the destination.
    Ok(provider_client_runtime::collect_provider_client_runtime(
        selected.provider,
        cloud::system_now_ms(),
    ))
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn recover_cloud_provider_client(
    cloud_root: String,
    app: AppHandle,
) -> Result<provider_recovery::ProviderRecoveryOutput, String> {
    let selected = selected_cloud_root(&app, &cloud_root)?;
    // Recovery targets only the verified, fixed desktop client. A disconnected root is the
    // condition recovery is meant to repair, so destination readability is not a precondition.
    provider_recovery::recover_provider_client(selected.provider, cloud::system_now_ms())
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn inspect_icloud_new_copy_admission(
    app: AppHandle,
) -> Result<icloud_sync_health::IcloudSyncHealthReport, String> {
    let home = resolve_home(&app)?;
    let mut report = icloud_sync_health::inspect_new_copy_admission(&home, cloud::system_now_ms())?;
    if !persist_icloud_health_evidence(&app, &report) {
        report
            .notices
            .push("icloud-sync-health-evidence-persistence-failed".into());
    }
    Ok(report)
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn inspect_cloud_provider_global_sync(
    cloud_root: String,
    app: AppHandle,
) -> Result<provider_global_sync::ProviderGlobalSyncReport, String> {
    let selected = selected_cloud_root(&app, &cloud_root)?;
    // The read-only provider dump is the evidence needed to explain an unreadable/disconnected
    // root; requiring directory access first would hide the very blocker we need to report.
    if selected.provider == cloud::CloudProvider::Icloud {
        return Err("provider-global-sync-icloud-specialized".into());
    }
    provider_global_sync::inspect_new_copy_admission(selected.provider)
}

#[cfg(not(coverage))]
struct CloudPlanningOutput {
    selected: cloud::CloudRoot,
    report: cloud::CloudPlanReport,
    icloud_health: Option<icloud_sync_health::IcloudSyncHealthReport>,
    provider_global_sync: Option<provider_global_sync::ProviderGlobalSyncReport>,
}

#[cfg(not(coverage))]
fn persist_icloud_health_evidence(
    app: &AppHandle,
    report: &icloud_sync_health::IcloudSyncHealthReport,
) -> bool {
    app.path()
        .app_data_dir()
        .ok()
        .and_then(|app_data_dir| {
            icloud_sync_health::write_icloud_sync_health_evidence(&app_data_dir, report).ok()
        })
        .is_some()
}

#[cfg(not(coverage))]
fn attach_pre_copy_evidence_cohort(
    report: &mut cloud::CloudPlanReport,
    runtime: &provider_client_runtime::ProviderClientRuntimeSnapshot,
    health: Option<&icloud_sync_health::IcloudSyncHealthReport>,
) {
    let local = report
        .local_volume
        .as_ref()
        .map(|snapshot| cloud::PreCopyEvidenceObservation {
            stream: "volume-pressure-evidence".into(),
            observed_at_ms: snapshot.observed_at_ms,
            evidence_complete: crate::volume_pressure::validate_snapshot(snapshot).is_ok(),
            fingerprint: snapshot.evidence_fingerprint.clone(),
        })
        .unwrap_or_else(|| cloud::PreCopyEvidenceObservation {
            stream: "volume-pressure-evidence".into(),
            observed_at_ms: 0,
            evidence_complete: false,
            fingerprint: "0".repeat(64),
        });
    let runtime = cloud::PreCopyEvidenceObservation {
        stream: "provider-client-runtime-evidence".into(),
        observed_at_ms: runtime.observed_at_ms,
        evidence_complete: runtime.process_observation_complete,
        fingerprint: runtime.snapshot_fingerprint_sha256.clone(),
    };
    let health = health
        .and_then(|value| icloud_sync_health::health_evidence_snapshot_from_report(value).ok())
        .map(|snapshot| cloud::PreCopyEvidenceObservation {
            stream: "icloud-sync-health-evidence".into(),
            observed_at_ms: snapshot.observed_at_ms,
            evidence_complete: snapshot.evidence_complete,
            fingerprint: snapshot.evidence_fingerprint_sha256,
        })
        .unwrap_or_else(|| cloud::PreCopyEvidenceObservation {
            stream: "icloud-sync-health-evidence".into(),
            observed_at_ms: 0,
            evidence_complete: false,
            fingerprint: "0".repeat(64),
        });
    let cohort = cloud::compare_pre_copy_evidence(vec![local, runtime, health]);
    if cohort.complete {
        report.notices.push("pre-copy-evidence-cohort-complete".into());
    } else {
        report.notices.push("pre-copy-evidence-cohort-blocked".into());
        report.notices.extend(cohort.blockers.iter().cloned());
    }
    report.pre_copy_evidence = Some(cohort);
}

#[cfg(not(coverage))]
fn cloud_plan_for_inputs(
    root: &str,
    cloud_root: &str,
    min_size_mib: u64,
    min_age_days: u64,
    limit: usize,
    app: &AppHandle,
) -> Result<CloudPlanningOutput, String> {
    let root_path = PathBuf::from(root);
    cloud::validate_source_root_readable(&root_path)?;
    let home = resolve_home(app)?;
    let discovered = cloud::discover_cloud_roots(&home);
    let selected = discovered
        .iter()
        .find(|candidate| candidate.path == cloud_root)
        .cloned()
        .ok_or_else(|| "탐지된 클라우드 루트가 아님".to_string())?;
    cloud::validate_cloud_root_readable(&selected)?;
    let excluded: Vec<PathBuf> = discovered
        .iter()
        .map(|root| PathBuf::from(&root.path))
        .collect();
    if excluded.iter().any(|cloud| root_path.starts_with(cloud)) {
        return Err("이미 클라우드 안에 있는 경로는 오프로드 원본으로 사용할 수 없음".into());
    }
    let collection = cloud::collect_archive_files_bounded(
        &root_path,
        &excluded,
        cloud::ARCHIVE_SCAN_MAX_ENTRIES,
        cloud::ARCHIVE_SCAN_MAX_DURATION,
    );
    let observed_at_ms = cloud::system_now_ms();
    let capacity_snapshot = match authenticated_capacity_snapshot(&selected, app, observed_at_ms) {
        Ok(snapshot) => snapshot,
        Err(error) => provider_capacity::unavailable_capacity_from_error(
            selected.provider,
            observed_at_ms,
            &error,
        ),
    };
    let selected =
        provider_capacity::root_with_verified_capacity_scope(&selected, &capacity_snapshot)?;
    let snapshot = cloud::prepare_cloud_archive_source_from_collection(
        &collection,
        &root_path,
        observed_at_ms,
        cloud::CloudPlanOptions {
            min_size_bytes: min_size_mib.saturating_mul(1024 * 1024),
            min_age_days,
            limit: limit.clamp(1, 1_000),
        },
    );
    let mut report = cloud::plan_cloud_archive_from_snapshot(&snapshot, &selected);
    if let Some(local_volume) = report.local_volume.as_ref() {
        let evidence_persisted = app
            .path()
            .app_data_dir()
            .map_err(|error| error.to_string())
            .and_then(|app_data_dir| {
                crate::volume_pressure::write_snapshot_evidence(&app_data_dir, local_volume)
                    .map(|_| ())
            })
            .is_ok();
        if !evidence_persisted {
            report
                .notices
                .push("local-volume-evidence-persistence-failed".into());
        }
    }
    attach_capacity_assessment(&mut report, capacity_snapshot)?;
    let runtime = provider_client_runtime::collect_provider_client_runtime(
        selected.provider,
        cloud::system_now_ms(),
    );
    let runtime_evidence_persisted = app
        .path()
        .app_data_dir()
        .ok()
        .and_then(|app_data_dir| {
            provider_client_runtime::write_runtime_snapshot_evidence(&app_data_dir, &runtime).ok()
        })
        .is_some();
    if !runtime_evidence_persisted {
        report
            .notices
            .push("provider-client-runtime-evidence-persistence-failed".into());
    }
    provider_client_runtime::attach_runtime_notice(&mut report.notices, &runtime);
    let native_client_mode = report.capacity.as_ref().is_some_and(|assessment| {
        provider_capacity::native_personal_client_copy_capacity_exception(
            selected.provider,
            selected.account_scope,
            runtime.copy_prerequisite_met,
            &assessment.snapshot,
        )
    });
    if native_client_mode {
        report.notices.push("native-client-copy-capacity-unverified".into());
    }
    let (icloud_health, provider_global_sync) = if selected.provider == cloud::CloudProvider::Icloud
    {
        let health = icloud_sync_health::inspect_new_copy_admission(&home, cloud::system_now_ms()).ok();
        if let Some(health) = health.as_ref() {
            if !persist_icloud_health_evidence(app, health) {
                report
                    .notices
                    .push("icloud-sync-health-evidence-persistence-failed".into());
            }
        }
        icloud_sync_health::attach_new_copy_admission_notice(&mut report.notices, health.as_ref());
        (health, None)
    } else {
        let global_sync = provider_global_sync::inspect_new_copy_admission(selected.provider).ok();
        provider_global_sync::attach_new_copy_admission_notice(
            &mut report.notices,
            global_sync.as_ref(),
        );
        (None, global_sync)
    };
    if selected.provider == cloud::CloudProvider::Icloud {
        attach_pre_copy_evidence_cohort(&mut report, &runtime, icloud_health.as_ref());
    }
    Ok(CloudPlanningOutput {
        selected,
        report,
        icloud_health,
        provider_global_sync,
    })
}

#[cfg(not(coverage))]
fn authenticated_capacity_snapshot(
    selected: &cloud::CloudRoot,
    app: &AppHandle,
    observed_at_ms: u64,
) -> Result<provider_capacity::CloudCapacitySnapshot, String> {
    if selected.provider == cloud::CloudProvider::Icloud {
        return provider_capacity::collect_icloud_native_capacity(observed_at_ms);
    }
    let access_token =
        provider_oauth::refreshed_access_token(&oauth_connections_path(app)?, selected)?;
    provider_capacity::collect_authenticated_capacity(
        selected.provider,
        access_token.as_str(),
        observed_at_ms,
        &provider_capacity::FixedHostProviderCapacityClient::default(),
    )
}

#[cfg(not(coverage))]
fn attach_capacity_assessment(
    report: &mut cloud::CloudPlanReport,
    snapshot: provider_capacity::CloudCapacitySnapshot,
) -> Result<(), String> {
    if snapshot.provider != report.cloud_root.provider
        || snapshot.account_scope.is_some_and(|scope| {
            report.cloud_root.account_scope != cloud::CloudAccountScope::Unknown
                && report.cloud_root.account_scope != scope
        })
    {
        return Err("cloud-capacity-root-binding-mismatch".into());
    }
    let largest_candidate_bytes = report
        .candidates
        .iter()
        .filter(|candidate| candidate.blocked_reason.is_none())
        .map(|candidate| candidate.bytes)
        .max()
        .unwrap_or_default();
    let assessment = provider_capacity::assess_capacity(
        snapshot,
        report.potentially_reclaimable_bytes,
        largest_candidate_bytes,
        provider_capacity::DEFAULT_CAPACITY_RESERVE_BYTES,
    );
    report
        .notices
        .retain(|notice| notice != "cloud-quota-unverified");
    report.notices.push(
        match assessment.can_fit {
            Some(true)
                if assessment.snapshot.evidence_kind
                    == provider_capacity::CapacityEvidenceKind::ProviderNativeStatus =>
            {
                "cloud-quota-provider-native-verified"
            }
            Some(true) => "cloud-quota-provider-api-verified",
            Some(false) => "cloud-quota-insufficient-or-blocked",
            None => "cloud-quota-unavailable",
        }
        .into(),
    );
    report.capacity = Some(assessment);
    Ok(())
}

#[cfg(not(coverage))]
fn require_capacity_for_copy(
    candidate: &cloud::CloudCandidate,
    snapshot: &provider_capacity::CloudCapacitySnapshot,
    allow_native_personal_client_exception: bool,
) -> Result<(), String> {
    let assessment = provider_capacity::assess_capacity(
        snapshot.clone(),
        candidate.bytes,
        candidate.bytes,
        provider_capacity::DEFAULT_CAPACITY_RESERVE_BYTES,
    );
    if assessment.can_fit == Some(true)
        || (allow_native_personal_client_exception
            && provider_capacity::native_personal_client_copy_capacity_exception(
                candidate.provider,
                candidate.destination_account_scope,
                true,
                snapshot,
            ))
    {
        Ok(())
    } else {
        Err(if assessment.blockers.is_empty() {
            "cloud-capacity-verification-required".into()
        } else {
            assessment.blockers.join(",")
        })
    }
}

#[cfg(not(coverage))]
fn require_local_copy_headroom(candidate: &cloud::CloudCandidate) -> Result<(), String> {
    copy_headroom::require_destination_copy_headroom(
        Path::new(&candidate.dst),
        candidate.bytes,
        cloud::system_now_ms(),
    )
}

#[cfg(not(coverage))]
#[tauri::command(async)]
pub async fn plan_cloud_archive(
    root: String,
    cloud_root: String,
    min_size_mib: u64,
    min_age_days: u64,
    limit: usize,
    app: AppHandle,
) -> Result<cloud_plan_view::CloudPlanReportView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let planning =
            cloud_plan_for_inputs(&root, &cloud_root, min_size_mib, min_age_days, limit, &app)?;
        Ok(planning.report.into())
    })
    .await
    .map_err(|_| "cloud-plan-task-failed".to_string())?
}

#[cfg(not(coverage))]
fn local_human_reviewer() -> String {
    let raw = std::env::var(if cfg!(windows) { "USERNAME" } else { "USER" })
        .unwrap_or_else(|_| "unknown".into());
    let bounded: String = raw
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
        .take(64)
        .collect();
    format!(
        "human:local:{}",
        if bounded.is_empty() {
            "unknown"
        } else {
            &bounded
        }
    )
}

#[cfg(not(coverage))]
#[tauri::command(async)]
pub async fn review_cloud_candidate(
    root: String,
    cloud_root: String,
    metadata_fingerprint: String,
    review_fingerprint: String,
    disposition: cloud_review::CloudReviewDisposition,
    rationale: String,
    min_size_mib: u64,
    min_age_days: u64,
    limit: usize,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<cloud_review::CloudReviewDecision, String> {
    for fingerprint in [&metadata_fingerprint, &review_fingerprint] {
        if fingerprint.len() != 64 || !fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("cloud-review-fingerprint-invalid".into());
        }
    }
    let cloud_review = Arc::clone(&state.cloud_review);
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = cloud_review
            .lock()
            .map_err(|_| "cloud-review-lock-poisoned".to_string())?;
        let planning =
            cloud_plan_for_inputs(&root, &cloud_root, min_size_mib, min_age_days, limit, &app)?;
        let matches: Vec<_> = planning
            .report
            .candidates
            .iter()
            .filter(|candidate| candidate.metadata_fingerprint == metadata_fingerprint)
            .collect();
        let candidate = match matches.as_slice() {
            [only] => *only,
            [] => return Err("fresh-plan-candidate-not-found".into()),
            _ => return Err("fresh-plan-candidate-ambiguous".into()),
        };
        if candidate.review_fingerprint != review_fingerprint {
            return Err("fresh-plan-review-fingerprint-mismatch".into());
        }
        let decision = cloud_review::create_attributed_decision(
            candidate,
            disposition,
            cloud::system_now_ms(),
            &local_human_reviewer(),
            &rationale,
        )?;
        cloud_review::write_immutable_decision(&cloud_review_directory(&app)?, &decision)?;
        Ok(decision)
    })
    .await
    .map_err(|_| "cloud-review-task-failed".to_string())?
}

#[cfg(not(coverage))]
#[derive(serde::Serialize)]
pub struct CloudCopyOutput {
    pub action: &'static str,
    pub goal_state: cloud_transfer::CloudOffloadGoalState,
    pub goal_status: Option<String>,
    pub receipt: cloud_transfer::CloudCopyReceipt,
    pub receipt_path: String,
    pub adr_path: Option<String>,
    pub goal_path: Option<String>,
    pub projection_warnings: Vec<String>,
    pub provider_object_id: Option<String>,
}

#[cfg(not(coverage))]
fn require_native_copy_not_cancelled(cancel: Option<&AtomicBool>) -> Result<(), String> {
    if cancel.is_some_and(|token| token.load(Ordering::SeqCst)) {
        return Err("cloud-copy-cancelled".into());
    }
    Ok(())
}

#[cfg(not(coverage))]
fn require_native_copy_not_cancelled_with_failure(
    cancel: Option<&AtomicBool>,
    candidate: &cloud::CloudCandidate,
    action: cloud_transfer::CloudCopyApprovalAction,
    failure_dir: &Path,
) -> Result<(), String> {
    let error = match require_native_copy_not_cancelled(cancel) {
        Ok(()) => return Ok(()),
        Err(error) => error,
    };
    let journal_error = cloud_transfer::record_copy_failure(
        candidate,
        action,
        &error,
        cloud::system_now_ms(),
        failure_dir,
    )
    .err();
    Err(match journal_error {
        Some(journal_error) => format!("{error};{journal_error}"),
        None => error,
    })
}

#[cfg(not(coverage))]
fn create_cloud_candidate_receipt(
    root: &str,
    cloud_root: &str,
    metadata_fingerprint: &str,
    min_size_mib: u64,
    min_age_days: u64,
    limit: usize,
    exact_confirmation_phrase: &str,
    approval_rationale: &str,
    app: &AppHandle,
    adopt_existing: bool,
    cancel: Option<&AtomicBool>,
) -> Result<CloudCopyOutput, String> {
    use tauri::Manager;
    if metadata_fingerprint.len() != 64
        || !metadata_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("metadata-fingerprint-invalid".into());
    }
    if !adopt_existing {
        require_native_copy_not_cancelled(cancel)?;
    }
    let planning =
        cloud_plan_for_inputs(root, cloud_root, min_size_mib, min_age_days, limit, app)?;
    let CloudPlanningOutput {
        selected,
        report,
        icloud_health,
        provider_global_sync,
    } = planning;
    let matches: Vec<_> = report
        .candidates
        .iter()
        .filter(|candidate| candidate.metadata_fingerprint == metadata_fingerprint)
        .collect();
    let candidate = match matches.as_slice() {
        [only] => *only,
        [] => return Err("fresh-plan-candidate-not-found".into()),
        _ => return Err("fresh-plan-candidate-ambiguous".into()),
    };
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "app-data-directory-unavailable".to_string())?;
    let receipt_dir = app_data_dir.join("cloud-receipts");
    let failure_dir = app_data_dir.join("cloud-copy-failures");
    let action = if adopt_existing {
        cloud_transfer::CloudCopyApprovalAction::AdoptExistingCopy
    } else {
        cloud_transfer::CloudCopyApprovalAction::CopyOnly
    };
    if !adopt_existing {
        require_native_copy_not_cancelled_with_failure(cancel, candidate, action, &failure_dir)?;
    }
    let review_decision = if candidate.requires_review {
        cloud_review::load_latest_decisions(&cloud_review_directory(&app)?)?
            .into_iter()
            .find(|decision| decision.candidate_fingerprint == candidate.metadata_fingerprint)
    } else {
        None
    };
    let action_at_ms = cloud::system_now_ms();
    let copy_approval = cloud_transfer::create_cloud_copy_approval(
        candidate,
        &selected,
        action,
        action_at_ms,
        &local_human_reviewer(),
        approval_rationale.trim(),
        exact_confirmation_phrase,
    )?;
    if !adopt_existing {
        // Native File Provider copies can materialize placeholders and stage more than the source
        // bytes. Re-check destination/staging headroom immediately before any mutation; adoption
        // only verifies an existing destination and does not create a local staging file.
        require_local_copy_headroom(candidate)?;
        require_native_copy_not_cancelled_with_failure(cancel, candidate, action, &failure_dir)?;
        let runtime = provider_client_runtime::require_provider_client_runtime(
            selected.provider,
            cloud::system_now_ms(),
        )?;
        require_native_copy_not_cancelled_with_failure(cancel, candidate, action, &failure_dir)?;
        if selected.provider == cloud::CloudProvider::Icloud {
            cloud::require_pre_copy_evidence_cohort(report.pre_copy_evidence.as_ref())?;
            let health = icloud_health
                .as_ref()
                .ok_or_else(|| "icloud-new-copy-admission-evidence-unavailable".to_string())?;
            icloud_sync_health::require_new_copy_admission(&health)?;
        } else {
            let global_sync = provider_global_sync
                .as_ref()
                .ok_or_else(|| "provider-global-sync-evidence-unavailable".to_string())?;
            provider_global_sync::require_new_copy_admission(global_sync)?;
        }
        require_native_copy_not_cancelled_with_failure(cancel, candidate, action, &failure_dir)?;
        let snapshot = report
            .capacity
            .as_ref()
            .ok_or_else(|| "cloud-capacity-verification-required".to_string())?;
        let native_client_mode =
            provider_capacity::native_personal_client_copy_capacity_exception(
                selected.provider,
                selected.account_scope,
                runtime.copy_prerequisite_met,
                &snapshot.snapshot,
            );
        require_capacity_for_copy(candidate, &snapshot.snapshot, native_client_mode)?;
        require_native_copy_not_cancelled_with_failure(cancel, candidate, action, &failure_dir)?;
    }
    let copy_result = if adopt_existing {
        cloud_transfer::adopt_existing_cloud_copy_with_approval(
            candidate,
            &selected,
            &receipt_dir,
            review_decision.as_ref(),
            &copy_approval,
        )
    } else {
        let cancel = cancel.ok_or_else(|| "native-copy-cancellation-unavailable".to_string())?;
        cloud_transfer::prepare_cloud_copy_with_approval_cancelable(
            candidate,
            &selected,
            &receipt_dir,
            review_decision.as_ref(),
            &copy_approval,
            cancel,
        )
    };
    let (receipt, receipt_path) = match copy_result {
        Ok(result) => result,
        Err(error) => {
            let journal_error = cloud_transfer::record_copy_failure(
                candidate,
                action,
                &error,
                cloud::system_now_ms(),
                &failure_dir,
            )
            .err();
            return Err(match journal_error {
                Some(journal_error) => format!("{error};{journal_error}"),
                None => error,
            });
        }
    };
    let mut projection_warnings = Vec::new();
    let (adr_path, goal_path) = match app.path().app_data_dir() {
        Ok(app_data_dir) => {
            let projection_updated_at_ms = cloud::system_now_ms();
            let adr = cloud_adr::initial_adr_snapshot(&receipt, projection_updated_at_ms);
            let goal = cloud_adr::initial_goal_snapshot(&receipt, projection_updated_at_ms);
            let (adr_path, goal_path, warnings) = cloud_adr::write_projection_pair(
                &app_data_dir.join("cloud-adr"),
                &adr,
                &app_data_dir.join("cloud-goals"),
                &goal,
            );
            projection_warnings.extend(warnings);
            (
                adr_path.map(|path| path.to_string_lossy().into_owned()),
                goal_path.map(|path| path.to_string_lossy().into_owned()),
            )
        }
        Err(_) => {
            projection_warnings.push("app-data-directory-unavailable".to_string());
            (None, None)
        }
    };
    let goal_status = cloud_adr::read_goal_status(
        &app_data_dir.join("cloud-goals"),
        &receipt.receipt_id,
    )
    .ok()
    .flatten();
    Ok(CloudCopyOutput {
        action: if adopt_existing {
            "adopt-existing-copy"
        } else {
            "copy-only"
        },
        goal_state: cloud_transfer::CloudOffloadGoalState::CopyVerified,
        goal_status,
        receipt,
        receipt_path: receipt_path.to_string_lossy().into_owned(),
        adr_path,
        goal_path,
        projection_warnings,
        provider_object_id: None,
    })
}

#[cfg(not(coverage))]
fn create_cloud_candidate_provider_api_receipt(
    root: &str,
    cloud_root: &str,
    metadata_fingerprint: &str,
    min_size_mib: u64,
    min_age_days: u64,
    limit: usize,
    exact_confirmation_phrase: &str,
    approval_rationale: &str,
    app: &AppHandle,
) -> Result<CloudCopyOutput, String> {
    use tauri::Manager;
    if metadata_fingerprint.len() != 64
        || !metadata_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("metadata-fingerprint-invalid".into());
    }
    let planning =
        cloud_plan_for_inputs(root, cloud_root, min_size_mib, min_age_days, limit, app)?;
    let CloudPlanningOutput {
        selected,
        report,
        ..
    } = planning;
    if selected.provider == cloud::CloudProvider::Icloud {
        return Err("provider-api-icloud-unsupported".into());
    }
    let candidate = report
        .candidates
        .iter()
        .find(|candidate| candidate.metadata_fingerprint == metadata_fingerprint)
        .ok_or_else(|| "fresh-plan-candidate-not-found".to_string())?;
    if report
        .candidates
        .iter()
        .filter(|entry| entry.metadata_fingerprint == metadata_fingerprint)
        .count()
        != 1
    {
        return Err("fresh-plan-candidate-ambiguous".into());
    }
    let connection_path = oauth_connections_path(app)?;
    let connection = provider_oauth::connection_for_root(
        &provider_oauth::load_connections(&connection_path)?,
        &selected,
    )?;
    if !provider_oauth::scope_allows_write(&connection) {
        return Err("provider-oauth-write-scope-required".into());
    }
    let capacity = report
        .capacity
        .as_ref()
        .ok_or_else(|| "cloud-capacity-verification-required".to_string())?;
    require_capacity_for_copy(candidate, &capacity.snapshot, false)?;
    let review_decision = if candidate.requires_review {
        cloud_review::load_latest_decisions(&cloud_review_directory(app)?)?
            .into_iter()
            .find(|decision| decision.candidate_fingerprint == candidate.metadata_fingerprint)
    } else {
        None
    };
    let copy_approval = cloud_transfer::create_cloud_copy_approval(
        candidate,
        &selected,
        cloud_transfer::CloudCopyApprovalAction::CopyOnly,
        cloud::system_now_ms(),
        &local_human_reviewer(),
        approval_rationale.trim(),
        exact_confirmation_phrase,
    )?;
    let copied_at_ms = cloud::system_now_ms();
    let (receipt, source_hashes) = cloud_transfer::prepare_provider_api_source_receipt(
        candidate,
        &selected,
        review_decision.as_ref(),
        &copy_approval,
        copied_at_ms,
    )?;
    let access_token = provider_oauth::refreshed_access_token(&connection_path, &selected)?;
    let upload = provider_api_write::upload_file(
        selected.provider,
        Path::new(&selected.path),
        Path::new(&candidate.dst),
        Path::new(&candidate.src),
        candidate.bytes,
        access_token.as_str(),
    )?;
    if let Err(error) = cloud_transfer::verify_provider_api_source_unchanged(candidate, &source_hashes)
    {
        let cleanup = provider_api_write::delete_uploaded_object(
            selected.provider,
            &upload.object_id,
            access_token.as_str(),
        );
        return Err(match cleanup {
            Ok(()) => error,
            Err(cleanup_error) => format!(
                "{error},provider-api-upload-cleanup-failed:{cleanup_error}"
            ),
        });
    }
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "app-data-directory-unavailable".to_string())?;
    let receipt_dir = app_data_dir.join("cloud-receipts");
    let receipt_path = match cloud_transfer::write_provider_api_receipt(&receipt, &receipt_dir) {
        Ok(path) => path,
        Err(error) => {
            let cleanup = provider_api_write::delete_uploaded_object(
                selected.provider,
                &upload.object_id,
                access_token.as_str(),
            );
            return Err(match cleanup {
                Ok(()) => error,
                Err(cleanup_error) => format!(
                    "{error},provider-api-upload-cleanup-failed:{cleanup_error}"
                ),
            });
        }
    };
    let mut projection_warnings = Vec::new();
    let (mut adr_path, mut goal_path) = match app.path().app_data_dir() {
        Ok(app_data_dir) => {
            let updated_at_ms = cloud::system_now_ms();
            let adr = cloud_adr::initial_adr_snapshot(&receipt, updated_at_ms);
            let goal = cloud_adr::initial_goal_snapshot(&receipt, updated_at_ms);
            let (adr_path, goal_path, warnings) = cloud_adr::write_projection_pair(
                &app_data_dir.join("cloud-adr"),
                &adr,
                &app_data_dir.join("cloud-goals"),
                &goal,
            );
            projection_warnings.extend(warnings);
            (
                adr_path.map(|path| path.to_string_lossy().into_owned()),
                goal_path.map(|path| path.to_string_lossy().into_owned()),
            )
        }
        Err(_) => {
            projection_warnings.push("app-data-directory-unavailable".to_string());
            (None, None)
        }
    };
    let mut goal_state = cloud_transfer::CloudOffloadGoalState::CopyVerified;
    let home = resolve_home(app)?;
    let cloud_roots = cloud::discover_cloud_roots(&home);
    let attestation_object_id = (selected.provider == cloud::CloudProvider::GoogleDrive)
        .then(|| upload.object_id.clone());
    match collect_cloud_attestation_for_receipt(
        &receipt,
        attestation_object_id,
        &app_data_dir.join("cloud-provider-evidence"),
        &app_data_dir.join("cloud-adr"),
        &app_data_dir.join("cloud-goals"),
        &connection_path,
        &cloud_roots,
        true,
    ) {
        Ok(attestation) => {
            goal_state = attestation.goal_state;
            adr_path = attestation.adr_path;
            goal_path = attestation.goal_path;
            projection_warnings.extend(attestation.projection_warnings);
        }
        Err(error) => {
            let provider_blocker = stable_reconciliation_error(&error);
            let projection_outcome =
                cloud_adr::ensure_initial_projection_pair_with_provider_state_outcome(
                    &receipt,
                    &app_data_dir.join("cloud-adr"),
                    &app_data_dir.join("cloud-goals"),
                    cloud::system_now_ms(),
                    &provider_blocker,
                );
            if let Some(path) = projection_outcome.adr_path {
                adr_path = Some(path.to_string_lossy().into_owned());
            }
            if let Some(path) = projection_outcome.goal_path {
                goal_path = Some(path.to_string_lossy().into_owned());
            }
            projection_warnings.extend(projection_outcome.warnings);
            projection_warnings.push(format!(
                "provider-attestation-incomplete:{provider_blocker}"
            ));
        }
    }
    let goal_status = cloud_adr::read_goal_status(
        &app_data_dir.join("cloud-goals"),
        &receipt.receipt_id,
    )
    .ok()
    .flatten();
    Ok(CloudCopyOutput {
        action: "copy-only",
        goal_state,
        goal_status,
        receipt,
        receipt_path: receipt_path.to_string_lossy().into_owned(),
        adr_path,
        goal_path,
        projection_warnings,
        provider_object_id: Some(upload.object_id),
    })
}

#[cfg(not(coverage))]
#[tauri::command(async)]
pub async fn copy_cloud_candidate(
    root: String,
    cloud_root: String,
    metadata_fingerprint: String,
    min_size_mib: u64,
    min_age_days: u64,
    limit: usize,
    exact_confirmation_phrase: String,
    approval_rationale: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CloudCopyOutput, String> {
    let cloud_review = Arc::clone(&state.cloud_review);
    let cloud_copy_cancel = Arc::clone(&state.cloud_copy_cancel);
    let cloud_copy_operation = Arc::clone(&state.cloud_copy_operation);
    tauri::async_runtime::spawn_blocking(move || {
        struct NativeCopyReset {
            cancel: Arc<AtomicBool>,
            operation: Arc<Mutex<Option<String>>>,
            fingerprint: String,
        }

        impl Drop for NativeCopyReset {
            fn drop(&mut self) {
                self.cancel.store(false, Ordering::SeqCst);
                if let Ok(mut active) = self.operation.lock() {
                    if active.as_deref() == Some(self.fingerprint.as_str()) {
                        *active = None;
                    }
                }
            }
        }

        {
            let mut active = cloud_copy_operation
                .lock()
                .map_err(|_| "cloud-copy-operation-lock-poisoned".to_string())?;
            if active.is_some() {
                return Err("cloud-copy-already-active".to_string());
            }
            cloud_copy_cancel.store(false, Ordering::SeqCst);
            *active = Some(metadata_fingerprint.clone());
        }
        let _reset = NativeCopyReset {
            cancel: Arc::clone(&cloud_copy_cancel),
            operation: Arc::clone(&cloud_copy_operation),
            fingerprint: metadata_fingerprint.clone(),
        };
        // Register before taking the shared review lock so a queued copy can be cancelled.
        // The token remains set if cancellation races with lock acquisition.
        let result = (|| {
            let _guard = cloud_review
                .lock()
                .map_err(|_| "cloud-review-lock-poisoned".to_string())?;
            create_cloud_candidate_receipt(
                &root,
                &cloud_root,
                &metadata_fingerprint,
                min_size_mib,
                min_age_days,
                limit,
                &exact_confirmation_phrase,
                &approval_rationale,
                &app,
                false,
                Some(&cloud_copy_cancel),
            )
        })();
        result
    })
    .await
    .map_err(|_| "cloud-copy-task-failed".to_string())?
}

#[cfg(not(coverage))]
#[tauri::command(async)]
pub async fn copy_cloud_candidate_via_provider_api(
    root: String,
    cloud_root: String,
    metadata_fingerprint: String,
    min_size_mib: u64,
    min_age_days: u64,
    limit: usize,
    exact_confirmation_phrase: String,
    approval_rationale: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CloudCopyOutput, String> {
    let cloud_review = Arc::clone(&state.cloud_review);
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = cloud_review
            .lock()
            .map_err(|_| "cloud-review-lock-poisoned".to_string())?;
        create_cloud_candidate_provider_api_receipt(
            &root,
            &cloud_root,
            &metadata_fingerprint,
            min_size_mib,
            min_age_days,
            limit,
            &exact_confirmation_phrase,
            &approval_rationale,
            &app,
        )
    })
    .await
    .map_err(|_| "cloud-provider-api-copy-task-failed".to_string())?
}

#[cfg(not(coverage))]
#[tauri::command(async)]
pub async fn adopt_existing_cloud_candidate(
    root: String,
    cloud_root: String,
    metadata_fingerprint: String,
    min_size_mib: u64,
    min_age_days: u64,
    limit: usize,
    exact_confirmation_phrase: String,
    approval_rationale: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CloudCopyOutput, String> {
    let cloud_review = Arc::clone(&state.cloud_review);
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = cloud_review
            .lock()
            .map_err(|_| "cloud-review-lock-poisoned".to_string())?;
        create_cloud_candidate_receipt(
            &root,
            &cloud_root,
            &metadata_fingerprint,
            min_size_mib,
            min_age_days,
            limit,
            &exact_confirmation_phrase,
            &approval_rationale,
            &app,
            true,
            None,
        )
    })
    .await
    .map_err(|_| "cloud-adopt-existing-task-failed".to_string())?
}

#[cfg(not(coverage))]
#[derive(serde::Serialize)]
pub struct CloudAttestationOutput {
    pub goal_state: cloud_transfer::CloudOffloadGoalState,
    pub goal_status: Option<String>,
    pub evidence: cloud_transfer::ProviderSyncEvidence,
    pub assessment: provider_sync::ProviderSyncTimelinessAssessment,
    pub evidence_record: provider_evidence::ProviderSyncEvidenceRecord,
    pub evidence_path: String,
    pub adr_path: Option<String>,
    pub goal_path: Option<String>,
    pub projection_warnings: Vec<String>,
    pub permit: Option<cloud_transfer::LocalEvictionPermit>,
    pub blockers: Vec<String>,
}

#[cfg(not(coverage))]
#[derive(Debug, serde::Serialize)]
pub struct CloudReceiptReconciliationEntry {
    pub receipt_id: Option<String>,
    pub provider: Option<cloud::CloudProvider>,
    pub goal_status: Option<String>,
    pub goal_state: Option<cloud_transfer::CloudOffloadGoalState>,
    pub provider_sync_state: Option<cloud_transfer::ProviderSyncState>,
    pub eviction_permit: bool,
    pub blockers: Vec<String>,
    pub error: Option<String>,
}

#[cfg(not(coverage))]
#[derive(Debug, serde::Serialize)]
pub struct CloudReceiptReconciliationOutput {
    pub schema_version: u32,
    pub observed_at_ms: u64,
    pub receipts_seen: u64,
    pub attested_count: u64,
    pub pending_count: u64,
    pub eviction_ready_count: u64,
    pub error_count: u64,
    pub provider_evidence_written: u64,
    pub unprocessed_count: u64,
    pub incomplete_reconciliation: bool,
    pub entries: Vec<CloudReceiptReconciliationEntry>,
    pub cloud_write_executed: bool,
    pub source_eviction_authorized: bool,
}

#[cfg(not(coverage))]
const MAX_CLOUD_RECEIPT_RECONCILIATION_ENTRIES: usize = 10_000;
#[cfg(not(coverage))]
const MAX_CLOUD_RECEIPTS_PER_RECONCILIATION: usize = 256;
#[cfg(not(coverage))]
const CLOUD_RECONCILIATION_MAX_DURATION: Duration = Duration::from_secs(30);

#[cfg(not(coverage))]
fn stable_reconciliation_error(error: &str) -> String {
    let token = error.split(',').next().unwrap_or_default();
    if !token.is_empty()
        && token.len() <= 128
        && token
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        token.to_string()
    } else {
        "provider-attestation-failed".into()
    }
}

#[cfg(not(coverage))]
fn reconcile_cloud_receipts_inner(
    receipt_dir: &Path,
    evidence_dir: &Path,
    adr_dir: &Path,
    goal_dir: &Path,
    connection_path: &Path,
    cloud_roots: &[cloud::CloudRoot],
) -> Result<CloudReceiptReconciliationOutput, String> {
    let reconciliation_started = Instant::now();
    let receipt_metadata = match std::fs::symlink_metadata(receipt_dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(CloudReceiptReconciliationOutput {
                schema_version: 1,
                observed_at_ms: cloud::system_now_ms(),
                receipts_seen: 0,
                attested_count: 0,
                pending_count: 0,
                eviction_ready_count: 0,
                error_count: 0,
                provider_evidence_written: 0,
                unprocessed_count: 0,
                incomplete_reconciliation: false,
                entries: Vec::new(),
                cloud_write_executed: false,
                source_eviction_authorized: false,
            });
        }
        Err(_) => return Err("cloud-receipt-directory-unavailable".into()),
    };
    if receipt_metadata.file_type().is_symlink() || !receipt_metadata.is_dir() {
        return Err("cloud-receipt-directory-unsafe".into());
    }
    let mut paths = std::fs::read_dir(receipt_dir)
        .map_err(|_| "cloud-receipt-directory-read-failed".to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    paths.sort();
    if paths.len() > MAX_CLOUD_RECEIPT_RECONCILIATION_ENTRIES {
        return Err("cloud-receipt-directory-entry-limit-exceeded".into());
    }
    let receipt_paths = paths
        .into_iter()
        .filter(|path| {
            let Ok(metadata) = std::fs::symlink_metadata(path) else {
                return false;
            };
            metadata.is_file()
                && !metadata.file_type().is_symlink()
                && path.extension().and_then(|value| value.to_str()) == Some("json")
        })
        .collect::<Vec<_>>();
    let mut output = CloudReceiptReconciliationOutput {
        schema_version: 1,
        observed_at_ms: cloud::system_now_ms(),
        receipts_seen: 0,
        attested_count: 0,
        pending_count: 0,
        eviction_ready_count: 0,
        error_count: 0,
        provider_evidence_written: 0,
        unprocessed_count: 0,
        incomplete_reconciliation: false,
        entries: Vec::new(),
        cloud_write_executed: false,
        source_eviction_authorized: false,
    };
    for (index, path) in receipt_paths.iter().enumerate() {
        if index >= MAX_CLOUD_RECEIPTS_PER_RECONCILIATION
            || reconciliation_started.elapsed() >= CLOUD_RECONCILIATION_MAX_DURATION
        {
            output.unprocessed_count = receipt_paths.len().saturating_sub(index) as u64;
            output.incomplete_reconciliation = output.unprocessed_count > 0;
            break;
        }
        output.receipts_seen = output.receipts_seen.saturating_add(1);
        let receipt = match cloud_transfer::read_immutable_receipt(path) {
            Ok(receipt) => receipt,
            Err(error) => {
                output.error_count = output.error_count.saturating_add(1);
                output.entries.push(CloudReceiptReconciliationEntry {
                    receipt_id: None,
                    provider: None,
                    goal_status: None,
                    goal_state: None,
                    provider_sync_state: None,
                    eviction_permit: false,
                    blockers: Vec::new(),
                    error: Some(stable_reconciliation_error(&error)),
                });
                continue;
            }
        };
        match collect_cloud_attestation_for_receipt(
            &receipt,
            None,
            evidence_dir,
            adr_dir,
            goal_dir,
            connection_path,
            cloud_roots,
            false,
        ) {
            Ok(attestation) => {
                output.attested_count = output.attested_count.saturating_add(1);
                output.provider_evidence_written =
                    output.provider_evidence_written.saturating_add(1);
                if attestation.goal_state
                    == cloud_transfer::CloudOffloadGoalState::PendingProviderSync
                {
                    output.pending_count = output.pending_count.saturating_add(1);
                }
                if attestation.permit.is_some() {
                    output.eviction_ready_count = output.eviction_ready_count.saturating_add(1);
                }
                output.entries.push(CloudReceiptReconciliationEntry {
                    receipt_id: Some(receipt.receipt_id.clone()),
                    provider: Some(receipt.provider),
                    goal_status: cloud_adr::read_goal_status(goal_dir, &receipt.receipt_id)
                        .ok()
                        .flatten(),
                    goal_state: Some(attestation.goal_state),
                    provider_sync_state: Some(attestation.evidence.sync_state),
                    eviction_permit: attestation.permit.is_some(),
                    blockers: attestation.blockers,
                    error: None,
                });
            }
            Err(error) => {
                output.error_count = output.error_count.saturating_add(1);
                let attestation_error = stable_reconciliation_error(&error);
                let projection_warnings =
                    cloud_adr::ensure_initial_projection_pair_with_provider_state_outcome(
                        &receipt,
                        adr_dir,
                        goal_dir,
                        output.observed_at_ms,
                        &attestation_error,
                    )
                    .warnings;
                let mut blockers = vec!["provider-attestation-incomplete".into()];
                if let Some(blocker) =
                    cloud_transfer::source_eviction_blocker(Path::new(&receipt.source))
                {
                    blockers.push(blocker.into());
                }
                if !projection_warnings.is_empty() {
                    blockers.push("dynamic-projection-update-incomplete".into());
                }
                let projection =
                    cloud_adr::read_projection_state(&receipt.receipt_id, adr_dir, goal_dir);
                let (goal_state, provider_sync_state) = match projection {
                    Ok(Some(state)) => {
                        blockers.push("projection-state-not-revalidated".into());
                        (Some(state.goal_state), Some(state.provider_sync_state))
                    }
                    Ok(None) => (None, None),
                    Err(_) => {
                        blockers.push("dynamic-projection-state-unavailable".into());
                        (None, None)
                    }
                };
                if goal_state == Some(cloud_transfer::CloudOffloadGoalState::PendingProviderSync) {
                    output.pending_count = output.pending_count.saturating_add(1);
                }
                output.entries.push(CloudReceiptReconciliationEntry {
                    receipt_id: Some(receipt.receipt_id.clone()),
                    provider: Some(receipt.provider),
                    goal_status: cloud_adr::read_goal_status(goal_dir, &receipt.receipt_id)
                        .ok()
                        .flatten(),
                    goal_state,
                    provider_sync_state,
                    eviction_permit: false,
                    blockers,
                    error: Some(attestation_error),
                });
            }
        }
    }
    Ok(output)
}

#[cfg(not(coverage))]
fn collect_cloud_attestation_for_receipt(
    receipt: &cloud_transfer::CloudCopyReceipt,
    object_id: Option<String>,
    evidence_dir: &Path,
    adr_dir: &Path,
    goal_dir: &Path,
    connection_path: &Path,
    cloud_roots: &[cloud::CloudRoot],
    force_provider_api: bool,
) -> Result<CloudAttestationOutput, String> {
    let confirmed_at_ms = cloud::system_now_ms();
    let evidence = match receipt.provider {
        cloud::CloudProvider::Icloud => {
            if object_id
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
            {
                return Err("icloud-provider-object-id-not-accepted".into());
            }
            provider_sync::collect_icloud_sync_evidence(receipt, confirmed_at_ms)?
        }
        cloud::CloudProvider::Onedrive | cloud::CloudProvider::GoogleDrive => {
            let destination = Path::new(&receipt.destination);
            let selected_root = cloud_roots
                .iter()
                .filter(|root| {
                    root.provider == receipt.provider
                        && destination.starts_with(Path::new(&root.path))
                })
                .max_by_key(|root| Path::new(&root.path).components().count())
                .cloned()
                .ok_or_else(|| "receipt-cloud-root-unavailable".to_string())?;
            let object_id = object_id
                .filter(|value| !value.trim().is_empty())
                .or_else(|| {
                    if receipt.provider == cloud::CloudProvider::GoogleDrive {
                        provider_evidence::latest_api_object_id(
                            evidence_dir,
                            &receipt.receipt_id,
                            receipt.provider,
                        )
                    } else {
                        None
                    }
                });
            let fallback_requested =
                receipt.provider == cloud::CloudProvider::Onedrive || object_id.is_some();
            let native_evidence = if force_provider_api {
                Err("provider-api-forced".to_string())
            } else {
                provider_sync::collect_file_provider_sync_evidence(receipt, confirmed_at_ms)
            };
            match native_evidence {
                Ok(evidence) if evidence.sync_complete || !fallback_requested => evidence,
                Err(error) if !fallback_requested => return Err(error),
                Ok(_) | Err(_) => {
                    let access_token =
                        provider_oauth::refreshed_access_token(connection_path, &selected_root)?;
                    let client = provider_api_client::FixedHostProviderMetadataClient::default();
                    match receipt.provider {
                        cloud::CloudProvider::Onedrive => {
                            if object_id.is_some() {
                                return Err("onedrive-provider-object-id-not-accepted".into());
                            }
                            let locator = provider_api_client::onedrive_path_locator(
                                Path::new(&selected_root.path),
                                Path::new(&receipt.destination),
                            )?;
                            provider_api_client::collect_authenticated_provider_api_evidence_from_source(
                                receipt,
                                &locator,
                                access_token.as_str(),
                                &client,
                                confirmed_at_ms,
                            )?
                        }
                        cloud::CloudProvider::GoogleDrive => {
                            let locator = provider_api_client::google_drive_path_locator(
                                Path::new(&selected_root.path),
                                Path::new(&receipt.destination),
                                object_id
                                    .as_deref()
                                    .ok_or_else(|| "provider-object-id-missing".to_string())?,
                            )?;
                            provider_api_client::collect_authenticated_google_drive_path_evidence_from_source(
                                receipt,
                                &locator,
                                access_token.as_str(),
                                &client,
                                confirmed_at_ms,
                            )?
                        }
                        cloud::CloudProvider::Icloud => unreachable!(),
                    }
                }
            }
        }
    };
    let assessment = provider_sync::assess_provider_sync_timeliness(receipt, &evidence)?;
    let (evidence_record, evidence_path) =
        provider_evidence::write_immutable_sync_evidence(evidence_dir, &evidence)?;
    let source_blocker = cloud_transfer::source_eviction_blocker(Path::new(&receipt.source));
    let (mut permit, mut blockers) =
        match cloud_transfer::approve_local_eviction(receipt, &evidence_record) {
            Ok(permit) => (Some(permit), Vec::new()),
            Err(blockers) => (None, blockers),
        };
    if let Some(blocker) = source_blocker {
        permit = None;
        if !blockers.iter().any(|existing| existing == blocker) {
            blockers.push(blocker.into());
        }
    }
    let goal_state =
        cloud_transfer::CloudOffloadGoalState::after_attestation(&evidence, permit.is_some());
    let mut adr = cloud_adr::snapshot_from_evidence(&evidence_record, goal_state, confirmed_at_ms);
    let mut goal = cloud_adr::goal_snapshot_from_evidence(
        receipt,
        &evidence_record,
        goal_state,
        confirmed_at_ms,
    );
    if let Some(blocker) = source_blocker {
        goal.status = "blocked".into();
        goal.completion_gates.insert("source-present".into(), false);
        adr.decision = format!("{}-source-state-unverified", adr.decision);
        adr.consequences
            .push(format!("source-state-blocked:{blocker}"));
    }
    let provider_blocker = blockers
        .iter()
        .find(|existing| Some(existing.as_str()) != source_blocker)
        .map(String::as_str);
    let projection = cloud_adr::write_projection_pair_with_state_blockers_outcome(
        adr_dir,
        &adr,
        goal_dir,
        &goal,
        source_blocker,
        provider_blocker,
    );
    Ok(CloudAttestationOutput {
        goal_state,
        goal_status: cloud_adr::read_goal_status(goal_dir, &receipt.receipt_id)
            .ok()
            .flatten(),
        evidence,
        assessment,
        evidence_record,
        evidence_path: evidence_path.to_string_lossy().into_owned(),
        adr_path: projection
            .adr_path
            .map(|path| path.to_string_lossy().into_owned()),
        goal_path: projection
            .goal_path
            .map(|path| path.to_string_lossy().into_owned()),
        projection_warnings: projection.warnings,
        permit,
        blockers,
    })
}

#[cfg(not(coverage))]
#[tauri::command(async)]
pub async fn attest_cloud_copy(
    receipt_id: String,
    object_id: Option<String>,
    app: AppHandle,
) -> Result<CloudAttestationOutput, String> {
    if receipt_id.len() != 64 || !receipt_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("receipt-id-invalid".into());
    }
    use tauri::Manager;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "app-data-directory-unavailable".to_string())?;
    let receipt_path = app_data_dir
        .join("cloud-receipts")
        .join(format!("{receipt_id}.json"));
    let evidence_dir = app_data_dir.join("cloud-provider-evidence");
    let adr_dir = app_data_dir.join("cloud-adr");
    let goal_dir = app_data_dir.join("cloud-goals");
    let connection_path = oauth_connections_path(&app)?;
    let home = resolve_home(&app)?;
    let cloud_roots = cloud::discover_cloud_roots(&home);
    tauri::async_runtime::spawn_blocking(move || {
        let receipt = cloud_transfer::read_immutable_receipt(&receipt_path)?;
        if receipt.receipt_id != receipt_id {
            return Err("receipt-id-mismatch".into());
        }
        let result = collect_cloud_attestation_for_receipt(
            &receipt,
            object_id,
            &evidence_dir,
            &adr_dir,
            &goal_dir,
            &connection_path,
            &cloud_roots,
            false,
        );
        if let Err(error) = &result {
            let provider_blocker = stable_reconciliation_error(error);
            let _ = cloud_adr::ensure_initial_projection_pair_with_provider_state_outcome(
                &receipt,
                &adr_dir,
                &goal_dir,
                cloud::system_now_ms(),
                &provider_blocker,
            );
        }
        result
    })
    .await
    .map_err(|_| "cloud-attestation-task-failed".to_string())?
}

#[cfg(not(coverage))]
#[tauri::command(async)]
pub async fn reconcile_cloud_receipts(
    app: AppHandle,
) -> Result<CloudReceiptReconciliationOutput, String> {
    use tauri::Manager;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "app-data-directory-unavailable".to_string())?;
    let receipt_dir = app_data_dir.join("cloud-receipts");
    let evidence_dir = app_data_dir.join("cloud-provider-evidence");
    let adr_dir = app_data_dir.join("cloud-adr");
    let goal_dir = app_data_dir.join("cloud-goals");
    let connection_path = oauth_connections_path(&app)?;
    let home = resolve_home(&app)?;
    let cloud_roots = cloud::discover_cloud_roots(&home);
    tauri::async_runtime::spawn_blocking(move || {
        reconcile_cloud_receipts_inner(
            &receipt_dir,
            &evidence_dir,
            &adr_dir,
            &goal_dir,
            &connection_path,
            &cloud_roots,
        )
    })
    .await
    .map_err(|_| "cloud-reconciliation-task-failed".to_string())?
}

#[cfg(not(coverage))]
#[derive(serde::Serialize)]
pub struct CloudSourceEvictionOutput {
    pub action: &'static str,
    pub goal_state: cloud_transfer::CloudOffloadGoalState,
    pub attestation: CloudAttestationOutput,
    pub approval: cloud_eviction::CloudSourceEvictionApproval,
    pub approval_path: String,
    pub eviction: cloud_eviction::CloudEvictionResult,
    pub adr_path: Option<String>,
    pub goal_path: Option<String>,
    pub projection_warnings: Vec<String>,
}

#[cfg(not(coverage))]
#[tauri::command(async)]
pub async fn trash_verified_cloud_source(
    receipt_id: String,
    confirmation_receipt_id: String,
    rationale: String,
    object_id: Option<String>,
    app: AppHandle,
) -> Result<CloudSourceEvictionOutput, String> {
    for value in [&receipt_id, &confirmation_receipt_id] {
        if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("receipt-id-invalid".into());
        }
    }
    use tauri::Manager;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "app-data-directory-unavailable".to_string())?;
    let receipt_path = app_data_dir
        .join("cloud-receipts")
        .join(format!("{receipt_id}.json"));
    let evidence_dir = app_data_dir.join("cloud-provider-evidence");
    let adr_dir = app_data_dir.join("cloud-adr");
    let goal_dir = app_data_dir.join("cloud-goals");
    let approval_dir = app_data_dir.join("cloud-source-eviction-approvals");
    let eviction_dir = app_data_dir.join("cloud-source-evictions");
    let journal_path = journal_file_path(&app)?;
    let connection_path = oauth_connections_path(&app)?;
    let home = resolve_home(&app)?;
    let cloud_roots = cloud::discover_cloud_roots(&home);
    let approved_by = local_human_reviewer();
    tauri::async_runtime::spawn_blocking(move || {
        let receipt = cloud_transfer::read_immutable_receipt(&receipt_path)?;
        if receipt.receipt_id != receipt_id {
            return Err("receipt-id-mismatch".into());
        }
        let attestation = match collect_cloud_attestation_for_receipt(
            &receipt,
            object_id,
            &evidence_dir,
            &adr_dir,
            &goal_dir,
            &connection_path,
            &cloud_roots,
            false,
        ) {
            Ok(attestation) => attestation,
            Err(error) => {
                let provider_blocker = stable_reconciliation_error(&error);
                let _ = cloud_adr::ensure_initial_projection_pair_with_provider_state_outcome(
                    &receipt,
                    &adr_dir,
                    &goal_dir,
                    cloud::system_now_ms(),
                    &provider_blocker,
                );
                return Err(error);
            }
        };
        let permit = attestation.permit.as_ref().ok_or_else(|| {
            if attestation.blockers.is_empty() {
                "source-eviction-permit-unavailable".to_string()
            } else {
                attestation.blockers.join(",")
            }
        })?;
        let active_use_observed_at_ms = cloud::system_now_ms();
        let active_use = cloud_local_eviction::observe_path_active_use(Path::new(&receipt.source));
        let approved_at_ms = cloud::system_now_ms();
        let approval = cloud_eviction::create_source_eviction_approval(
            &receipt,
            permit,
            &confirmation_receipt_id,
            approved_at_ms,
            &approved_by,
            &rationale,
            active_use_observed_at_ms,
            active_use,
        )?;
        let approval_path =
            cloud_eviction::write_immutable_source_eviction_approval(&approval_dir, &approval)?;
        let eviction = cloud_eviction::evict_source_with_human_approval(
            &receipt,
            permit,
            &approval,
            &confirmation_receipt_id,
            &eviction_dir,
            &journal_path,
            cloud::system_now_ms(),
        )?;
        let updated_at_ms = cloud::system_now_ms();
        let adr = cloud_adr::snapshot_from_evidence(
            &attestation.evidence_record,
            cloud_transfer::CloudOffloadGoalState::SourceEvicted,
            updated_at_ms,
        );
        let goal = cloud_adr::goal_snapshot_from_evidence(
            &receipt,
            &attestation.evidence_record,
            cloud_transfer::CloudOffloadGoalState::SourceEvicted,
            updated_at_ms,
        );
        let (adr_path, goal_path, projection_warnings) =
            cloud_adr::write_projection_pair(&adr_dir, &adr, &goal_dir, &goal);
        Ok(CloudSourceEvictionOutput {
            action: "attest-approve-and-trash-verified-cloud-source",
            goal_state: cloud_transfer::CloudOffloadGoalState::SourceEvicted,
            attestation,
            approval,
            approval_path: approval_path.to_string_lossy().into_owned(),
            eviction,
            adr_path: adr_path.map(|path| path.to_string_lossy().into_owned()),
            goal_path: goal_path.map(|path| path.to_string_lossy().into_owned()),
            projection_warnings,
        })
    })
    .await
    .map_err(|_| "cloud-source-eviction-task-failed".to_string())?
}

#[cfg(not(coverage))]
#[cfg_attr(not(feature = "llm-engine"), allow(unused_variables))]
#[tauri::command(async)]
pub fn plan_organize(
    root: String,
    app: AppHandle,
    state: State<AppState>,
) -> Result<Vec<organize::MovePlan>, String> {
    let onto = load_ontology_from(&bundled_ontology_ttl(&app)?)?;
    let rules = crate::userrules::parse_rules(&user_rules_json(&app))?;
    let files = dupes::collect_files_bounded(Path::new(&root), 10_000, Duration::from_secs(10))?;
    let home = resolve_home(&app)?;
    #[cfg(feature = "llm-engine")]
    {
        use tauri::Manager;
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        if model_status_for(&model_file_path(&dir)).present {
            let mut guard = state.engine.lock().unwrap();
            if guard.is_none() {
                if let Ok(e) = crate::llm::LlamaEngine::new(&model_file_path(&dir)) {
                    *guard = Some(e);
                }
            }
            if let Some(engine) = guard.as_ref() {
                let lineage_probe_count = std::cell::Cell::new(0usize);
                let pick = |p: &Path, cands: &[&str]| {
                    let mut meta = file_meta_at(p, 0, 0);
                    if lineage_probe_count.get() < organize::MAX_LINEAGE_PROBES {
                        lineage_probe_count.set(lineage_probe_count.get() + 1);
                        if let Some(lineage) = organize::lineage_metadata_for_path(p) {
                            meta.production_time_ms = lineage.production_time_ms;
                            meta.production_time_source = lineage.production_time_source;
                            meta.production_time_confidence = lineage.production_time_confidence;
                        }
                    }
                    crate::llm::pick_class(engine, &meta, cands)
                };
                return Ok(organize::plan_moves_with_metadata(
                    &files,
                    &onto,
                    &home,
                    now_ms(),
                    &rules,
                    &pick,
                    &organize::lineage_metadata_for_path,
                ));
            }
        }
    }
    Ok(organize::plan_moves_with_metadata(
        &files,
        &onto,
        &home,
        now_ms(),
        &rules,
        &|_, _| None,
        &organize::lineage_metadata_for_path,
    ))
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn export_organization_lineage(
    plans: Vec<organize::MovePlan>,
) -> Result<organization_lineage::OrganizationLineageBatch, String> {
    organization_lineage::export_move_plans(&plans, now_ms())
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn user_rules(app: AppHandle) -> Result<Vec<crate::userrules::Rule>, String> {
    crate::userrules::parse_rules(&user_rules_json(&app))
}

#[cfg(not(coverage))]
#[tauri::command(async)]
pub fn execute_moves(
    plans: Vec<organize::MovePlan>,
    app: AppHandle,
) -> Result<Vec<CleanResult>, String> {
    let jp = journal_file_path(&app)?;
    Ok(execute_moves_inner(&plans, &jp, now_ms()))
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn undo_last_moves(limit: usize, app: AppHandle) -> Result<Vec<CleanResult>, String> {
    let jp = journal_file_path(&app)?;
    Ok(undo_last_moves_inner(limit, &jp, now_ms()))
}

#[derive(serde::Serialize)]
pub struct ModelStatus {
    pub present: bool,
    pub name: String,
}

pub fn model_file_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir
        .join("models")
        .join(format!("{}.gguf", crate::llm::DEFAULT.name))
}

pub fn model_status_for(model_path: &Path) -> ModelStatus {
    ModelStatus {
        present: model_path.exists(),
        name: crate::llm::DEFAULT.name.to_string(),
    }
}

pub fn file_meta_at(path: &Path, size: u64, mtime_days: u64) -> crate::llm::FileMeta {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let parent = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    crate::llm::FileMeta {
        path: path.to_string_lossy().into_owned(),
        name,
        size,
        mtime_days,
        parent,
        production_time_ms: None,
        production_time_source: None,
        production_time_confidence: None,
    }
}

pub fn verdicts_with(
    engine: &dyn crate::llm::InferenceEngine,
    cache: &mut crate::llm::VerdictCache,
    items: &[(crate::llm::FileMeta, u64)],
) -> Vec<crate::llm::FileVerdict> {
    let mut out = Vec::with_capacity(items.len());
    for (meta, mtime_ms) in items {
        let key = crate::llm::VerdictCache::key(&meta.path, meta.size, *mtime_ms);
        if let Some(v) = cache.get(&key) {
            out.push(crate::llm::FileVerdict {
                path: meta.path.clone(),
                verdict: v,
                reason: String::new(),
            });
        } else {
            let fv = crate::llm::verdict_for(engine, meta);
            cache.put(key, fv.verdict);
            out.push(fv);
        }
    }
    out
}

#[cfg(not(coverage))]
fn meta_items(paths: &[String]) -> Vec<(crate::llm::FileMeta, u64)> {
    paths
        .iter()
        .filter_map(|p| {
            let path = std::path::Path::new(p);
            let md = std::fs::metadata(path).ok()?;
            let mtime_ms = md
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            let age_days = now_ms().saturating_sub(mtime_ms) / 86_400_000;
            Some((file_meta_at(path, md.len(), age_days), mtime_ms))
        })
        .collect()
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn model_status(app: AppHandle) -> Result<ModelStatus, String> {
    use tauri::Manager;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(model_status_for(&model_file_path(&dir)))
}

#[cfg(not(coverage))]
#[tauri::command(async)]
pub fn download_model(app: AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let path = model_file_path(&dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    crate::llm::download_to(&crate::llm::DEFAULT, &path)
}

#[cfg(not(coverage))]
#[cfg_attr(not(feature = "llm-engine"), allow(unused_variables))]
#[tauri::command(async)]
pub fn file_verdicts(
    paths: Vec<String>,
    app: AppHandle,
    state: State<AppState>,
) -> Result<Vec<crate::llm::FileVerdict>, String> {
    let items = meta_items(&paths);

    #[cfg(feature = "llm-engine")]
    {
        use tauri::Manager;
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        if model_status_for(&model_file_path(&dir)).present {
            let mut guard = state.engine.lock().unwrap();
            if guard.is_none() {
                if let Ok(e) = crate::llm::LlamaEngine::new(&model_file_path(&dir)) {
                    *guard = Some(e);
                }
            }
            if let Some(engine) = guard.as_ref() {
                let mut cache = state.verdict_cache.lock().unwrap();
                return Ok(verdicts_with(engine, &mut cache, &items));
            }
        }
    }

    Ok(items
        .iter()
        .map(|(meta, _)| crate::llm::FileVerdict {
            path: meta.path.clone(),
            verdict: crate::llm::Verdict::Unrated,
            reason: String::new(),
        })
        .collect())
}

#[cfg(not(coverage))]
#[cfg_attr(not(feature = "llm-engine"), allow(unused_variables))]
#[tauri::command(async)]
pub fn summarize_unknown_bucket(
    paths: Vec<String>,
    app: AppHandle,
    state: State<AppState>,
) -> Result<Option<String>, String> {
    if paths.is_empty() {
        return Ok(None);
    }
    let metas: Vec<crate::llm::FileMeta> = meta_items(&paths).into_iter().map(|(m, _)| m).collect();

    #[cfg(feature = "llm-engine")]
    {
        use tauri::Manager;
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        if model_status_for(&model_file_path(&dir)).present {
            let mut guard = state.engine.lock().unwrap();
            if guard.is_none() {
                if let Ok(e) = crate::llm::LlamaEngine::new(&model_file_path(&dir)) {
                    *guard = Some(e);
                }
            }
            if let Some(engine) = guard.as_ref() {
                return Ok(crate::llm::summarize_unknown(engine, &metas));
            }
        }
    }

    Ok(None)
}

#[cfg(not(coverage))]
#[cfg_attr(not(feature = "llm-engine"), allow(unused_variables))]
#[tauri::command(async)]
pub fn reason_unknown_extensions(
    samples: Vec<String>,
    app: AppHandle,
    state: State<AppState>,
) -> Result<Vec<crate::reasoning::ExtInsight>, String> {
    let exts = crate::reasoning::distinct_extensions(&samples);
    let settings = get_settings(app.clone())?;
    let ddg = crate::web::DdgLookup;
    let web_fn = |ext: &str| -> Option<String> {
        crate::web::WebLookup::file_type(&ddg, ext).ok().flatten()
    };
    let web: Option<&dyn Fn(&str) -> Option<String>> = if settings.online_mode {
        Some(&web_fn)
    } else {
        None
    };

    #[cfg(feature = "llm-engine")]
    {
        use tauri::Manager;
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        if model_status_for(&model_file_path(&dir)).present {
            let onto = load_ontology_from(&bundled_ontology_ttl(&app)?)?;
            let candidates: Vec<String> = onto
                .classes
                .iter()
                .map(|c| c.id.rsplit(['#', '/']).next().unwrap_or(&c.id).to_string())
                .collect();
            let cand_refs: Vec<&str> = candidates.iter().map(|s| s.as_str()).collect();
            let mut guard = state.engine.lock().unwrap();
            if guard.is_none() {
                if let Ok(e) = crate::llm::LlamaEngine::new(&model_file_path(&dir)) {
                    *guard = Some(e);
                }
            }
            if let Some(engine) = guard.as_ref() {
                let reason = |ext: &str| crate::llm::reason_extension(engine, ext, &cand_refs);
                return Ok(crate::reasoning::build_insights(&exts, &reason, web));
            }
        }
    }

    let reason = |_: &str| -> Option<crate::llm::ExtReasoning> { None };
    Ok(crate::reasoning::build_insights(&exts, &reason, web))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::scan_dir_with_interval;
    use std::fs;
    use std::sync::atomic::AtomicBool;

    use crate::llm::{InferenceEngine, Verdict, VerdictCache};

    struct CountingFake {
        out: String,
        calls: std::cell::Cell<usize>,
    }
    impl InferenceEngine for CountingFake {
        fn infer(&self, _p: &str) -> Result<String, String> {
            self.calls.set(self.calls.get() + 1);
            Ok(self.out.clone())
        }
    }

    #[test]
    fn model_file_path_is_under_models_dir() {
        let p = model_file_path(std::path::Path::new("/data"));
        assert!(p.ends_with(format!("{}.gguf", crate::llm::DEFAULT.name)));
        assert!(p.to_string_lossy().contains("models"));
    }

    #[cfg(not(coverage))]
    #[test]
    fn reconciliation_error_output_is_stable_and_path_free() {
        assert_eq!(
            stable_reconciliation_error("provider-oauth-refresh-failed,secret/path"),
            "provider-oauth-refresh-failed"
        );
        assert_eq!(
            stable_reconciliation_error("No such file or directory (os error 2)"),
            "provider-attestation-failed"
        );
    }

    #[cfg(not(coverage))]
    #[test]
    fn reconciliation_without_receipts_is_read_only() {
        let temporary = tempfile::tempdir().unwrap();
        let output = reconcile_cloud_receipts_inner(
            &temporary.path().join("missing-receipts"),
            &temporary.path().join("evidence"),
            &temporary.path().join("adr"),
            &temporary.path().join("goals"),
            &temporary.path().join("oauth.json"),
            &[],
        )
        .unwrap();
        assert_eq!(output.receipts_seen, 0);
        assert_eq!(output.attested_count, 0);
        assert!(!output.cloud_write_executed);
        assert!(!output.source_eviction_authorized);
    }

    #[cfg(not(coverage))]
    #[test]
    fn reconciliation_reports_receipts_left_after_entry_budget() {
        let temporary = tempfile::tempdir().unwrap();
        let receipts = temporary.path().join("receipts");
        std::fs::create_dir(&receipts).unwrap();
        for index in 0..=MAX_CLOUD_RECEIPTS_PER_RECONCILIATION {
            std::fs::write(receipts.join(format!("{index:04}.json")), b"{}").unwrap();
        }
        let output = reconcile_cloud_receipts_inner(
            &receipts,
            &temporary.path().join("evidence"),
            &temporary.path().join("adr"),
            &temporary.path().join("goals"),
            &temporary.path().join("oauth.json"),
            &[],
        )
        .unwrap();
        assert_eq!(output.receipts_seen, MAX_CLOUD_RECEIPTS_PER_RECONCILIATION as u64);
        assert_eq!(output.unprocessed_count, 1);
        assert!(output.incomplete_reconciliation);
        assert_eq!(output.error_count, MAX_CLOUD_RECEIPTS_PER_RECONCILIATION as u64);
    }

    #[cfg(not(coverage))]
    #[test]
    fn missing_source_blocks_eviction_permit() {
        let temporary = tempfile::tempdir().unwrap();
        let missing = temporary.path().join("missing.bin");
        assert_eq!(
            cloud_transfer::source_eviction_blocker(&missing),
            Some("source-not-present")
        );
        std::fs::write(&missing, b"source").unwrap();
        assert_eq!(cloud_transfer::source_eviction_blocker(&missing), None);
    }

    #[test]
    fn model_status_reflects_presence() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("no.gguf");
        assert!(!model_status_for(&missing).present);
        let there = tmp.path().join("m.gguf");
        std::fs::write(&there, b"x").unwrap();
        assert!(model_status_for(&there).present);
        assert_eq!(model_status_for(&there).name, crate::llm::DEFAULT.name);
    }

    #[test]
    fn brew_cleanup_inputs_are_bounded_and_exact() {
        assert!(valid_brew_fingerprint(&"a".repeat(64)));
        assert!(!valid_brew_fingerprint(&"a".repeat(63)));
        assert!(!valid_brew_fingerprint(&format!("{}g", "a".repeat(63))));
        assert!(valid_brew_rationale("reviewed dry-run output"));
        assert!(!valid_brew_rationale(" leading-space"));
        assert!(!valid_brew_rationale("control\ncharacter"));
    }

    #[test]
    fn file_meta_at_extracts_name_and_parent() {
        let m = file_meta_at(std::path::Path::new("/downloads/report.pdf"), 42, 7);
        assert_eq!(m.name, "report.pdf");
        assert_eq!(m.parent, "downloads");
        assert_eq!(m.size, 42);
        assert_eq!(m.mtime_days, 7);
        let root = file_meta_at(std::path::Path::new("/"), 0, 0);
        assert_eq!(root.name, "");
        assert_eq!(root.parent, "");
    }

    #[test]
    fn verdicts_with_caches_and_avoids_reinference() {
        let engine = CountingFake {
            out: r#"{"verdict":"safe","reason":"r"}"#.into(),
            calls: std::cell::Cell::new(0),
        };
        let mut cache = VerdictCache::new();
        let meta = file_meta_at(std::path::Path::new("/x/a.bin"), 100, 1);
        let items = vec![(meta.clone(), 1700u64), (meta, 1700u64)];
        let out = verdicts_with(&engine, &mut cache, &items);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|fv| fv.verdict == Verdict::Safe));
        assert_eq!(engine.calls.get(), 1);
    }

    #[test]
    fn verdicts_with_distinct_items_infer_each() {
        let engine = CountingFake {
            out: r#"{"verdict":"keep"}"#.into(),
            calls: std::cell::Cell::new(0),
        };
        let mut cache = VerdictCache::new();
        let a = (file_meta_at(std::path::Path::new("/x/a"), 1, 1), 10u64);
        let b = (file_meta_at(std::path::Path::new("/x/b"), 2, 2), 20u64);
        let out = verdicts_with(&engine, &mut cache, &[a, b]);
        assert_eq!(out.len(), 2);
        assert_eq!(engine.calls.get(), 2);
        let _ = out;
    }

    fn scan(root: &Path) -> ScanResult {
        scan_dir_with_interval(root, &AtomicBool::new(false), 1, |_| {})
    }

    #[test]
    fn load_ontology_from_valid_ttl_ok() {
        let ttl = r#"
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix dm: <https://disksage.app/ontology#> .
dm:Image a owl:Class ; rdfs:label "이미지"@ko .
"#;
        let onto = load_ontology_from(ttl).unwrap();
        assert_eq!(onto.classes.len(), 1);
    }

    #[test]
    fn load_ontology_from_garbage_is_err() {
        assert!(load_ontology_from("@@@ not turtle").is_err());
    }

    #[test]
    fn node_view_lists_entries_sorted_by_size_desc() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir(root.join("sub")).unwrap();
        fs::write(root.join("sub").join("inner.bin"), vec![0u8; 500]).unwrap();
        fs::write(root.join("small.txt"), vec![0u8; 10]).unwrap();
        let res = scan(root);
        let view = node_view(&res, root).unwrap();
        assert_eq!(view.size, 510);
        assert_eq!(view.entries.len(), 2);
        assert_eq!(view.entries[0].name, "sub");
        assert!(view.entries[0].is_dir);
        assert_eq!(view.entries[0].size, 500);
        assert_eq!(view.entries[1].name, "small.txt");
        assert!(!view.entries[1].is_dir);
    }

    #[test]
    fn node_view_rejects_path_outside_root() {
        let tmp = tempfile::tempdir().unwrap();
        let res = scan(tmp.path());
        assert!(node_view(&res, &std::env::temp_dir().join("..")).is_err());
    }

    #[test]
    fn node_view_rejects_parent_dir_components() {
        let tmp = tempfile::tempdir().unwrap();
        let res = scan(tmp.path());
        let sneaky = tmp.path().join("..");
        assert!(node_view(&res, &sneaky).is_err());
    }

    #[test]
    fn node_view_rejects_sibling_path_outside_root() {
        let tmp = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let res = scan(tmp.path());
        assert!(node_view(&res, other.path()).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn node_view_skips_junctions() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::create_dir(root.join("real")).unwrap();
        let junction = root.join("junc");
        let status = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(root.join("real"))
            .status()
            .unwrap();
        assert!(status.success(), "mklink /J failed");
        let res = scan(root);
        let view = node_view(&res, root).unwrap();
        assert!(view.entries.iter().all(|e| e.name != "junc"));
    }

    #[test]
    fn node_view_errors_on_unreadable_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let res = scan(tmp.path());
        assert!(node_view(&res, &tmp.path().join("missing")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn node_view_skips_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        fs::write(root.join("real.bin"), vec![0u8; 5]).unwrap();
        std::os::unix::fs::symlink(root.join("real.bin"), root.join("link.bin")).unwrap();
        let res = scan(root);
        let view = node_view(&res, root).unwrap();
        assert!(view.entries.iter().all(|e| e.name != "link.bin"));
    }

    #[test]
    fn parse_move_entry_splits_valid_entry() {
        assert_eq!(
            parse_move_entry("/a/b -> /c/d"),
            Some(("/a/b".to_string(), "/c/d".to_string()))
        );
    }

    #[test]
    fn parse_move_entry_malformed_is_none() {
        assert_eq!(parse_move_entry("no arrow here"), None);
    }

    #[test]
    fn list_roots_returns_platform_roots() {
        let roots = list_roots();
        assert!(!roots.is_empty());
        #[cfg(windows)]
        assert!(roots.iter().any(|r| r.ends_with(":\\")));
        #[cfg(not(windows))]
        assert!(roots.contains(&"/".to_string()));
    }

    #[test]
    fn clean_paths_inner_reports_per_item_results() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let ok_dir = tmp.path().join("disksage-clean-fixture-dir");
        fs::create_dir(&ok_dir).unwrap();
        fs::write(ok_dir.join("inner.bin"), vec![0u8; 32]).unwrap();
        let ok_file = tmp.path().join("disksage-clean-fixture-file.bin");
        fs::write(&ok_file, vec![0u8; 16]).unwrap();
        let missing = tmp.path().join("ghost");
        let protected =
            std::path::PathBuf::from(if cfg!(windows) { "C:\\Windows" } else { "/usr" });

        let results = clean_paths_inner(
            &[ok_dir.clone(), ok_file.clone(), missing, protected],
            &jp,
            7,
        );

        assert_eq!(results.len(), 4);
        assert!(results[0].ok);
        assert!(results[1].ok);
        assert!(!results[2].ok && results[2].error.contains("휴지통"));
        assert!(!results[3].ok && results[3].error.contains("보호"));
        assert!(!ok_dir.exists());
        assert!(!ok_file.exists());

        let recent = crate::safety::journal_recent(&jp, 10);
        let ok_entry = recent
            .iter()
            .find(|e| e.outcome == "ok" && e.path.contains("disksage-clean-fixture-dir"))
            .unwrap();
        assert_eq!(ok_entry.bytes, 32);
        let ok_file_entry = recent
            .iter()
            .find(|e| e.outcome == "ok" && e.path.contains("disksage-clean-fixture-file"))
            .unwrap();
        assert_eq!(ok_file_entry.bytes, 16);

        #[cfg(any(windows, target_os = "linux"))]
        {
            let items: Vec<_> = trash::os_limited::list()
                .unwrap()
                .into_iter()
                .filter(|i| {
                    let n = i.name.to_string_lossy();
                    n.contains("disksage-clean-fixture-dir")
                        || n.contains("disksage-clean-fixture-file")
                })
                .collect();
            trash::os_limited::purge_all(items).unwrap();
        }
    }

    #[cfg(all(not(coverage), target_os = "macos"))]
    #[test]
    fn automatic_cache_cleanup_uses_only_observed_macos_cache_ids() {
        assert_eq!(
            crate::cache_cleanup::AUTO_REGENERABLE_CACHE_IDS,
            [
                "npm-cache",
                "pnpm-cache",
                "adobe-cache",
                "edge-cache",
                "uv-cache",
                "trivy-cache",
            ]
        );
        let tmp = tempfile::tempdir().unwrap();
        let bases = crate::rules::BaseDirs {
            temp: tmp.path().join("tmp"),
            local_data: tmp.path().join("local"),
            home: tmp.path().join("home"),
        };
        for id in crate::cache_cleanup::AUTO_REGENERABLE_CACHE_IDS {
            let path = match id {
                "npm-cache" => bases.home.join(".npm"),
                "pnpm-cache" => bases.home.join("Library/Caches/pnpm"),
                "adobe-cache" => bases.home.join("Library/Caches/Adobe"),
                "edge-cache" => bases.home.join("Library/Caches/Microsoft Edge"),
                "uv-cache" => bases.local_data.join("uv"),
                "trivy-cache" => bases.home.join("Library/Caches/trivy"),
                _ => unreachable!(),
            };
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("fixture.bin"), b"regenerable").unwrap();
        }
        let results = clean_regenerable_caches_inner(&bases, &tmp.path().join("journal.jsonl"), 7);
        assert_eq!(results.len(), 6);
        assert!(results.iter().all(|result| result.ok));
    }

    #[test]
    fn dev_artifact_cleanup_rejects_a_stale_metadata_fingerprint() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("webapp");
        let artifact = project.join("node_modules");
        fs::create_dir_all(&artifact).unwrap();
        fs::write(project.join("package.json"), b"{}").unwrap();
        fs::write(artifact.join("payload.bin"), b"old").unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let observed = crate::dev_artifacts::find_artifacts(tmp.path(), 0, now);
        assert_eq!(observed.len(), 1);
        fs::write(artifact.join("payload.bin"), b"recreated-with-different-size").unwrap();
        let results = clean_dev_artifacts_inner(
            &observed,
            tmp.path(),
            0,
            &tmp.path().join("journal.jsonl"),
            now,
        );
        assert_eq!(results.len(), 1);
        assert!(!results[0].ok);
        assert!(results[0].error.contains("다시 스캔"));
        assert!(artifact.join("payload.bin").exists());
    }

    #[test]
    fn execute_moves_inner_reports_per_item_and_isolates_failures() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let src_ok = tmp.path().join("a.bin");
        std::fs::write(&src_ok, vec![1u8; 16]).unwrap();
        let dst_ok = tmp.path().join("sub").join("a.bin");
        let plans = vec![
            organize::MovePlan {
                src: src_ok.to_string_lossy().into(),
                dst: dst_ok.to_string_lossy().into(),
                class_id: "x".into(),
                ..Default::default()
            },
            organize::MovePlan {
                src: tmp.path().join("ghost").to_string_lossy().into(),
                dst: tmp.path().join("g2").to_string_lossy().into(),
                class_id: "x".into(),
                ..Default::default()
            },
        ];
        let results = execute_moves_inner(&plans, &jp, 1);
        assert_eq!(results.len(), 2);
        assert!(results[0].ok);
        assert!(!results[1].ok);
        assert!(!src_ok.exists());
        assert!(dst_ok.exists());
    }

    #[test]
    fn undo_last_moves_inner_reverses_recent_moves_newest_first() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let a = tmp.path().join("a.bin");
        std::fs::write(&a, vec![2u8; 8]).unwrap();
        let a_moved = tmp.path().join("dest").join("a.bin");
        let plans = vec![organize::MovePlan {
            src: a.to_string_lossy().into(),
            dst: a_moved.to_string_lossy().into(),
            class_id: "x".into(),
            ..Default::default()
        }];
        execute_moves_inner(&plans, &jp, 5);
        assert!(!a.exists());
        assert!(a_moved.exists());
        let undone = undo_last_moves_inner(10, &jp, 6);
        assert_eq!(undone.len(), 1);
        assert!(undone[0].ok);
        assert!(a.exists());
        assert!(!a_moved.exists());
    }

    #[test]
    fn undo_last_moves_inner_respects_limit_after_filtering() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        for name in ["x.bin", "y.bin"] {
            let s = tmp.path().join(name);
            std::fs::write(&s, b"z").unwrap();
            let d = tmp.path().join("d").join(name);
            execute_moves_inner(
                &[organize::MovePlan {
                    src: s.to_string_lossy().into(),
                    dst: d.to_string_lossy().into(),
                    class_id: "x".into(),
                    ..Default::default()
                }],
                &jp,
                1,
            );
        }
        let undone = undo_last_moves_inner(1, &jp, 9);
        assert_eq!(undone.len(), 1);
    }

    #[test]
    fn undo_last_moves_inner_reports_failure_when_original_path_reoccupied() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let a = tmp.path().join("a.bin");
        std::fs::write(&a, vec![3u8; 4]).unwrap();
        let a_moved = tmp.path().join("dest").join("a.bin");
        let plans = vec![organize::MovePlan {
            src: a.to_string_lossy().into(),
            dst: a_moved.to_string_lossy().into(),
            class_id: "x".into(),
            ..Default::default()
        }];
        execute_moves_inner(&plans, &jp, 1);
        assert!(a_moved.exists());
        std::fs::write(&a, b"blocker").unwrap();
        let undone = undo_last_moves_inner(1, &jp, 2);
        assert_eq!(undone.len(), 1);
        assert!(!undone[0].ok);
        assert!(a_moved.exists());
    }
}
