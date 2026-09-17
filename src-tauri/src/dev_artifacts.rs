use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::scanner;

// A development tree can contain millions of generated entries. The inventory remains
// fail-closed for cleanup when this bounded metadata manifest cannot finish; it must never turn
// a partial observation into permission to move a recreated directory to the trash.

/// UI / Tauri path keeps a short budget so incomplete large trees fail closed instead of
/// blocking the interactive session. Headless `disksage-dev-artifacts` may raise this.
pub const ARTIFACT_MANIFEST_BUDGET_UI: Duration = Duration::from_secs(3);
/// Default for the CLI when neither `--manifest-budget-secs` nor the env override is set.
/// Large Rust `target/` trees (1–5 GB) routinely exceed the UI 3s gate.
pub const ARTIFACT_MANIFEST_BUDGET_CLI_DEFAULT: Duration = Duration::from_secs(300);
const ARTIFACT_MANIFEST_MAX_RECORDS: usize = 250_000;
const MANIFEST_BUDGET_ENV: &str = "DISKSAGE_ARTIFACT_MANIFEST_BUDGET_SECS";

/// Resolve the headless CLI manifest budget: explicit seconds win, else env, else 300s.
pub fn resolve_cli_manifest_budget(explicit_secs: Option<u64>) -> Duration {
    if let Some(secs) = explicit_secs {
        return Duration::from_secs(secs);
    }
    if let Ok(raw) = std::env::var(MANIFEST_BUDGET_ENV) {
        if let Ok(secs) = raw.trim().parse::<u64>() {
            return Duration::from_secs(secs);
        }
    }
    ARTIFACT_MANIFEST_BUDGET_CLI_DEFAULT
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DevArtifact {
    pub path: String,
    pub kind: String,
    pub project: String,
    pub bytes: u64,
    pub files: u64,
    pub skipped: u64,
    pub scan_complete: bool,
    /// Deterministic metadata manifest; file contents are never read.
    pub fingerprint: String,
    /// Platform filesystem identity of the candidate root; unlike a path it cannot be reused by
    /// a recreated directory on Unix/Windows.
    pub object_id: String,
    pub age_days: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DevArtifactCleanResult {
    pub path: String,
    pub ok: bool,
    pub error: String,
}

/// (아티팩트 디렉토리명, 같은 부모에 있어야 하는 프로젝트 마커들)
const ARTIFACT_KINDS: &[(&str, &[&str])] = &[
    ("node_modules", &["package.json"]),
    ("target", &["Cargo.toml"]),
    (".venv", &["pyproject.toml", "requirements.txt", "setup.py"]),
    ("venv", &["pyproject.toml", "requirements.txt", "setup.py"]),
    ("__pycache__", &[]), // 마커 불필요 — 이름 자체가 파이썬 캐시
    (".codegraph", &[]), // 재생성 가능한 CodeGraph 인덱스
];

fn artifact_kind(name: &str) -> Option<&'static (&'static str, &'static [&'static str])> {
    ARTIFACT_KINDS.iter().find(|(k, _)| *k == name)
}

fn age_days(path: &Path, now_ms: u64) -> u64 {
    let Ok(md) = path.metadata() else { return 0 };
    let Ok(mtime) = md.modified() else { return 0 };
    let Ok(dur) = mtime.duration_since(std::time::UNIX_EPOCH) else { return 0 };
    let mtime_ms = dur.as_millis() as u64;
    now_ms.saturating_sub(mtime_ms) / 86_400_000
}

#[derive(Default)]
struct ArtifactManifest {
    bytes: u64,
    files: u64,
    skipped: u64,
    scan_complete: bool,
    records: Vec<String>,
    fingerprint: String,
    object_id: String,
}

