//! Evidence-bound reclaim planning for generated artifacts under native temporary roots.

use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::dev_artifacts::{
    clean_artifact_exact, inspect_artifact, DevArtifact, DevArtifactCleanResult,
};
use crate::git_worktree::GitWorktreeActiveUseEvidence;

// The shared development-artifact inspector has its own three-second manifest ceiling. Keep the
// enclosing temp-root budget above that ceiling, then pass only the remaining wall-clock budget to
// the potentially blocking active-handle probe so one candidate cannot turn this bounded planner
// into a 30-second operation.
const DISCOVERY_BUDGET: Duration = Duration::from_millis(3_500);
const MAX_DISCOVERY_ENTRIES: usize = 4_096;
const MAX_CANDIDATES: usize = 64;
pub const MAX_APPROVAL_AGE_MS: u64 = 5 * 60 * 1_000;

#[derive(Debug, Clone, Serialize)]
pub struct TempReclaimCandidate {
    pub artifact: DevArtifact,
    pub active_use: GitWorktreeActiveUseEvidence,
    pub candidate_fingerprint: String,
    pub eligible_for_approval: bool,
    pub exact_approval_phrase: Option<String>,
    pub blockers: Vec<String>,
    pub permanent_delete_available: bool,
    pub next_action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TempReclaimPlan {
    pub schema_version: u32,
    pub schema_kind: &'static str,
    pub requested_root: String,
    pub canonical_root: String,
    pub observed_at_ms: u64,
    pub scan_complete: bool,
    pub visited_entries: usize,
    pub unavailable_entries: usize,
    pub candidates: Vec<TempReclaimCandidate>,
    pub plan_fingerprint: String,
    pub permanent_delete_available: bool,
    pub next_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TempReclaimApproval {
    pub candidate_fingerprint: String,
    pub approved_at_ms: u64,
    pub approved_by: String,
    pub exact_phrase: String,
}

fn hash_fields(domain: &[u8], fields: &[&[u8]]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    for field in fields {
        hasher.update(&(field.len() as u64).to_le_bytes());
        hasher.update(field);
    }
    hasher.finalize().to_hex().to_string()
}

fn candidate_fingerprint(root: &Path, artifact: &DevArtifact) -> String {
    hash_fields(
        b"disksage-temp-reclaim-candidate-v1\0",
        &[
            root.to_string_lossy().as_bytes(),
            artifact.path.as_bytes(),
            artifact.kind.as_bytes(),
            artifact.object_id.as_bytes(),
            artifact.fingerprint.as_bytes(),
            &artifact.bytes.to_le_bytes(),
        ],
    )
}

fn approval_phrase_for_fingerprint(candidate_fingerprint: &str) -> String {
    format!("MOVE GENERATED TEMP ARTIFACT {candidate_fingerprint} TO TRASH")
}

pub fn approval_phrase(candidate: &TempReclaimCandidate) -> Option<String> {
    candidate
        .eligible_for_approval
        .then(|| approval_phrase_for_fingerprint(&candidate.candidate_fingerprint))
}

fn native_temp_root() -> Result<PathBuf, String> {
    #[cfg(target_os = "macos")]
    let root = PathBuf::from("/tmp");
    #[cfg(not(target_os = "macos"))]
    let root = std::env::temp_dir();
    if !root.is_absolute() {
        return Err("temporary-root-not-absolute".into());
    }
    canonical_temp_root(&root)
}

fn canonical_temp_root(requested: &Path) -> Result<PathBuf, String> {
    if !requested.is_absolute()
        || requested
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("temporary-root-invalid".into());
    }
    let canonical = requested
        .canonicalize()
        .map_err(|_| "temporary-root-canonicalize-failed".to_string())?;
    #[cfg(target_os = "macos")]
    if requested == Path::new("/tmp") && canonical != Path::new("/private/tmp") {
        return Err("temporary-root-alias-invalid".into());
    }
    Ok(canonical)
}

fn forbidden_candidate(path: &Path) -> bool {
    let text = path.to_string_lossy().replace('\\', "/");
    text.contains("/Library/CloudStorage/")
        || text.contains("/Library/Mobile Documents/")
        || text.contains(".photoslibrary/")
        || text.ends_with(".photoslibrary")
}

/// Native temporary-root cleanup intentionally accepts only development artifact kinds whose
/// normal inspector requires an adjacent project marker. Marker-free cache names remain useful in
/// ordinary development scans but are insufficient authority inside a shared temporary root.
fn marker_bound_temp_artifact(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some("node_modules" | "target" | ".venv" | "venv")
    )
}

fn remaining_discovery_timeout_ms(started: Instant) -> Option<u64> {
    let remaining = DISCOVERY_BUDGET.checked_sub(started.elapsed())?;
    let millis = remaining.as_millis();
    (millis > 0).then(|| u64::try_from(millis).unwrap_or(u64::MAX))
}

