//! macOS Library 관계 기반 고아 후보 계획.
//!
//! 내용은 읽지 않고 파일명·종류·크기·mtime만 bounded manifest로 수집한다. 후보 계획은
//! advisory이며, 실제 이동은 재계획·지문 일치·safety::trash_delete를 모두 통과해야 한다.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const DM: &str = "https://disksage.app/ontology#";
const PLAN_BUDGET: Duration = Duration::from_secs(5);
const MAX_CANDIDATES: usize = 256;
const MAX_RECORDS: usize = 100_000;

#[derive(Debug, Clone, serde::Serialize)]
pub struct RelationEvidence {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub source: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OrphanCandidate {
    pub path: String,
    pub kind: String,
    pub bundle_id: Option<String>,
    pub bytes: u64,
    pub files: u64,
    pub skipped: u64,
    pub scan_complete: bool,
    pub fingerprint: String,
    pub ontology_class: String,
    pub confidence: String,
    pub relations: Vec<RelationEvidence>,
    pub review_reasons: Vec<String>,
    pub auto_trash_eligible: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OrphanPlan {
    pub schema_version: u32,
    pub root: String,
    pub generated_at_ms: u64,
    pub plan_fingerprint: String,
    pub candidate_bytes: u64,
    pub scan_complete: bool,
    pub candidates: Vec<OrphanCandidate>,
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct OrphanCleanupRequest {
    pub path: String,
    pub bytes: u64,
    pub files: u64,
    pub skipped: u64,
    pub scan_complete: bool,
    pub fingerprint: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OrphanJudgment {
    pub path: String,
    pub plan_fingerprint: String,
    pub verdict: crate::llm::Verdict,
    pub reason: String,
    pub model_name: String,
    pub judged_at_ms: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OrphanJudgmentReport {
    pub plan_fingerprint: String,
    pub judgments: Vec<OrphanJudgment>,
}

#[derive(Default)]
struct Manifest {
    bytes: u64,
    files: u64,
    skipped: u64,
    scan_complete: bool,
    records: Vec<String>,
}

/// macOS 전용 Library 계획. 다른 플랫폼에서는 앱 UI가 해당 기능을 노출하지 않는다.
pub fn plan(home: &Path, now_ms: u64) -> Result<OrphanPlan, String> {
    #[cfg(target_os = "macos")]
    {
        let library = home.join("Library");
        let watched = [
            (library.join("Application Support"), "application-support"),
            (library.join("Caches"), "cache"),
        ];
        let application_roots = [
            PathBuf::from("/Applications"),
            home.join("Applications"),
            PathBuf::from("/System/Applications"),
        ];
        return plan_for_roots(home, &watched, &application_roots, now_ms);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (home, now_ms);
        Err("orphan-plan-macos-only".into())
    }
}

/// 관계 증거를 포함한 로컬 LLM 자문. 반환값은 UI 배지일 뿐 휴지통 권한이 아니다.
pub fn judge_plan(
    plan: &OrphanPlan,
    engine: Option<&dyn crate::llm::InferenceEngine>,
    model_name: &str,
    now_ms: u64,
) -> OrphanJudgmentReport {
    let judgments = plan
        .candidates
        .iter()
        .map(|candidate| {
            let (mut verdict, mut reason) = match engine {
                Some(engine) => engine
                    .infer(&prompt(candidate))
                    .map(|raw| crate::llm::parse_verdict_full(&raw))
                    .unwrap_or((crate::llm::Verdict::Unrated, String::new())),
                None => (crate::llm::Verdict::Unrated, String::new()),
            };
            if verdict == crate::llm::Verdict::Safe && !candidate.auto_trash_eligible {
                verdict = crate::llm::Verdict::Caution;
                reason = "deterministic relation/safety gate requires manual review".into();
            }
            OrphanJudgment {
                path: candidate.path.clone(),
                plan_fingerprint: plan.plan_fingerprint.clone(),
                verdict,
                reason,
                model_name: model_name.into(),
                judged_at_ms: now_ms,
            }
        })
        .collect();
    OrphanJudgmentReport {
        plan_fingerprint: plan.plan_fingerprint.clone(),
        judgments,
    }
}

fn prompt(candidate: &OrphanCandidate) -> String {
    let relations = candidate
        .relations
        .iter()
        .take(8)
        .map(|relation| {
            format!(
                "{} --{}--> {}",
                relation.subject, relation.predicate, relation.object
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "You are an advisory macOS orphan-data reviewer. Use only bounded metadata and the explicit relations below; never infer file contents.\n\
         Candidate: path={path} kind={kind} bundle_id={bundle:?} bytes={bytes} files={files} complete={complete} class={class} confidence={confidence}\n\
         Relations: {relations}\n\
         Review reasons: {reasons}\n\
         Reply ONLY JSON {{\"verdict\":\"safe|caution|keep\",\"reason\":\"<short>\"}}.\n\
         safe means a fully scanned regenerable cache only; caution means manual review; keep means preserve. Never output commands or paths to delete.",
        path = candidate.path,
        kind = candidate.kind,
        bundle = candidate.bundle_id,
        bytes = candidate.bytes,
        files = candidate.files,
        complete = candidate.scan_complete,
        class = candidate.ontology_class,
        confidence = candidate.confidence,
        relations = relations,
        reasons = candidate.review_reasons.join("; "),
    )
}

#[cfg(target_os = "macos")]
pub fn plan_for_roots(
    home: &Path,
    watched: &[(PathBuf, &str)],
    application_roots: &[PathBuf],
    now_ms: u64,
) -> Result<OrphanPlan, String> {
    let deadline = Instant::now() + PLAN_BUDGET;
    let (installed, installed_inventory_complete) = installed_bundle_ids(application_roots);
    let mut candidates = Vec::new();
    let mut notices = vec![
        "metadata-only: file contents are never read".to_string(),
        "Application Support candidates require manual review".to_string(),
        "Containers, Mobile Documents, Mail, Preferences, and Keychains are excluded".to_string(),
    ];

    for (root, kind) in watched {
        if Instant::now() >= deadline || candidates.len() >= MAX_CANDIDATES {
            notices.push("bounded orphan scan stopped before all entries were observed".into());
            break;
        }
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            if Instant::now() >= deadline || candidates.len() >= MAX_CANDIDATES {
                notices.push("bounded orphan scan stopped before all entries were observed".into());
                break;
            }
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                if path.exists() {
                    continue;
                }
                candidates.push(broken_link_candidate(&path, root));
                continue;
            }
            if !file_type.is_dir() {
                continue;
            }
            let Some(bundle_id) = bundle_id_from_name(&path) else {
                continue;
            };
            if installed.contains(&bundle_id) {
                continue;
            }
            let manifest = bounded_manifest(&path, deadline);
            candidates.push(directory_candidate(
                &path,
                kind,
                bundle_id,
                manifest,
                installed_inventory_complete,
            ));
        }
    }

    candidates.sort_by(|a, b| a.path.cmp(&b.path));
    let candidate_bytes = candidates.iter().map(|c| c.bytes).sum();
    let scan_complete = candidates.iter().all(|c| c.scan_complete);
    if !scan_complete {
        notices.push(
            "one or more candidate manifests are incomplete; no automatic trash is allowed".into(),
        );
    }
    if !installed_inventory_complete {
        notices.push(
            "installed application inventory is incomplete; cache candidates remain review-only"
                .into(),
        );
    }
    let plan_fingerprint = plan_fingerprint(&candidates);
    let root = home.to_string_lossy().into_owned();
    let _ = now_ms;
    Ok(OrphanPlan {
        schema_version: 1,
        root,
        generated_at_ms: now_ms,
        plan_fingerprint,
        candidate_bytes,
        scan_complete,
        candidates,
        notices,
    })
}

#[cfg(target_os = "macos")]
fn bundle_id_from_name(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    valid_bundle_id(name).then(|| name.to_string())
}

#[cfg(target_os = "macos")]
fn valid_bundle_id(value: &str) -> bool {
    let mut parts = value.split('.');
    matches!(parts.next(), Some("com" | "org" | "net" | "io" | "app"))
        && parts.all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        })
}

#[cfg(target_os = "macos")]
fn installed_bundle_ids(roots: &[PathBuf]) -> (BTreeSet<String>, bool) {
    let mut ids = BTreeSet::new();
    let mut complete = true;
    for root in roots {
        complete &= collect_bundle_ids(root, 0, &mut ids);
    }
    (ids, complete)
}

#[cfg(target_os = "macos")]
fn collect_bundle_ids(root: &Path, depth: usize, ids: &mut BTreeSet<String>) -> bool {
    if depth > 3 {
        return true;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return !root.exists();
    };
    let mut complete = true;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            if path.extension().and_then(|e| e.to_str()) == Some("app") {
                if let Some(id) = read_bundle_id(&path) {
                    ids.insert(id);
                }
            } else {
                complete &= collect_bundle_ids(&path, depth + 1, ids);
            }
        }
    }
    complete
}

#[cfg(target_os = "macos")]
fn read_bundle_id(app: &Path) -> Option<String> {
    let bytes = std::fs::read(app.join("Contents/Info.plist")).ok()?;
    if bytes.len() > 1_048_576 {
        return None;
    }
    let value = plist::Value::from_reader(std::io::Cursor::new(bytes)).ok()?;
    let id = value
        .as_dictionary()?
        .get("CFBundleIdentifier")?
        .as_string()?
        .to_string();
    valid_bundle_id(&id).then_some(id)
}

#[cfg(target_os = "macos")]
fn bounded_manifest(root: &Path, deadline: Instant) -> Manifest {
    let mut out = Manifest {
        scan_complete: true,
        ..Manifest::default()
    };
    collect_manifest(root, root, deadline, &mut out);
    if !out.scan_complete {
        out.records
            .push("!incomplete\0bounded-orphan-manifest".into());
    }
    out.records.sort_unstable();
    out
}

#[cfg(target_os = "macos")]
fn collect_manifest(root: &Path, dir: &Path, deadline: Instant, out: &mut Manifest) {
    if Instant::now() >= deadline || out.records.len() >= MAX_RECORDS {
        out.scan_complete = false;
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        out.skipped = out.skipped.saturating_add(1);
        out.scan_complete = false;
        return;
    };
    for entry in entries.flatten() {
        if Instant::now() >= deadline || out.records.len() >= MAX_RECORDS {
            out.scan_complete = false;
            return;
        }
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let Ok(file_type) = entry.file_type() else {
            out.skipped = out.skipped.saturating_add(1);
            out.scan_complete = false;
            continue;
        };
        if file_type.is_symlink() {
            let target = std::fs::read_link(&path)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| "<unreadable>".into());
            out.records.push(format!("S\0{relative}\0{target}"));
        } else if file_type.is_dir() {
            out.records.push(format!("D\0{relative}"));
            collect_manifest(root, &path, deadline, out);
        } else if file_type.is_file() {
            let Ok(metadata) = entry.metadata() else {
                out.skipped = out.skipped.saturating_add(1);
                out.scan_complete = false;
                continue;
            };
            let modified = modified_stamp(&metadata).unwrap_or_else(|| {
                out.skipped = out.skipped.saturating_add(1);
                out.scan_complete = false;
                "<unknown>".into()
            });
            out.bytes = out.bytes.saturating_add(metadata.len());
            out.files = out.files.saturating_add(1);
            out.records
                .push(format!("F\0{relative}\0{}\0{modified}", metadata.len()));
        }
    }
}