/// Build a bounded, deterministic metadata-only manifest for one generated directory.
///
/// Paths, kinds, sizes, mtimes, and symlink targets are enough to detect a stale selection while
/// avoiding sensitive content reads. A time/record bound makes the cleanup gate fail closed on
/// unusually large trees instead of blocking the UI indefinitely.
fn artifact_manifest(root: &Path, budget: Duration) -> ArtifactManifest {
    let mut manifest = ArtifactManifest {
        scan_complete: true,
        ..ArtifactManifest::default()
    };
    let root_object_id = crate::safety::filesystem_object_id(root).ok();
    if root_object_id.is_none() {
        manifest.scan_complete = false;
    }
    manifest.object_id = root_object_id.unwrap_or_default();
    let deadline = Instant::now() + budget;
    let walker = walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| entry.depth() == 0 || scanner::keep_entry(entry));

    for entry in walker {
        if Instant::now() >= deadline || manifest.records.len() >= ARTIFACT_MANIFEST_MAX_RECORDS {
            manifest.scan_complete = false;
            break;
        }
        let Ok(entry) = entry else {
            manifest.skipped = manifest.skipped.saturating_add(1);
            manifest.scan_complete = false;
            continue;
        };
        let entry_path = entry.path();
        let relative = entry_path
            .strip_prefix(root)
            .unwrap_or(entry_path)
            .to_string_lossy()
            .replace('\\', "/");
        let relative = if relative.is_empty() { "." } else { &relative };
        let file_type = entry.file_type();
        if file_type.is_dir() {
            let Ok(metadata) = entry.metadata() else {
                manifest.skipped = manifest.skipped.saturating_add(1);
                manifest.scan_complete = false;
                continue;
            };
            let identity = crate::safety::filesystem_object_id(&entry_path).unwrap_or_else(|_| {
                manifest.skipped = manifest.skipped.saturating_add(1);
                manifest.scan_complete = false;
                "<unknown>".into()
            });
            let modified = modified_stamp(&metadata).unwrap_or_else(|| {
                manifest.skipped = manifest.skipped.saturating_add(1);
                manifest.scan_complete = false;
                "<unknown>".into()
            });
            manifest
                .records
                .push(format!("D\0{relative}\0{identity}\0{modified}"));
        } else if file_type.is_file() {
            let Ok(metadata) = entry.metadata() else {
                manifest.skipped = manifest.skipped.saturating_add(1);
                manifest.scan_complete = false;
                continue;
            };
            let identity = crate::safety::filesystem_object_id(&entry_path).unwrap_or_else(|_| {
                manifest.skipped = manifest.skipped.saturating_add(1);
                manifest.scan_complete = false;
                "<unknown>".into()
            });
            let modified = modified_stamp(&metadata).unwrap_or_else(|| {
                manifest.skipped = manifest.skipped.saturating_add(1);
                manifest.scan_complete = false;
                "<unknown>".into()
            });
            manifest.bytes = manifest.bytes.saturating_add(metadata.len());
            manifest.files = manifest.files.saturating_add(1);
            manifest
                .records
                .push(format!("F\0{relative}\0{identity}\0{}\0{modified}", metadata.len()));
        }
    }

    if !manifest.scan_complete {
        manifest.records.push("!incomplete\0bounded-artifact-manifest".into());
    }
    manifest.records.sort_unstable();
    manifest.fingerprint = metadata_fingerprint(&manifest.records);
    manifest
}

fn modified_stamp(metadata: &std::fs::Metadata) -> Option<String> {
    let duration = metadata.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(format!("{}:{}", duration.as_secs(), duration.subsec_nanos()))
}