fn plan_with_active<F>(
    requested: &Path,
    observed_at_ms: u64,
    active: F,
) -> Result<TempReclaimPlan, String>
where
    F: Fn(&Path, u64) -> GitWorktreeActiveUseEvidence,
{
    let root = canonical_temp_root(requested)?;
    let started = Instant::now();
    let mut visited = 0usize;
    let mut unavailable = 0usize;
    let mut complete = true;
    let mut candidates = Vec::new();
    let children =
        std::fs::read_dir(&root).map_err(|_| "temporary-root-read-failed".to_string())?;
    'outer: for child in children {
        if started.elapsed() >= DISCOVERY_BUDGET || visited >= MAX_DISCOVERY_ENTRIES {
            complete = false;
            break;
        }
        visited += 1;
        let Ok(child) = child else {
            unavailable += 1;
            complete = false;
            continue;
        };
        let Ok(kind) = child.file_type() else {
            unavailable += 1;
            complete = false;
            continue;
        };
        if kind.is_symlink() || !kind.is_dir() {
            unavailable += 1;
            continue;
        }
        let project = child.path();
        let Ok(project_root) = project.canonicalize() else {
            unavailable += 1;
            complete = false;
            continue;
        };
        if project_root.parent() != Some(root.as_path()) || forbidden_candidate(&project_root) {
            unavailable += 1;
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&project_root) else {
            unavailable += 1;
            complete = false;
            continue;
        };
        for entry in entries {
            if started.elapsed() >= DISCOVERY_BUDGET || visited >= MAX_DISCOVERY_ENTRIES {
                complete = false;
                break 'outer;
            }
            visited += 1;
            let Ok(entry) = entry else {
                unavailable += 1;
                complete = false;
                continue;
            };
            let Ok(kind) = entry.file_type() else {
                unavailable += 1;
                complete = false;
                continue;
            };
            if kind.is_symlink() || !kind.is_dir() {
                unavailable += 1;
                continue;
            }
            let path = entry.path();
            if !marker_bound_temp_artifact(&path) {
                unavailable += 1;
                continue;
            }
            let Some(artifact) = inspect_artifact(&path, observed_at_ms) else {
                unavailable += 1;
                continue;
            };
            let canonical = match path.canonicalize() {
                Ok(canonical) => canonical,
                Err(_) => {
                    unavailable += 1;
                    complete = false;
                    continue;
                }
            };
            if canonical.parent() != Some(project_root.as_path()) || forbidden_candidate(&canonical)
            {
                unavailable += 1;
                continue;
            }
            let Some(active_timeout_ms) = remaining_discovery_timeout_ms(started) else {
                complete = false;
                break 'outer;
            };
            let use_evidence = active(&canonical, active_timeout_ms);
            if started.elapsed() >= DISCOVERY_BUDGET {
                complete = false;
            }
            let mut blockers = Vec::new();
            if !artifact.scan_complete || artifact.skipped != 0 {
                blockers.push("temporary-artifact-manifest-incomplete".into());
            }
            if !use_evidence.assessed || !use_evidence.evidence_complete {
                blockers.push("temporary-artifact-active-use-incomplete".into());
            } else if use_evidence.active {
                blockers.push("temporary-artifact-active-use-detected".into());
            }
            let fingerprint = candidate_fingerprint(&root, &artifact);
            let eligible = blockers.is_empty() && complete;
            let exact_approval_phrase =
                eligible.then(|| approval_phrase_for_fingerprint(&fingerprint));
            candidates.push(TempReclaimCandidate {
                artifact,
                active_use: use_evidence,
                candidate_fingerprint: fingerprint,
                eligible_for_approval: eligible,
                exact_approval_phrase,
                blockers,
                permanent_delete_available: false,
                next_action: if eligible {
                    "검토한 생성물만 정확한 승인 문구를 직접 입력해 휴지통으로 이동하세요."
                } else {
                    "사용 중인 작업을 종료하고 전체 증거를 다시 확인하세요."
                }
                .into(),
            });
            if !complete {
                break 'outer;
            }
            if candidates.len() >= MAX_CANDIDATES {
                complete = false;
                break 'outer;
            }
        }
    }
    candidates.sort_by(|a, b| a.artifact.path.cmp(&b.artifact.path));
    if !complete {
        for candidate in &mut candidates {
            candidate.eligible_for_approval = false;
            candidate.exact_approval_phrase = None;
            if !candidate
                .blockers
                .iter()
                .any(|value| value == "temporary-discovery-incomplete")
            {
                candidate
                    .blockers
                    .push("temporary-discovery-incomplete".into());
            }
            candidate.next_action =
                "전체 임시 공간 확인이 끝나지 않았습니다. 실행하지 말고 다시 확인하세요.".into();
        }
    }
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-temp-reclaim-plan-v1\0");
    hasher.update(root.to_string_lossy().as_bytes());
    hasher.update(&observed_at_ms.to_le_bytes());
    hasher.update(&[complete as u8]);
    for candidate in &candidates {
        hasher.update(candidate.candidate_fingerprint.as_bytes());
    }
    Ok(TempReclaimPlan {
        schema_version: 1,
        schema_kind: "disksage.temp-reclaim-plan",
        requested_root: requested.to_string_lossy().into_owned(),
        canonical_root: root.to_string_lossy().into_owned(),
        observed_at_ms,
        scan_complete: complete,
        visited_entries: visited,
        unavailable_entries: unavailable,
        candidates,
        plan_fingerprint: hasher.finalize().to_hex().to_string(),
        permanent_delete_available: false,
        next_action: "안전 판정된 생성물만 선택해 휴지통 이동을 승인하세요. 알 수 없는 임시 항목은 그대로 둡니다.".into(),
    })
}

