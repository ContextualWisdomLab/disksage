use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
#[cfg(not(coverage))]
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

#[cfg(not(coverage))]
use tauri::{AppHandle, Emitter, State};

#[cfg(not(coverage))]
use crate::scanner;
use crate::scanner::ScanResult;

// clean_paths_inner/execute_moves_inner/undo_last_moves_inner(순수 함수)가 쓰는 것은 무조건 import; 래퍼 전용은 cfg(not(coverage))
use crate::organize;
use crate::safety;
#[cfg(not(coverage))]
use crate::{
    cloud, cloud_review, cloud_transfer, dev_artifacts, dupes, provider_api_client,
    provider_capacity, provider_evidence, provider_oauth, provider_sync, rules,
};

#[derive(Default)]
pub struct AppState {
    pub result: Arc<Mutex<Option<ScanResult>>>,
    pub cancel: Arc<AtomicBool>,
    pub scanning: Arc<AtomicBool>,
    /// Serialize review writes with review-gated copies so a later hold cannot race a copy.
    pub cloud_review: Arc<Mutex<()>>,
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
    if path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
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
pub fn clean_paths_inner(
    paths: &[PathBuf],
    journal_path: &Path,
    now_ms: u64,
) -> Vec<CleanResult> {
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

/// 저널의 move 경로 필드 "src -> dst"를 분리 (순수 함수 — 테스트 대상). 구분자 없으면 None.
pub fn parse_move_entry(path_field: &str) -> Option<(String, String)> {
    path_field
        .split_once(" -> ")
        .map(|(s, d)| (s.to_string(), d.to_string()))
}

/// MovePlan을 safety::move_file로 실행하는 순수 코어 — 항목별 결과, 하나 실패해도 나머지는 진행 (M2와 동일 원칙)
pub fn execute_moves_inner(plans: &[organize::MovePlan], journal_path: &Path, now_ms: u64) -> Vec<CleanResult> {
    plans
        .iter()
        .map(|p| match safety::move_file(Path::new(&p.src), Path::new(&p.dst), journal_path, now_ms) {
            Ok(()) => CleanResult { path: p.src.clone(), ok: true, error: String::new() },
            Err(e) => CleanResult { path: p.src.clone(), ok: false, error: e.to_string() },
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
        .map(|(src, dst)| match safety::move_file(Path::new(&dst), Path::new(&src), journal_path, now_ms) {
            Ok(()) => CleanResult { path: src, ok: true, error: String::new() },
            Err(e) => CleanResult { path: src, ok: false, error: e.to_string() },
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
        if let Ok(s) = std::fs::read_to_string(dir.join("userrules.json")) { return s; }
    }
    "[]".to_string()
}

#[cfg(not(coverage))]
fn bundled_ontology_ttl(app: &AppHandle) -> Result<String, String> {
    use tauri::Manager;
    // 사용자 설정 디렉토리 오버라이드 우선, 없으면 번들 리소스.
    // 오버라이드 파일이 없으면(read 실패) 조용히 번들로 폴백하지만, 파일이 있어도
    // parse가 실패하면(malformed) 상위 load_ontology_from이 에러를 낸다 — 의도적:
    // 사용자가 편집한 잘못된 온톨로지를 조용히 무시하지 않고 알린다.
    if let Ok(dir) = app.path().app_config_dir() {
        let user_ttl = dir.join("ontology.ttl");
        if let Ok(s) = std::fs::read_to_string(&user_ttl) {
            return Ok(s);
        }
    }
    let res = app
        .path()
        .resolve("resources/ontology/default.ttl", tauri::path::BaseDirectory::Resource)
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
pub fn disk_inventory(root: String, app: AppHandle) -> Result<crate::inventory::InventoryReport, String> {
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

/// 현재 설정 조회. 파일 없으면 기본값(offline). 손상 파일은 parse_settings가 기본값으로 흡수.
#[cfg(not(coverage))]
#[tauri::command]
pub fn get_settings(app: AppHandle) -> Result<crate::settings::Settings, String> {
    let path = settings_file_path(&app)?;
    match std::fs::read_to_string(&path) {
        Ok(s) => Ok(crate::settings::parse_settings(&s)),
        Err(_) => Ok(crate::settings::Settings::default()),
    }
}

/// online_mode 설정 후 영속. 반환은 저장된 설정.
#[cfg(not(coverage))]
#[tauri::command]
pub fn set_settings(online_mode: bool, app: AppHandle) -> Result<crate::settings::Settings, String> {
    let s = crate::settings::Settings { online_mode };
    let path = settings_file_path(&app)?;
    std::fs::write(&path, crate::settings::serialize_settings(&s)).map_err(|e| e.to_string())?;
    Ok(s)
}

// 아래 Tauri command 래퍼들은 coverage 빌드에서 제외 — 순수 로직(node_view 등)은 위에서 측정됨
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
        // 패닉으로 스레드가 죽어도 scanning 플래그는 반드시 해제
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
        *slot.lock().unwrap() = Some(res); // done 이벤트 전에 저장 (레이스 방지)
        drop(_reset); // emit 전에 scanning 플래그 해제 (원래 순서 복원, 패닉 안전성은 Drop이 유지)
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
pub fn get_node(path: String, state: State<AppState>) -> Result<NodeView, String> {
    // ponytail: lock held across read_dir I/O; snapshot dir_sizes and read outside the lock if this stalls on huge/network dirs
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
fn journal_file_path(app: &AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("journal.jsonl"))
}

#[cfg(not(coverage))]
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn list_cache_candidates() -> Result<Vec<rules::CacheCandidate>, String> {
    let bases = rules::BaseDirs::from_env().ok_or("환경변수에서 기본 경로를 찾지 못함")?;
    Ok(rules::cache_candidates(&bases))
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn list_dev_artifacts(
    root: String,
    min_age_days: u64,
) -> Result<Vec<dev_artifacts::DevArtifact>, String> {
    Ok(dev_artifacts::find_artifacts(Path::new(&root), min_age_days, now_ms()))
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
pub fn recent_operations(limit: usize, app: AppHandle) -> Result<Vec<safety::JournalEntry>, String> {
    Ok(safety::journal_recent(&journal_file_path(&app)?, limit))
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn expand_clean_targets(dir: String) -> Vec<String> {
    // 카탈로그 경로로만 스코프 — 임의 디렉토리 열람 IPC가 되지 않도록
    let Some(bases) = rules::BaseDirs::from_env() else { return Vec::new() };
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

/// home 해석: app.path().home_dir() 우선, 실패 시 HOME/USERPROFILE 환경변수 폴백.
#[cfg(not(coverage))]
fn resolve_home(app: &AppHandle) -> PathBuf {
    use tauri::Manager;
    app.path()
        .home_dir()
        .ok()
        .or_else(|| std::env::var("HOME").ok().map(PathBuf::from))
        .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Candidate local roots exposed by iCloud Drive, OneDrive, and Google Drive, including their
/// discovery-time readability evidence.
#[cfg(not(coverage))]
#[tauri::command]
pub fn list_cloud_roots(app: AppHandle) -> Vec<cloud::CloudRoot> {
    cloud::discover_cloud_roots(&resolve_home(&app))
}

/// Return selectable roots together with bounded provider/account discovery failures. This does
/// not create a probe file, hydrate a placeholder, or contact a provider API.
#[cfg(not(coverage))]
#[tauri::command]
pub fn inspect_cloud_roots(app: AppHandle) -> cloud::CloudRootDiscoveryReport {
    cloud::discover_cloud_roots_report(&resolve_home(&app))
}

#[cfg(not(coverage))]
fn selected_cloud_root(app: &AppHandle, cloud_root: &str) -> Result<cloud::CloudRoot, String> {
    let matches: Vec<_> = cloud::discover_cloud_roots(&resolve_home(app))
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

/// Return non-secret OAuth connection descriptors. Refresh tokens remain in the OS credential
/// store and this command never reads or returns them.
#[cfg(not(coverage))]
#[tauri::command]
pub fn list_cloud_provider_connections(
    app: AppHandle,
) -> Result<Vec<provider_oauth::OAuthConnection>, String> {
    provider_oauth::load_connections(&oauth_connections_path(&app)?)
}

/// Return only the latest non-secret approve/hold decision for each candidate fingerprint.
#[cfg(not(coverage))]
#[tauri::command]
pub fn list_cloud_review_decisions(
    app: AppHandle,
) -> Result<Vec<cloud_review::CloudReviewDecision>, String> {
    cloud_review::load_latest_decisions(&cloud_review_directory(&app)?)
}

/// Start a native browser authorization-code flow with PKCE and a random loopback port. The
/// provider refresh token is committed to the OS credential store only after state validation and
/// a successful token exchange. Client IDs are public desktop-app identifiers, not secrets.
#[cfg(not(coverage))]
#[tauri::command(async)]
pub async fn connect_cloud_provider(
    cloud_root: String,
    client_id: String,
    app: AppHandle,
) -> Result<provider_oauth::OAuthConnection, String> {
    let selected = selected_cloud_root(&app, &cloud_root)?;
    cloud::validate_cloud_root_readable(&selected)?;
    if selected.provider == cloud::CloudProvider::Icloud {
        return Err("icloud-oauth-not-supported".into());
    }
    let pending = provider_oauth::prepare_authorization(selected.provider, &client_id)?;
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(pending.authorization_url(), None::<&str>)
        .map_err(|_| "oauth-system-browser-open-failed".to_string())?;
    let connection_path = oauth_connections_path(&app)?;
    let connected_at_ms = cloud::system_now_ms();
    tauri::async_runtime::spawn_blocking(move || {
        provider_oauth::finish_authorization(
            pending,
            &selected,
            &connection_path,
            connected_at_ms,
        )
    })
    .await
    .map_err(|_| "provider-oauth-task-failed".to_string())?
}

/// Remove the selected root's refresh token from the OS credential store and its non-secret local
/// connection descriptor. This does not alter any cloud file.
#[cfg(not(coverage))]
#[tauri::command(async)]
pub async fn disconnect_cloud_provider(
    cloud_root: String,
    app: AppHandle,
) -> Result<(), String> {
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

/// Revalidate a saved provider connection after launch without exposing access or refresh tokens.
///
/// This is deliberately opt-in because it reads the OS credential store and contacts the fixed
/// provider capacity endpoint. Failures are returned as redacted, stable capacity evidence rather
/// than raw OAuth or transport details.
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
fn cloud_plan_for_inputs(
    root: &str,
    cloud_root: &str,
    min_size_mib: u64,
    min_age_days: u64,
    limit: usize,
    app: &AppHandle,
) -> Result<(cloud::CloudRoot, cloud::CloudPlanReport), String> {
    let root_path = PathBuf::from(root);
    cloud::validate_source_root_readable(&root_path)?;
    let discovered = cloud::discover_cloud_roots(&resolve_home(app));
    let selected = discovered
        .iter()
        .find(|candidate| candidate.path == cloud_root)
        .cloned()
        .ok_or_else(|| "탐지된 클라우드 루트가 아님".to_string())?;
    cloud::validate_cloud_root_readable(&selected)?;
    let excluded: Vec<PathBuf> = discovered.iter().map(|root| PathBuf::from(&root.path)).collect();
    if excluded.iter().any(|cloud| root_path.starts_with(cloud)) {
        return Err("이미 클라우드 안에 있는 경로는 오프로드 원본으로 사용할 수 없음".into());
    }
    let files = cloud::collect_archive_files(&root_path, &excluded);
    let report = cloud::plan_cloud_archive(
        &files,
        &root_path,
        &selected,
        cloud::system_now_ms(),
        cloud::CloudPlanOptions {
            min_size_bytes: min_size_mib.saturating_mul(1024 * 1024),
            min_age_days,
            limit: limit.clamp(1, 1_000),
        },
    );
    Ok((selected, report))
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
    selected: &cloud::CloudRoot,
    app: &AppHandle,
) {
    let observed_at_ms = cloud::system_now_ms();
    let snapshot = match authenticated_capacity_snapshot(selected, app, observed_at_ms) {
        Ok(snapshot) => snapshot,
        Err(error) => provider_capacity::unavailable_capacity_from_error(
            selected.provider,
            observed_at_ms,
            &error,
        ),
    };
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
    report.notices.push(match assessment.can_fit {
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
    .into());
    report.capacity = Some(assessment);
}

#[cfg(not(coverage))]
fn require_capacity_for_copy(
    selected: &cloud::CloudRoot,
    candidate: &cloud::CloudCandidate,
    app: &AppHandle,
) -> Result<(), String> {
    let snapshot = authenticated_capacity_snapshot(selected, app, cloud::system_now_ms())?;
    let assessment = provider_capacity::assess_capacity(
        snapshot,
        candidate.bytes,
        candidate.bytes,
        provider_capacity::DEFAULT_CAPACITY_RESERVE_BYTES,
    );
    if assessment.can_fit == Some(true) {
        Ok(())
    } else {
        Err(if assessment.blockers.is_empty() {
            "cloud-capacity-verification-required".into()
        } else {
            assessment.blockers.join(",")
        })
    }
}

/// Read-only cloud offload plan. The selected destination must be one of the roots discovered
/// on this machine; this command never creates a folder or moves a file.
#[cfg(not(coverage))]
#[tauri::command(async)]
pub async fn plan_cloud_archive(
    root: String,
    cloud_root: String,
    min_size_mib: u64,
    min_age_days: u64,
    limit: usize,
    app: AppHandle,
) -> Result<cloud::CloudPlanReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (selected, mut report) =
            cloud_plan_for_inputs(&root, &cloud_root, min_size_mib, min_age_days, limit, &app)?;
        attach_capacity_assessment(&mut report, &selected, &app);
        Ok(report)
    })
    .await
    .map_err(|_| "cloud-plan-task-failed".to_string())?
}

/// Rebuild the plan and append an immutable approve/hold decision for the exact evidence shown by
/// the UI. A stale UI cannot approve a changed metadata snapshot.
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
        if bounded.is_empty() { "unknown" } else { &bounded }
    )
}

#[cfg(not(coverage))]
#[tauri::command(async)]
pub fn review_cloud_candidate(
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
    state: State<AppState>,
) -> Result<cloud_review::CloudReviewDecision, String> {
    for fingerprint in [&metadata_fingerprint, &review_fingerprint] {
        if fingerprint.len() != 64
            || !fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("cloud-review-fingerprint-invalid".into());
        }
    }
    let _guard = state
        .cloud_review
        .lock()
        .map_err(|_| "cloud-review-lock-poisoned".to_string())?;
    let (_, report) = cloud_plan_for_inputs(
        &root,
        &cloud_root,
        min_size_mib,
        min_age_days,
        limit,
        &app,
    )?;
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
}

#[cfg(not(coverage))]
#[derive(serde::Serialize)]
pub struct CloudCopyOutput {
    pub action: &'static str,
    pub receipt: cloud_transfer::CloudCopyReceipt,
    pub receipt_path: String,
}

#[cfg(not(coverage))]
fn create_cloud_candidate_receipt(
    root: &str,
    cloud_root: &str,
    metadata_fingerprint: &str,
    min_size_mib: u64,
    min_age_days: u64,
    limit: usize,
    app: &AppHandle,
    adopt_existing: bool,
) -> Result<CloudCopyOutput, String> {
    if metadata_fingerprint.len() != 64
        || !metadata_fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("metadata-fingerprint-invalid".into());
    }
    let (selected, report) = cloud_plan_for_inputs(
        root,
        cloud_root,
        min_size_mib,
        min_age_days,
        limit,
        app,
    )?;
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
    use tauri::Manager;
    let receipt_dir = app
        .path()
        .app_data_dir()
        .map_err(|_| "app-data-directory-unavailable".to_string())?
        .join("cloud-receipts");
    let review_decision = if candidate.requires_review {
        cloud_review::load_latest_decisions(&cloud_review_directory(&app)?)?
            .into_iter()
            .find(|decision| decision.candidate_fingerprint == candidate.metadata_fingerprint)
    } else {
        None
    };
    if !adopt_existing {
        require_capacity_for_copy(&selected, candidate, app)?;
    }
    let (receipt, receipt_path) = if adopt_existing {
        cloud_transfer::adopt_existing_cloud_copy_with_review(
            candidate,
            &selected,
            &receipt_dir,
            cloud::system_now_ms(),
            review_decision.as_ref(),
        )?
    } else {
        cloud_transfer::prepare_cloud_copy_with_review(
            candidate,
            &selected,
            &receipt_dir,
            cloud::system_now_ms(),
            review_decision.as_ref(),
        )?
    };
    Ok(CloudCopyOutput {
        action: if adopt_existing {
            "adopt-existing-copy"
        } else {
            "copy-only"
        },
        receipt,
        receipt_path: receipt_path.to_string_lossy().into_owned(),
    })
}

/// Rebuild the plan from current metadata, then copy one uniquely matching safe candidate.
/// The source is retained and no local-eviction API is exposed by this command.
#[cfg(not(coverage))]
#[tauri::command(async)]
pub fn copy_cloud_candidate(
    root: String,
    cloud_root: String,
    metadata_fingerprint: String,
    min_size_mib: u64,
    min_age_days: u64,
    limit: usize,
    app: AppHandle,
    state: State<AppState>,
) -> Result<CloudCopyOutput, String> {
    let _guard = state
        .cloud_review
        .lock()
        .map_err(|_| "cloud-review-lock-poisoned".to_string())?;
    create_cloud_candidate_receipt(
        &root,
        &cloud_root,
        &metadata_fingerprint,
        min_size_mib,
        min_age_days,
        limit,
        &app,
        false,
    )
}

/// Rebuild the plan and adopt an already-existing destination only after full content-digest
/// equality is proven. Both source and destination remain in place.
#[cfg(not(coverage))]
#[tauri::command(async)]
pub fn adopt_existing_cloud_candidate(
    root: String,
    cloud_root: String,
    metadata_fingerprint: String,
    min_size_mib: u64,
    min_age_days: u64,
    limit: usize,
    app: AppHandle,
    state: State<AppState>,
) -> Result<CloudCopyOutput, String> {
    let _guard = state
        .cloud_review
        .lock()
        .map_err(|_| "cloud-review-lock-poisoned".to_string())?;
    create_cloud_candidate_receipt(
        &root,
        &cloud_root,
        &metadata_fingerprint,
        min_size_mib,
        min_age_days,
        limit,
        &app,
        true,
    )
}

#[cfg(not(coverage))]
#[derive(serde::Serialize)]
pub struct CloudAttestationOutput {
    pub evidence: cloud_transfer::ProviderSyncEvidence,
    pub assessment: provider_sync::ProviderSyncTimelinessAssessment,
    pub evidence_record: provider_evidence::ProviderSyncEvidenceRecord,
    pub evidence_path: String,
    pub permit: Option<cloud_transfer::LocalEvictionPermit>,
    pub blockers: Vec<String>,
}

/// Read-only provider attestation. OneDrive and Google Drive access tokens are refreshed from an OS
/// credential-store token, used once in memory, and never accepted from or returned to the UI.
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
    let connection_path = oauth_connections_path(&app)?;
    let cloud_roots = cloud::discover_cloud_roots(&resolve_home(&app));
    tauri::async_runtime::spawn_blocking(move || {
        let receipt = cloud_transfer::read_immutable_receipt(&receipt_path)?;
        if receipt.receipt_id != receipt_id {
            return Err("receipt-id-mismatch".into());
        }
        let confirmed_at_ms = cloud::system_now_ms();
        let evidence = match receipt.provider {
            cloud::CloudProvider::Icloud => {
                if object_id
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
                {
                    return Err("icloud-provider-object-id-not-accepted".into());
                }
                provider_sync::collect_icloud_sync_evidence(&receipt, confirmed_at_ms)?
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
                let object_id = object_id.filter(|value| !value.trim().is_empty());
                let fallback_requested = receipt.provider == cloud::CloudProvider::Onedrive
                    || object_id.is_some();
                match provider_sync::collect_file_provider_sync_evidence(&receipt, confirmed_at_ms)
                {
                    Ok(evidence) if evidence.sync_complete || !fallback_requested => evidence,
                    Err(error) if !fallback_requested => return Err(error),
                    Ok(_) | Err(_) => {
                        let access_token = provider_oauth::refreshed_access_token(
                            &connection_path,
                            &selected_root,
                        )?;
                        let client =
                            provider_api_client::FixedHostProviderMetadataClient::default();
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
                                    &receipt,
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
                                    &receipt,
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
        let assessment = provider_sync::assess_provider_sync_timeliness(&receipt, &evidence)?;
        let (evidence_record, evidence_path) =
            provider_evidence::write_immutable_sync_evidence(&evidence_dir, &evidence)?;
        let (permit, blockers) =
            match cloud_transfer::approve_local_eviction(&receipt, &evidence_record) {
                Ok(permit) => (Some(permit), Vec::new()),
                Err(blockers) => (None, blockers),
            };
        Ok(CloudAttestationOutput {
            evidence,
            assessment,
            evidence_record,
            evidence_path: evidence_path.to_string_lossy().into_owned(),
            permit,
            blockers,
        })
    })
    .await
    .map_err(|_| "cloud-attestation-task-failed".to_string())?
}

#[cfg(not(coverage))]
#[cfg_attr(not(feature = "llm-engine"), allow(unused_variables))]
#[tauri::command(async)]
pub fn plan_organize(root: String, app: AppHandle, state: State<AppState>) -> Result<Vec<organize::MovePlan>, String> {
    let onto = load_ontology_from(&bundled_ontology_ttl(&app)?)?;
    let rules = crate::userrules::parse_rules(&user_rules_json(&app))?; // malformed → Err surfaced
    let files = dupes::collect_files(Path::new(&root));
    let home = resolve_home(&app);
    // classify_prompt는 name/parent만 쓰므로 picker는 size 불필요(0으로 구성).
    // ponytail: LLM picker는 파일마다 추론 1회 — 대규모 스캔 프리뷰에선 느릴 수 있음.
    //           지금은 모델 있으면 전부 LLM 분류; 필요 시 후속에서 미분류 항목만으로 제한.
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
                let pick = |p: &Path, cands: &[&str]| {
                    let meta = file_meta_at(p, 0, 0);
                    crate::llm::pick_class(engine, &meta, cands)
                };
                return Ok(organize::plan_moves_with(&files, &onto, &home, now_ms(), &rules, &pick));
            }
        }
    }
    Ok(organize::plan_moves_with(&files, &onto, &home, now_ms(), &rules, &|_, _| None))
}

/// 활성 사용자 규칙 조회(UI 표시용). 손상 파일은 Err.
#[cfg(not(coverage))]
#[tauri::command]
pub fn user_rules(app: AppHandle) -> Result<Vec<crate::userrules::Rule>, String> {
    crate::userrules::parse_rules(&user_rules_json(&app))
}

/// MovePlan을 safety::move_file로 실행 — 항목별 결과, 하나 실패해도 나머지는 진행 (M2와 동일 원칙)
#[cfg(not(coverage))]
#[tauri::command(async)]
pub fn execute_moves(plans: Vec<organize::MovePlan>, app: AppHandle) -> Result<Vec<CleanResult>, String> {
    let jp = journal_file_path(&app)?;
    Ok(execute_moves_inner(&plans, &jp, now_ms()))
}

/// 최근 저널에서 op=="move"·outcome=="ok" 항목을 찾아 역이동(dst→src)한다.
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

/// 모델 파일 경로: <app_data>/models/<DEFAULT.name>.gguf
pub fn model_file_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("models").join(format!("{}.gguf", crate::llm::DEFAULT.name))
}

/// 모델 존재 여부 + 이름. 없으면 앱은 규칙 기반으로 동작(배지 미판정).
pub fn model_status_for(model_path: &Path) -> ModelStatus {
    ModelStatus { present: model_path.exists(), name: crate::llm::DEFAULT.name.to_string() }
}

/// 경로 + (이미 읽은) size·age로 FileMeta 구성. name/parent는 경로에서, 없으면 빈 문자열(패닉 없음).
pub fn file_meta_at(path: &Path, size: u64, mtime_days: u64) -> crate::llm::FileMeta {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let parent = path
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    crate::llm::FileMeta { path: path.to_string_lossy().into_owned(), name, size, mtime_days, parent }
}

/// 항목마다 캐시(path|size|mtime_ms) 확인 후 미스면 추론. 판정만 캐시(이유는 미스 시에만).
pub fn verdicts_with(
    engine: &dyn crate::llm::InferenceEngine,
    cache: &mut crate::llm::VerdictCache,
    items: &[(crate::llm::FileMeta, u64)],
) -> Vec<crate::llm::FileVerdict> {
    let mut out = Vec::with_capacity(items.len());
    for (meta, mtime_ms) in items {
        let key = crate::llm::VerdictCache::key(&meta.path, meta.size, *mtime_ms);
        if let Some(v) = cache.get(&key) {
            out.push(crate::llm::FileVerdict { path: meta.path.clone(), verdict: v, reason: String::new() });
        } else {
            let fv = crate::llm::verdict_for(engine, meta);
            cache.put(key, fv.verdict);
            out.push(fv);
        }
    }
    out
}

// --- M5: 모델 상태/다운로드, 캐시된 파일 판정, 미분류 뭉치 요약 IPC ---
// 순수 로직(model_file_path/model_status_for/file_meta_at/verdicts_with)은 위(게이트 측정 대상)에 있음.
// 아래는 io/엔진 수명주기를 다루는 얇은 래퍼 — coverage에서 제외.

#[cfg(not(coverage))]
fn meta_items(paths: &[String]) -> Vec<(crate::llm::FileMeta, u64)> {
    paths.iter().filter_map(|p| {
        let path = std::path::Path::new(p);
        let md = std::fs::metadata(path).ok()?;
        let mtime_ms = md.modified().ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let age_days = now_ms().saturating_sub(mtime_ms) / 86_400_000; // 실제 파일 나이(프롬프트용); 캐시 키는 원시 mtime_ms 사용
        Some((file_meta_at(path, md.len(), age_days), mtime_ms))
    }).collect()
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

/// 캐시된 파일 판정 — 엔진 있으면 실제 추론(세션 캐시 활용), 없으면(feature off/모델 없음/엔진 초기화 실패) 전부 Unrated로 완만히 저하.
#[cfg(not(coverage))]
#[cfg_attr(not(feature = "llm-engine"), allow(unused_variables))]
#[tauri::command(async)]
pub fn file_verdicts(paths: Vec<String>, app: AppHandle, state: State<AppState>) -> Result<Vec<crate::llm::FileVerdict>, String> {
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

/// 미분류 뭉치 한 줄 요약 — 엔진 없으면 None(스펙 §6 graceful degradation).
#[cfg(not(coverage))]
#[cfg_attr(not(feature = "llm-engine"), allow(unused_variables))]
#[tauri::command(async)]
pub fn summarize_unknown_bucket(paths: Vec<String>, app: AppHandle, state: State<AppState>) -> Result<Option<String>, String> {
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

/// 미분류 확장자 자문 추론. samples = InventoryReport.unknown_samples(경로). online_mode일 때만 웹 조회.
/// LLM은 feature+모델 있을 때만; 웹은 online_mode일 때만(feature 무관). 둘 다 없으면 source="none".
#[cfg(not(coverage))]
#[cfg_attr(not(feature = "llm-engine"), allow(unused_variables))]
#[tauri::command(async)]
pub fn reason_unknown_extensions(
    samples: Vec<String>,
    app: AppHandle,
    state: State<AppState>,
) -> Result<Vec<crate::reasoning::ExtInsight>, String> {
    let exts = crate::reasoning::distinct_extensions(&samples);

    // opt-in 웹: online_mode일 때만 DdgLookup, 아니면 None → build_insights의 웹 분기 절대 미실행(default offline)
    let settings = get_settings(app.clone())?;
    let ddg = crate::web::DdgLookup;
    let web_fn = |ext: &str| -> Option<String> { crate::web::WebLookup::file_type(&ddg, ext).ok().flatten() };
    let web: Option<&dyn Fn(&str) -> Option<String>> = if settings.online_mode { Some(&web_fn) } else { None };

    // 오프라인 LLM(feature+모델+엔진 있으면 실제; 그 블록에서 반환). 없으면 아래 fallback로 낙하.
    #[cfg(feature = "llm-engine")]
    {
        use tauri::Manager;
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        if model_status_for(&model_file_path(&dir)).present {
            // 온톨로지 로드는 LLM 경로에서만 필요 — 여기로 이동해 기본/웹전용 빌드가 malformed ontology.ttl로 실패하지 않게 함
            let onto = load_ontology_from(&bundled_ontology_ttl(&app)?)?;
            let candidates: Vec<String> = onto.classes.iter()
                .map(|c| c.id.rsplit(['#', '/']).next().unwrap_or(&c.id).to_string()).collect();
            let cand_refs: Vec<&str> = candidates.iter().map(|s| s.as_str()).collect();

            let mut guard = state.engine.lock().unwrap();
            if guard.is_none() {
                if let Ok(e) = crate::llm::LlamaEngine::new(&model_file_path(&dir)) {
                    *guard = Some(e);
                }
            }
            if let Some(engine) = guard.as_ref() {
                let reason = |ext: &str| crate::llm::reason_extension(engine, ext, &cand_refs);
                // ponytail: engine lock held across the opt-in web lookups in build_insights (≤5s×N). Fine for the few distinct unknown exts; if a concurrent verdict call ever contends, split into a locked LLM pass + an unlocked web pass.
                return Ok(crate::reasoning::build_insights(&exts, &reason, web));
            }
        }
    }

    // fallback: LLM 없음(feature off/모델 없음/init 실패) — reason은 항상 None, 웹은 위 settings대로 적용
    let reason = |_: &str| -> Option<crate::llm::ExtReasoning> { None };
    Ok(crate::reasoning::build_insights(&exts, &reason, web))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::scan_dir_with_interval;
    use std::fs;
    use std::sync::atomic::AtomicBool;

    // --- M5 LLM 커맨드 순수 헬퍼 ---
    use crate::llm::{InferenceEngine, Verdict, VerdictCache};

    struct CountingFake { out: String, calls: std::cell::Cell<usize> }
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
    fn file_meta_at_extracts_name_and_parent() {
        let m = file_meta_at(std::path::Path::new("/downloads/report.pdf"), 42, 7);
        assert_eq!(m.name, "report.pdf");
        assert_eq!(m.parent, "downloads");
        assert_eq!(m.size, 42);
        assert_eq!(m.mtime_days, 7);
        // 파일명/부모 없는 경로 → 빈 문자열(패닉 없음)
        let root = file_meta_at(std::path::Path::new("/"), 0, 0);
        assert_eq!(root.name, "");
        assert_eq!(root.parent, "");
    }

    #[test]
    fn verdicts_with_caches_and_avoids_reinference() {
        let engine = CountingFake { out: r#"{"verdict":"safe","reason":"r"}"#.into(), calls: std::cell::Cell::new(0) };
        let mut cache = VerdictCache::new();
        let meta = file_meta_at(std::path::Path::new("/x/a.bin"), 100, 1);
        let items = vec![(meta.clone(), 1700u64), (meta, 1700u64)]; // 같은 path|size|mtime → 두 번째는 캐시 히트
        let out = verdicts_with(&engine, &mut cache, &items);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|fv| fv.verdict == Verdict::Safe));
        assert_eq!(engine.calls.get(), 1, "두 번째 항목은 캐시 히트라 추론 1회만");
    }

    #[test]
    fn verdicts_with_distinct_items_infer_each() {
        let engine = CountingFake { out: r#"{"verdict":"keep"}"#.into(), calls: std::cell::Cell::new(0) };
        let mut cache = VerdictCache::new();
        let a = (file_meta_at(std::path::Path::new("/x/a"), 1, 1), 10u64);
        let b = (file_meta_at(std::path::Path::new("/x/b"), 2, 2), 20u64);
        let out = verdicts_with(&engine, &mut cache, &[a, b]);
        assert_eq!(out.len(), 2);
        assert_eq!(engine.calls.get(), 2);
        let _ = out; // FileVerdict used
    }

    // 간격 1로 스캔 — 진행 콜백(클로저)도 매 엔트리마다 실행돼 커버리지에 0으로 남지 않는다
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
        // lexical starts_with는 통과하지만 OS 해석은 루트 밖(실존 디렉토리)인 경로 — 가드 없으면 Ok
        let sneaky = tmp.path().join("..");
        assert!(node_view(&res, &sneaky).is_err());
    }

    #[test]
    fn node_view_rejects_sibling_path_outside_root() {
        // '..' 없이 루트 밖인 경로 — 두 번째 가드(starts_with)를 직접 태운다
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
        // 단일 파일 대상 — bytes 분기의 metadata().map(|m| m.len()) 성공 경로를 태운다
        // (missing은 metadata 실패만 태우고 성공은 태우지 않는다)
        let ok_file = tmp.path().join("disksage-clean-fixture-file.bin");
        fs::write(&ok_file, vec![0u8; 16]).unwrap();
        let missing = tmp.path().join("ghost");
        let protected = std::path::PathBuf::from(if cfg!(windows) { "C:\\Windows" } else { "/usr" });

        let results = clean_paths_inner(&[ok_dir.clone(), ok_file.clone(), missing, protected], &jp, 7);

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
        assert_eq!(ok_entry.bytes, 32, "디렉토리는 재귀 크기로 저널링");
        let ok_file_entry = recent
            .iter()
            .find(|e| e.outcome == "ok" && e.path.contains("disksage-clean-fixture-file"))
            .unwrap();
        assert_eq!(ok_file_entry.bytes, 16, "단일 파일은 metadata 크기로 저널링");

        // 테스트 픽스처 휴지통 정리 (win/linux)
        #[cfg(any(windows, target_os = "linux"))]
        {
            let items: Vec<_> = trash::os_limited::list()
                .unwrap()
                .into_iter()
                .filter(|i| {
                    let n = i.name.to_string_lossy();
                    n.contains("disksage-clean-fixture-dir") || n.contains("disksage-clean-fixture-file")
                })
                .collect();
            trash::os_limited::purge_all(items).unwrap();
        }
    }

    #[test]
    fn execute_moves_inner_reports_per_item_and_isolates_failures() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let src_ok = tmp.path().join("a.bin");
        std::fs::write(&src_ok, vec![1u8; 16]).unwrap();
        let dst_ok = tmp.path().join("sub").join("a.bin");
        // 하나는 성공(같은 볼륨 rename), 하나는 실패(존재하지 않는 src)
        let plans = vec![
            organize::MovePlan { src: src_ok.to_string_lossy().into(), dst: dst_ok.to_string_lossy().into(), class_id: "x".into() },
            organize::MovePlan { src: tmp.path().join("ghost").to_string_lossy().into(), dst: tmp.path().join("g2").to_string_lossy().into(), class_id: "x".into() },
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
        // 먼저 이동 실행(저널에 move/ok 기록)
        let plans = vec![organize::MovePlan { src: a.to_string_lossy().into(), dst: a_moved.to_string_lossy().into(), class_id: "x".into() }];
        execute_moves_inner(&plans, &jp, 5);
        assert!(!a.exists());
        assert!(a_moved.exists());
        // 되돌리기 → 원위치 복원
        let undone = undo_last_moves_inner(10, &jp, 6);
        assert_eq!(undone.len(), 1);
        assert!(undone[0].ok);
        assert!(a.exists(), "되돌리기로 원위치 복원");
        assert!(!a_moved.exists());
    }

    #[test]
    fn undo_last_moves_inner_respects_limit_after_filtering() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        // 두 번 이동 → 저널에 move/ok 2건(+pending 2건). limit=1이면 최신 1건만 되돌림.
        for name in ["x.bin", "y.bin"] {
            let s = tmp.path().join(name);
            std::fs::write(&s, b"z").unwrap();
            let d = tmp.path().join("d").join(name);
            execute_moves_inner(&[organize::MovePlan { src: s.to_string_lossy().into(), dst: d.to_string_lossy().into(), class_id: "x".into() }], &jp, 1);
        }
        let undone = undo_last_moves_inner(1, &jp, 9);
        assert_eq!(undone.len(), 1, "filter-before-take: pending 라인이 실제 성공을 밀어내지 않음");
    }

    #[test]
    fn undo_last_moves_inner_reports_failure_when_original_path_reoccupied() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let a = tmp.path().join("a.bin");
        std::fs::write(&a, vec![3u8; 4]).unwrap();
        let a_moved = tmp.path().join("dest").join("a.bin");
        let plans = vec![organize::MovePlan { src: a.to_string_lossy().into(), dst: a_moved.to_string_lossy().into(), class_id: "x".into() }];
        execute_moves_inner(&plans, &jp, 1);
        assert!(a_moved.exists());
        // 원래 자리에 새 파일이 다시 생겨 되돌리기 목적지가 막힘 → move_file이 실패해야 함
        std::fs::write(&a, b"blocker").unwrap();
        let undone = undo_last_moves_inner(1, &jp, 2);
        assert_eq!(undone.len(), 1);
        assert!(!undone[0].ok, "목적지 재점유 시 되돌리기 실패를 보고해야 함");
        assert!(a_moved.exists(), "실패 시 원본은 이동된 위치에 그대로 남음");
    }
}