fn metadata_fingerprint(records: &[String]) -> String {
    let mut hasher = blake3::Hasher::new();
    for record in records {
        hasher.update(&(record.len() as u64).to_le_bytes());
        hasher.update(record.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

/// 마커 인접 아티팩트 디렉토리를 찾아 mtime 나이로 걸러 크기 내림차순으로 반환.
///
/// 2패스로 나눈 이유: 순회 백엔드의 방문 순서에 의존하지 않고 부모/자식 관계를
/// 보장하지 않는다. 그래서 "이미 찾은 아티팩트의 하위는 건너뛴다" 식으로 순회
/// 도중 걸러내면, 중첩 node_modules의 자식이 부모보다 먼저 방문될 경우 둘 다
/// 별도 항목으로 남는다. 1패스에서는 마커 인접 검증까지만 마친 후보 경로를 전부
/// 모으고(순서 무관), 2패스에서 다른 후보의 하위 경로인 것을 제거한 뒤에야 크기를
/// 계산해 중첩분을 이중 계산하지 않는다.
///
/// `manifest_budget` bounds per-artifact metadata inventory. UI callers should pass
/// [`ARTIFACT_MANIFEST_BUDGET_UI`]; headless CLI may pass a larger value.
pub fn find_artifacts(
    root: &Path,
    min_age_days: u64,
    now_ms: u64,
    manifest_budget: Duration,
) -> Vec<DevArtifact> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    let walker = walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            // 심링크/reparse point 제외 — scanner의 순회 전반 패턴과 동일
            entry.depth() == 0 || scanner::keep_entry(entry)
        });

    for entry in walker {
        let Ok(e) = entry else { continue };
        if !e.file_type().is_dir() {
            continue;
        }
        let path = e.path();
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
        let Some((_, markers)) = artifact_kind(&name) else { continue };
        let parent = path.parent().unwrap_or(root);
        let marker_ok = markers.is_empty() || markers.iter().any(|m| parent.join(m).exists());
        if marker_ok {
            candidates.push(path.to_path_buf());
        }
    }

    // 다른 후보의 하위 경로(중첩 아티팩트)는 제거 — 방문 순서에 의존하지 않는 비교
    let top_level: Vec<&Path> = candidates
        .iter()
        .enumerate()
        .filter(|(i, p)| {
            !candidates
                .iter()
                .enumerate()
                .any(|(j, other)| *i != j && p.starts_with(other))
        })
        .map(|(_, p)| p.as_path())
        .collect();

    let mut found: Vec<DevArtifact> = top_level
        .into_iter()
        .filter_map(|path| {
            let age = if now_ms == u64::MAX { u64::MAX } else { age_days(path, now_ms) };
            if age < min_age_days {
                return None;
            }
            let name = path.file_name()?.to_string_lossy().into_owned();
            let (kind, _) = artifact_kind(&name)?;
            let parent = path.parent().unwrap_or(root);
            let manifest = artifact_manifest(path, manifest_budget);
            Some(DevArtifact {
                path: path.to_string_lossy().into_owned(),
                kind: kind.to_string(),
                project: parent
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                bytes: manifest.bytes,
                files: manifest.files,
                skipped: manifest.skipped,
                scan_complete: manifest.scan_complete,
                fingerprint: manifest.fingerprint,
                object_id: manifest.object_id,
                age_days: if age == u64::MAX { 0 } else { age },
            })
        })
        .collect();

    found.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    found
}