#[cfg(target_os = "macos")]
fn modified_stamp(metadata: &std::fs::Metadata) -> Option<String> {
    let duration = metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    Some(format!(
        "{}:{}",
        duration.as_secs(),
        duration.subsec_nanos()
    ))
}

#[cfg(target_os = "macos")]
fn metadata_fingerprint(records: &[String]) -> String {
    let mut hasher = blake3::Hasher::new();
    for record in records {
        hasher.update(&(record.len() as u64).to_le_bytes());
        hasher.update(record.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

#[cfg(target_os = "macos")]
fn directory_candidate(
    path: &Path,
    kind: &str,
    bundle_id: String,
    manifest: Manifest,
    installed_inventory_complete: bool,
) -> OrphanCandidate {
    let is_cache = kind == "cache";
    let ontology_class = if is_cache {
        format!("{DM}RegenerableCache")
    } else {
        format!("{DM}ApplicationSupport")
    };
    let app = format!("urn:bundle:{bundle_id}");
    let mut relations = vec![
        relation(
            path,
            "instanceOf",
            &format!("{DM}OrphanCandidate"),
            "metadata",
        ),
        relation(path, "locatedIn", &ontology_class, "path ontology"),
        relation(
            path,
            "uninstalledApplicationOf",
            &app,
            "bundle-id inventory",
        ),
    ];
    if is_cache {
        relations.push(relation(path, "mayBeRegeneratedBy", &app, "cache ontology"));
    } else {
        relations.push(relation(
            path,
            "mayContain",
            &format!("{DM}ProtectedUserData"),
            "Apple directory semantics",
        ));
    }
    let mut review_reasons = vec!["bundle-id-not-present-in-installed-applications".into()];
    if is_cache {
        review_reasons.push("cache-is-regenerable-but-still-requires-confirmation".into());
    } else {
        review_reasons.push("Application Support may contain user data".into());
    }
    if !installed_inventory_complete {
        review_reasons.push("installed-application-inventory-incomplete".into());
    }
    let fingerprint = metadata_fingerprint(&manifest.records);
    OrphanCandidate {
        path: path.to_string_lossy().into_owned(),
        kind: kind.into(),
        bundle_id: Some(bundle_id),
        bytes: manifest.bytes,
        files: manifest.files,
        skipped: manifest.skipped,
        scan_complete: manifest.scan_complete,
        fingerprint,
        ontology_class,
        confidence: if is_cache {
            "high".into()
        } else {
            "medium".into()
        },
        relations,
        review_reasons,
        auto_trash_eligible: is_cache
            && installed_inventory_complete
            && manifest.scan_complete
            && manifest.skipped == 0,
    }
}

#[cfg(target_os = "macos")]
fn broken_link_candidate(path: &Path, root: &Path) -> OrphanCandidate {
    let target = std::fs::read_link(path)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "<unreadable>".into());
    let fingerprint = metadata_fingerprint(&[format!("S\0{target}")]);
    OrphanCandidate {
        path: path.to_string_lossy().into_owned(),
        kind: "broken-link".into(),
        bundle_id: None,
        bytes: 0,
        files: 0,
        skipped: 0,
        scan_complete: true,
        fingerprint,
        ontology_class: format!("{DM}OrphanCandidate"),
        confidence: "high".into(),
        relations: vec![
            relation(
                path,
                "instanceOf",
                &format!("{DM}OrphanCandidate"),
                "metadata",
            ),
            relation(path, "locatedIn", &root.to_string_lossy(), "path metadata"),
        ],
        review_reasons: vec!["broken-symlink-requires-manual-review".into()],
        auto_trash_eligible: false,
    }
}

#[cfg(target_os = "macos")]
fn relation(subject: &Path, predicate: &str, object: &str, source: &str) -> RelationEvidence {
    RelationEvidence {
        subject: subject.to_string_lossy().into_owned(),
        predicate: format!("{DM}{predicate}"),
        object: object.to_string(),
        source: source.into(),
    }
}

#[cfg(target_os = "macos")]
fn plan_fingerprint(candidates: &[OrphanCandidate]) -> String {
    let mut hasher = blake3::Hasher::new();
    for candidate in candidates {
        for value in [
            candidate.path.as_str(),
            candidate.kind.as_str(),
            candidate.fingerprint.as_str(),
        ] {
            hasher.update(&(value.len() as u64).to_le_bytes());
            hasher.update(value.as_bytes());
        }
    }
    hasher.finalize().to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn finds_uninstalled_cache_and_keeps_application_support_manual() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let library = home.join("Library");
        let support = library.join("Application Support");
        let caches = library.join("Caches");
        std::fs::create_dir_all(support.join("com.example.old")).unwrap();
        std::fs::create_dir_all(caches.join("com.example.old")).unwrap();
        std::fs::write(caches.join("com.example.old/item.bin"), b"cache").unwrap();
        let app_root = tmp.path().join("Applications");
        std::fs::create_dir_all(&app_root).unwrap();
        let plan = plan_for_roots(
            home,
            &[(support, "application-support"), (caches, "cache")],
            &[app_root],
            42,
        )
        .unwrap();
        assert_eq!(plan.generated_at_ms, 42);
        assert_eq!(plan.candidates.len(), 2);
        let support_candidate = plan
            .candidates
            .iter()
            .find(|c| c.kind == "application-support")
            .unwrap();
        assert!(!support_candidate.auto_trash_eligible);
        assert!(support_candidate
            .relations
            .iter()
            .any(|r| r.predicate.ends_with("mayContain")));
        let cache_candidate = plan.candidates.iter().find(|c| c.kind == "cache").unwrap();
        assert!(cache_candidate.auto_trash_eligible);
        assert!(cache_candidate.bytes > 0);
        assert!(!plan.plan_fingerprint.is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn installed_bundle_id_suppresses_candidate() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let support = home.join("Library/Application Support");
        std::fs::create_dir_all(support.join("com.example.app")).unwrap();
        let app_root = home.join("Applications");
        let app = app_root.join("Example.app/Contents");
        std::fs::create_dir_all(&app).unwrap();
        std::fs::write(
            app.join("Info.plist"),
            br#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>com.example.app</string></dict></plist>"#,
        )
        .unwrap();
        let plan =
            plan_for_roots(home, &[(support, "application-support")], &[app_root], 1).unwrap();
        assert!(plan.candidates.is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn llm_safe_cannot_authorize_application_support() {
        struct Safe;
        impl crate::llm::InferenceEngine for Safe {
            fn infer(&self, prompt: &str) -> Result<String, String> {
                assert!(prompt.contains("Relations:"));
                assert!(!prompt.contains("rm -rf"));
                Ok(r#"{"verdict":"safe","reason":"looks empty"}"#.into())
            }
        }
        let tmp = tempfile::tempdir().unwrap();
        let support = tmp.path().join("Library/Application Support");
        let caches = tmp.path().join("Library/Caches");
        std::fs::create_dir_all(support.join("com.example.old")).unwrap();
        std::fs::create_dir_all(caches.join("com.example.old")).unwrap();
        let plan = plan_for_roots(
            tmp.path(),
            &[(support, "application-support"), (caches, "cache")],
            &[tmp.path().join("Applications")],
            3,
        )
        .unwrap();
        let report = judge_plan(&plan, Some(&Safe), "test-model", 4);
        let support = report
            .judgments
            .iter()
            .find(|judgment| judgment.path.contains("Application Support"))
            .unwrap();
        assert_eq!(support.verdict, crate::llm::Verdict::Caution);
        assert!(support.reason.contains("manual review"));
        let cache = report
            .judgments
            .iter()
            .find(|judgment| judgment.path.contains("Caches"))
            .unwrap();
        assert_eq!(cache.verdict, crate::llm::Verdict::Safe);
    }
}