pub fn plan_native_temp_reclaim(observed_at_ms: u64) -> Result<TempReclaimPlan, String> {
    #[cfg(not(unix))]
    {
        let _ = observed_at_ms;
        return Err("temporary-reclaim-platform-unsupported".into());
    }
    #[cfg(unix)]
    {
        let root = native_temp_root()?;
        plan_with_active(&root, observed_at_ms, |path, timeout_ms| {
            crate::git_worktree::active_use_evidence(path, timeout_ms, 256, true)
        })
    }
}

pub fn execute_candidate(
    plan: &TempReclaimPlan,
    candidate_fingerprint: &str,
    approval: &TempReclaimApproval,
    journal_path: &Path,
    now_ms: u64,
) -> DevArtifactCleanResult {
    let failed = |code: &str| DevArtifactCleanResult {
        path: String::new(),
        ok: false,
        error: code.into(),
    };
    if now_ms < approval.approved_at_ms
        || now_ms - approval.approved_at_ms > MAX_APPROVAL_AGE_MS
        || approval.candidate_fingerprint != candidate_fingerprint
        || approval.approved_by.trim().is_empty()
        || approval.approved_by.chars().any(char::is_control)
    {
        return failed("temporary-reclaim-approval-invalid-or-stale");
    }
    if !plan.scan_complete {
        return failed("temporary-reclaim-discovery-incomplete");
    }
    let Some(candidate) = plan
        .candidates
        .iter()
        .find(|item| item.candidate_fingerprint == candidate_fingerprint)
    else {
        return failed("temporary-reclaim-candidate-not-in-plan");
    };
    if !candidate.eligible_for_approval
        || approval_phrase(candidate).as_deref() != Some(approval.exact_phrase.as_str())
    {
        return failed("temporary-reclaim-exact-approval-required");
    }
    let path = Path::new(&candidate.artifact.path);
    let active = crate::git_worktree::active_use_evidence(path, 30_000, 256, true);
    if !active.assessed || !active.evidence_complete || active.active {
        return failed("temporary-artifact-active-use-recheck-failed");
    }
    clean_artifact_exact(&candidate.artifact, journal_path, now_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idle() -> GitWorktreeActiveUseEvidence {
        GitWorktreeActiveUseEvidence {
            method: "fake-complete-handle-scan".into(),
            assessed: true,
            evidence_complete: true,
            active: false,
            observed_pids: Vec::new(),
            results_truncated: false,
            error: None,
        }
    }

    #[test]
    fn only_marker_bound_generated_roots_are_candidates() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        std::fs::create_dir_all(project.join("target")).unwrap();
        std::fs::write(project.join("Cargo.toml"), b"[package]").unwrap();
        std::fs::write(project.join("target/output"), b"generated").unwrap();
        std::fs::create_dir_all(temp.path().join("unknown/private-data")).unwrap();
        let plan = plan_with_active(temp.path(), 10, |_, _| idle()).unwrap();
        assert_eq!(plan.candidates.len(), 1);
        assert_eq!(plan.candidates[0].artifact.kind, "target");
        assert!(plan.candidates[0].eligible_for_approval);
        assert_eq!(
            plan.candidates[0].exact_approval_phrase,
            approval_phrase(&plan.candidates[0])
        );
        assert!(!plan.permanent_delete_available);
        assert!(plan.unavailable_entries > 0);
    }

    #[test]
    fn wrong_exact_phrase_cannot_reach_trash_boundary() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        std::fs::create_dir_all(project.join("target")).unwrap();
        std::fs::write(project.join("Cargo.toml"), b"[package]").unwrap();
        std::fs::write(project.join("target/output"), b"generated").unwrap();
        let plan = plan_with_active(temp.path(), 10, |_, _| idle()).unwrap();
        let candidate = &plan.candidates[0];
        let result = execute_candidate(
            &plan,
            &candidate.candidate_fingerprint,
            &TempReclaimApproval {
                candidate_fingerprint: candidate.candidate_fingerprint.clone(),
                approved_at_ms: 10,
                approved_by: "local:test-user".into(),
                exact_phrase: "not the backend phrase".into(),
            },
            &temp.path().join("journal.jsonl"),
            11,
        );
        assert!(!result.ok);
        assert_eq!(result.error, "temporary-reclaim-exact-approval-required");
        assert!(project.join("target/output").is_file());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_tmp_alias_resolves_only_to_private_tmp() {
        assert_eq!(
            canonical_temp_root(Path::new("/tmp")).unwrap(),
            Path::new("/private/tmp")
        );
    }
}