/// Locate the nearest enclosing worktree root (directory containing `.git`) for an artifact path.
pub fn enclosing_worktree_root(artifact: &Path) -> Option<PathBuf> {
    for ancestor in artifact.ancestors() {
        if ancestor.join(".git").exists() {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

/// Assess whether a rebuildable artifact should be withheld under Orca/dev protection criteria.
pub fn assess_dev_artifact_protection(
    artifact: &DevArtifact,
    context: &crate::reclaim_protection::ProtectionContext,
) -> crate::reclaim_protection::ProtectionAssessment {
    let path = Path::new(&artifact.path);
    let mut reasons = Vec::new();

    // Never reclaim protected data directory names if they somehow appear as candidates.
    if crate::reclaim_protection::is_protected_data_dir_name(&artifact.kind) {
        match artifact.kind.as_str() {
            "local" => reasons.push(crate::reclaim_protection::REASON_PROTECTED_DATA_LOCAL.into()),
            "results" => {
                reasons.push(crate::reclaim_protection::REASON_PROTECTED_DATA_RESULTS.into())
            }
            _ => {}
        }
    }
    for component in path.components() {
        if let std::path::Component::Normal(name) = component {
            let name = name.to_string_lossy();
            if crate::reclaim_protection::is_protected_data_dir_name(&name) {
                match name.as_ref() {
                    "local" => reasons
                        .push(crate::reclaim_protection::REASON_PROTECTED_DATA_LOCAL.into()),
                    "results" => reasons
                        .push(crate::reclaim_protection::REASON_PROTECTED_DATA_RESULTS.into()),
                    _ => {}
                }
            }
            if crate::reclaim_protection::is_orchestration_lead_name(&name) {
                reasons.push(crate::reclaim_protection::REASON_ORCHESTRATION_LEAD.into());
            }
        }
    }

    if let Some(worktree) = enclosing_worktree_root(path) {
        let assessment = crate::reclaim_protection::assess_worktree_protections(
            &worktree,
            None,
            None,
            context,
            crate::reclaim_protection::path_is_under_any(&worktree, &context.orca_live_worktree_paths),
            false,
            None,
            false,
            None,
            true,
        );
        reasons.extend(
            crate::reclaim_protection::artifact_blocking_reason_codes(&assessment).into_iter(),
        );
        // Editable install specifically protects target/python trees under the worktree.
        if assessment
            .reason_codes
            .iter()
            .any(|code| code == crate::reclaim_protection::REASON_EDITABLE_INSTALL)
            && (artifact.kind == "target"
                || artifact.kind == ".venv"
                || artifact.kind == "venv"
                || path
                    .components()
                    .any(|c| matches!(c, std::path::Component::Normal(n) if n == "python")))
        {
            if !reasons
                .iter()
                .any(|code| code == crate::reclaim_protection::REASON_EDITABLE_INSTALL)
            {
                reasons.push(crate::reclaim_protection::REASON_EDITABLE_INSTALL.into());
            }
        }
    }

    if let Some(window) = context.recent_write_window_secs {
        let now = context
            .now_unix_secs
            .unwrap_or_else(crate::reclaim_protection::now_unix_secs);
        if let Some(code) = crate::reclaim_protection::recent_write_reason(path, window, now) {
            reasons.push(code.to_string());
        }
    }

    crate::reclaim_protection::ProtectionAssessment::with_reasons(reasons)
}

/// Partition artifacts into reclaimable vs protected using stable reason codes.
pub fn partition_artifacts_by_protection(
    artifacts: &[DevArtifact],
    context: &crate::reclaim_protection::ProtectionContext,
) -> (Vec<DevArtifact>, Vec<(DevArtifact, crate::reclaim_protection::ProtectionAssessment)>) {
    let mut reclaimable = Vec::new();
    let mut protected = Vec::new();
    for artifact in artifacts {
        let assessment = assess_dev_artifact_protection(artifact, context);
        let blockers =
            crate::reclaim_protection::artifact_blocking_reason_codes(&assessment);
        // Also block protected-data path hits for artifact cleanup.
        let data_blocked = assessment.reason_codes.iter().any(|code| {
            code.starts_with("protected-data-path:")
                || code == crate::reclaim_protection::REASON_PROTECTED_CREDENTIALS
        });
        if blockers.is_empty() && !data_blocked {
            reclaimable.push(artifact.clone());
        } else {
            protected.push((artifact.clone(), assessment));
        }
    }
    (reclaimable, protected)
}

/// Re-scan and move only unchanged development artifacts to OS Trash.
///
/// The request manifest is deliberately compared against a fresh bounded scan. A path match is
/// not sufficient because a recreated `target` or `node_modules` directory could otherwise cause
/// an unrelated artifact to be removed.
pub fn clean_artifacts(
    requests: &[DevArtifact],
    root: &Path,
    min_age_days: u64,
    journal_path: &Path,
    now_ms: u64,
    manifest_budget: Duration,
) -> Vec<DevArtifactCleanResult> {
    clean_artifacts_with_protection(
        requests,
        root,
        min_age_days,
        journal_path,
        now_ms,
        manifest_budget,
        None,
    )
}

/// Like [`clean_artifacts`], but refuses paths blocked by an optional protection context.
pub fn clean_artifacts_with_protection(
    requests: &[DevArtifact],
    root: &Path,
    min_age_days: u64,
    journal_path: &Path,
    now_ms: u64,
    manifest_budget: Duration,
    protection: Option<&crate::reclaim_protection::ProtectionContext>,
) -> Vec<DevArtifactCleanResult> {
    let current = find_artifacts(root, min_age_days, now_ms, manifest_budget);
    requests
        .iter()
        .map(|request| {
            if let Some(context) = protection {
                let assessment = assess_dev_artifact_protection(request, context);
                let blockers =
                    crate::reclaim_protection::artifact_blocking_reason_codes(&assessment);
                let data_blocked = assessment.reason_codes.iter().any(|code| {
                    code.starts_with("protected-data-path:")
                        || code == crate::reclaim_protection::REASON_PROTECTED_CREDENTIALS
                });
                if !blockers.is_empty() || data_blocked {
                    return DevArtifactCleanResult {
                        path: request.path.clone(),
                        ok: false,
                        error: format!(
                            "protected-by-criteria:{}",
                            assessment.reason_codes.join(",")
                        ),
                    };
                }
            }

            let matches = current.iter().find(|candidate| {
                candidate.path == request.path
                    && candidate.kind == request.kind
                    && candidate.project == request.project
                    && candidate.bytes == request.bytes
                    && candidate.files == request.files
                    && candidate.skipped == request.skipped
                    && candidate.scan_complete
                    && request.scan_complete
                    && request.skipped == 0
                    && candidate.fingerprint == request.fingerprint
                    && !request.object_id.is_empty()
                    && candidate.object_id == request.object_id
                    && candidate.age_days >= request.age_days
            });

            if matches.is_none() {
                return DevArtifactCleanResult {
                    path: request.path.clone(),
                    ok: false,
                    error: "development artifact changed or its bounded manifest is incomplete; rescan before cleanup".into(),
                };
            }

            match crate::safety::trash_delete_if_identity(
                Path::new(&request.path),
                &request.object_id,
                request.bytes,
                journal_path,
                now_ms,
            ) {
                Ok(()) => DevArtifactCleanResult {
                    path: request.path.clone(),
                    ok: true,
                    error: String::new(),
                },
                Err(error) => DevArtifactCleanResult {
                    path: request.path.clone(),
                    ok: false,
                    error: error.to_string(),
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn project(root: &std::path::Path, name: &str, marker: &str, artifact: &str) -> std::path::PathBuf {
        let p = root.join(name);
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join(marker), b"{}").unwrap();
        let a = p.join(artifact);
        fs::create_dir_all(&a).unwrap();
        fs::write(a.join("payload.bin"), vec![0u8; 256]).unwrap();
        a
    }

    #[test]
    fn finds_marker_adjacent_artifacts() {
        let tmp = tempfile::tempdir().unwrap();
        project(tmp.path(), "webapp", "package.json", "node_modules");
        project(tmp.path(), "cli", "Cargo.toml", "target");
        // 마커 없는 가짜 — 탐지되면 안 됨
        let orphan = tmp.path().join("random").join("node_modules");
        fs::create_dir_all(&orphan).unwrap();

        let found = find_artifacts(tmp.path(), 0, u64::MAX, ARTIFACT_MANIFEST_BUDGET_UI);

        let kinds: Vec<&str> = found.iter().map(|a| a.kind.as_str()).collect();
        assert!(kinds.contains(&"node_modules"));
        assert!(kinds.contains(&"target"));
        assert!(
            !found.iter().any(|a| a.path.contains("random")),
            "마커 없는 아티팩트는 제외"
        );
        let nm = found.iter().find(|a| a.kind == "node_modules").unwrap();
        assert_eq!(nm.project, "webapp");
        assert_eq!(nm.bytes, 256);
        assert_eq!(nm.age_days, 0, "sentinel now_ms는 age_days 0으로 보고");
    }

    #[test]
    fn finds_regenerable_codegraph_indexes() {
        let tmp = tempfile::tempdir().unwrap();
        let index = tmp.path().join("repo/.codegraph");
        fs::create_dir_all(&index).unwrap();
        fs::write(index.join("db"), b"generated").unwrap();

        let found = find_artifacts(tmp.path(), 0, u64::MAX, ARTIFACT_MANIFEST_BUDGET_UI);

        assert!(found.iter().any(|artifact| {
            artifact.kind == ".codegraph" && artifact.path == index.to_string_lossy()
        }));
    }

    #[test]
    fn respects_min_age() {
        let tmp = tempfile::tempdir().unwrap();
        project(tmp.path(), "fresh", "package.json", "node_modules");
        // 방금 만든 것: min_age_days=30이면 제외 (now = 실제 현재로는 나이가 0)
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        assert!(find_artifacts(tmp.path(), 30, now_ms, ARTIFACT_MANIFEST_BUDGET_UI).is_empty());
        // min_age_days=0이면 포함
        assert_eq!(
            find_artifacts(tmp.path(), 0, now_ms, ARTIFACT_MANIFEST_BUDGET_UI).len(),
            1
        );
    }

    #[test]
    fn artifacts_inside_artifacts_are_not_double_counted() {
        let tmp = tempfile::tempdir().unwrap();
        let nm = project(tmp.path(), "app", "package.json", "node_modules");
        // node_modules 내부의 중첩 node_modules — 별도 항목이면 안 됨
        let nested = nm.join("dep").join("node_modules");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nm.join("dep").join("package.json"), b"{}").unwrap();

        assert_eq!(
            find_artifacts(tmp.path(), 0, u64::MAX, ARTIFACT_MANIFEST_BUDGET_UI).len(),
            1
        );
    }

    #[test]
    fn cleanup_fails_closed_when_artifact_identity_changes() {
        let tmp = tempfile::tempdir().unwrap();
        project(tmp.path(), "app", "package.json", "node_modules");
        let candidates = find_artifacts(tmp.path(), 0, u64::MAX, ARTIFACT_MANIFEST_BUDGET_UI);
        assert_eq!(candidates.len(), 1);
        let journal = tmp.path().join("journal.jsonl");
        let original = tmp.path().join("original-node-modules");
        let live = tmp.path().join("app/node_modules");
        std::fs::rename(&live, &original).unwrap();
        std::fs::create_dir(&live).unwrap();
        std::fs::write(live.join("replacement.bin"), b"replacement").unwrap();
        let results = clean_artifacts(
            &candidates,
            tmp.path(),
            0,
            &journal,
            1,
            ARTIFACT_MANIFEST_BUDGET_UI,
        );
        assert_eq!(results.len(), 1);
        assert!(!results[0].ok);
        assert!(results[0].error.contains("changed"));
        assert!(live.exists());
        assert!(original.exists());
        assert!(!journal.exists(), "stale identity must not create a journal");
    }

    #[test]
    fn zero_manifest_budget_fails_closed_larger_budget_completes() {
        let tmp = tempfile::tempdir().unwrap();
        project(tmp.path(), "crate", "Cargo.toml", "target");

        let red = find_artifacts(tmp.path(), 0, u64::MAX, Duration::ZERO);
        assert_eq!(red.len(), 1);
        assert!(
            !red[0].scan_complete,
            "zero budget must leave scan_complete=false (RED)"
        );

        let green = find_artifacts(tmp.path(), 0, u64::MAX, ARTIFACT_MANIFEST_BUDGET_UI);
        assert_eq!(green.len(), 1);
        assert!(
            green[0].scan_complete,
            "UI budget must finish a small tree (GREEN)"
        );
        assert!(green[0].files >= 1);
        assert!(!green[0].object_id.is_empty());
        assert!(!green[0].fingerprint.is_empty());
        assert_ne!(
            red[0].fingerprint, green[0].fingerprint,
            "incomplete marker must change the fingerprint"
        );
    }

    #[test]
    fn incomplete_manifest_cannot_be_trashed_even_with_generous_budget_on_clean() {
        let tmp = tempfile::tempdir().unwrap();
        project(tmp.path(), "crate", "Cargo.toml", "target");
        let incomplete = find_artifacts(tmp.path(), 0, u64::MAX, Duration::ZERO);
        assert!(!incomplete[0].scan_complete);
        let journal = tmp.path().join("journal.jsonl");
        let results = clean_artifacts(
            &incomplete,
            tmp.path(),
            0,
            &journal,
            1,
            ARTIFACT_MANIFEST_BUDGET_CLI_DEFAULT,
        );
        assert!(!results[0].ok);
        assert!(tmp.path().join("crate/target").exists());
        assert!(!journal.exists());
    }

    #[test]
    fn resolve_cli_manifest_budget_prefers_explicit_then_default() {
        assert_eq!(
            resolve_cli_manifest_budget(Some(42)),
            Duration::from_secs(42)
        );
        assert_eq!(ARTIFACT_MANIFEST_BUDGET_UI, Duration::from_secs(3));
        assert_eq!(ARTIFACT_MANIFEST_BUDGET_CLI_DEFAULT, Duration::from_secs(300));
        // Explicit None uses env-or-default; when env is unset this equals CLI default.
        let resolved = resolve_cli_manifest_budget(None);
        if std::env::var_os(MANIFEST_BUDGET_ENV).is_none() {
            assert_eq!(resolved, ARTIFACT_MANIFEST_BUDGET_CLI_DEFAULT);
        }
    }

    #[test]
    fn partition_protects_orchestration_lead_and_editable_target_red_to_green() {
        let tmp = tempfile::tempdir().unwrap();
        let lead = tmp.path().join("orchestration-lead-demo");
        fs::create_dir_all(lead.join("target")).unwrap();
        fs::write(lead.join("Cargo.toml"), b"[package]\nname=\"x\"\n").unwrap();
        fs::write(lead.join("target/x.bin"), vec![0u8; 64]).unwrap();
        fs::write(lead.join(".git"), b"gitdir: /tmp/fake\n").unwrap();

        let idle = project(tmp.path(), "idle-crate", "Cargo.toml", "target");
        fs::write(idle.parent().unwrap().join(".git"), b"gitdir: /tmp/fake2\n").unwrap();

        let found = find_artifacts(tmp.path(), 0, u64::MAX, ARTIFACT_MANIFEST_BUDGET_UI);
        assert!(found.len() >= 2);

        // RED: lead worktree artifacts are blocked.
        let context = crate::reclaim_protection::ProtectionContext::default();
        let (reclaimable, protected) = partition_artifacts_by_protection(&found, &context);
        assert!(protected.iter().any(|(artifact, assessment)| {
            artifact.path.contains("orchestration-lead-demo")
                && assessment
                    .reason_codes
                    .iter()
                    .any(|code| code == crate::reclaim_protection::REASON_ORCHESTRATION_LEAD)
        }));
        assert!(reclaimable.iter().any(|artifact| artifact.path.contains("idle-crate")));

        // GREEN: idle crate remains reclaimable without recent-write window.
        assert!(reclaimable.iter().any(|artifact| {
            artifact.kind == "target" && artifact.path.contains("idle-crate")
        }));
    }
}
