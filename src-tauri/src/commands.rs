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
use crate::{dev_artifacts, dupes, rules};

#[derive(Default)]
pub struct AppState {
    pub result: Arc<Mutex<Option<ScanResult>>>,
    pub cancel: Arc<AtomicBool>,
    pub scanning: Arc<AtomicBool>,
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

#[cfg(not(coverage))]
#[cfg_attr(not(feature = "llm-engine"), allow(unused_variables))]
#[tauri::command(async)]
pub fn plan_organize(root: String, app: AppHandle, state: State<AppState>) -> Result<Vec<organize::MovePlan>, String> {
    let onto = load_ontology_from(&bundled_ontology_ttl(&app)?)?;
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
                return Ok(organize::plan_moves_with(&files, &onto, &home, &pick));
            }
        }
    }
    Ok(organize::plan_moves(&files, &onto, &home))
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
