//! Short-lived human-review authority for development build-root cleanup.
//!
//! Inventory evidence remains owned by `dev_artifacts`. This module binds an exact selected set to
//! a backend-authored phrase and a five-minute review window before delegating to the existing
//! identity/manifest/active-use revalidation and Trash-only executor.

use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

use crate::dev_artifacts::{self, DevArtifact, DevArtifactCleanResult};

pub const MAX_REVIEW_AGE_MS: u64 = 5 * 60 * 1_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DevArtifactApproval {
    pub selection_fingerprint: String,
    pub reviewed_at_ms: u64,
    pub expires_at_ms: u64,
    pub exact_phrase: String,
}

fn hash_field(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value);
}

pub fn selection_fingerprint(root: &Path, requests: &[DevArtifact]) -> Result<String, String> {
    if requests.is_empty()
        || !root.is_absolute()
        || root
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("development-artifact-selection-invalid".into());
    }
    let canonical_root = root
        .canonicalize()
        .map_err(|_| "development-artifact-root-canonicalize-failed".to_string())?;
    let mut ordered: Vec<&DevArtifact> = requests.iter().collect();
    ordered.sort_by(|left, right| left.path.cmp(&right.path));
    if ordered.windows(2).any(|pair| pair[0].path == pair[1].path) {
        return Err("development-artifact-selection-duplicate".into());
    }

    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-dev-artifact-approval-v1\0");
    hash_field(&mut hasher, canonical_root.to_string_lossy().as_bytes());
    for artifact in ordered {
        hash_field(&mut hasher, artifact.path.as_bytes());
        hash_field(&mut hasher, artifact.kind.as_bytes());
        hash_field(&mut hasher, artifact.project.as_bytes());
        hash_field(&mut hasher, artifact.object_id.as_bytes());
        hash_field(&mut hasher, artifact.fingerprint.as_bytes());
        hash_field(&mut hasher, &artifact.bytes.to_le_bytes());
        hash_field(&mut hasher, &artifact.allocated_bytes.to_le_bytes());
        hash_field(&mut hasher, &artifact.files.to_le_bytes());
        hash_field(&mut hasher, &artifact.skipped.to_le_bytes());
        hash_field(&mut hasher, &[u8::from(artifact.scan_complete)]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn exact_phrase(selection_fingerprint: &str) -> String {
    format!("MOVE DEVELOPMENT ARTIFACTS {selection_fingerprint} TO TRASH")
}

pub fn review_selection(
    root: &Path,
    requests: &[DevArtifact],
    reviewed_at_ms: u64,
) -> Result<DevArtifactApproval, String> {
    let selection_fingerprint = selection_fingerprint(root, requests)?;
    Ok(DevArtifactApproval {
        exact_phrase: exact_phrase(&selection_fingerprint),
        selection_fingerprint,
        reviewed_at_ms,
        expires_at_ms: reviewed_at_ms.saturating_add(MAX_REVIEW_AGE_MS),
    })
}

fn review_current_selection(
    root: &Path,
    requests: &[DevArtifact],
    reviewed_at_ms: u64,
) -> Result<DevArtifactApproval, String> {
    let requested_fingerprint = selection_fingerprint(root, requests)?;
    let current = dev_artifacts::find_artifacts(root, 0, reviewed_at_ms);
    let mut refreshed = Vec::with_capacity(requests.len());
    for request in requests {
        let Some(candidate) = current.iter().find(|candidate| candidate.path == request.path) else {
            return Err("development-artifact-selection-stale".into());
        };
        refreshed.push(candidate.clone());
    }
    let refreshed_fingerprint = selection_fingerprint(root, &refreshed)?;
    if refreshed_fingerprint != requested_fingerprint {
        return Err("development-artifact-selection-stale".into());
    }
    review_selection(root, &refreshed, reviewed_at_ms)
}

fn rejection(requests: &[DevArtifact], code: &str) -> Vec<DevArtifactCleanResult> {
    requests
        .iter()
        .map(|request| DevArtifactCleanResult {
            path: request.path.clone(),
            ok: false,
            error: code.into(),
        })
        .collect()
}

pub fn clean_artifacts_with_confirmation(
    requests: &[DevArtifact],
    root: &Path,
    min_age_days: u64,
    journal_path: &Path,
    now_ms: u64,
    approval: &DevArtifactApproval,
    confirmation_phrase: &str,
) -> Vec<DevArtifactCleanResult> {
    let Ok(current_fingerprint) = selection_fingerprint(root, requests) else {
        return rejection(requests, "development-artifact-selection-invalid");
    };
    if confirmation_phrase != approval.exact_phrase {
        return rejection(requests, "development-artifact-confirmation-required");
    }
    if now_ms < approval.reviewed_at_ms
        || now_ms > approval.expires_at_ms
        || approval.expires_at_ms
            != approval.reviewed_at_ms.saturating_add(MAX_REVIEW_AGE_MS)
        || now_ms.saturating_sub(approval.reviewed_at_ms) > MAX_REVIEW_AGE_MS
        || approval.selection_fingerprint != current_fingerprint
        || approval.exact_phrase != exact_phrase(&current_fingerprint)
    {
        return rejection(requests, "development-artifact-approval-invalid-or-stale");
    }
    dev_artifacts::clean_artifacts(
        requests,
        root,
        min_age_days,
        journal_path,
        now_ms,
        true,
    )
}

/// CLI callers reconstruct `DevArtifactApproval` from a phrase the operator typed on the command
/// line, so the approval field itself remains the independent confirmation channel there.
pub fn clean_artifacts_with_approval(
    requests: &[DevArtifact],
    root: &Path,
    min_age_days: u64,
    journal_path: &Path,
    now_ms: u64,
    approval: &DevArtifactApproval,
) -> Vec<DevArtifactCleanResult> {
    clean_artifacts_with_confirmation(
        requests,
        root,
        min_age_days,
        journal_path,
        now_ms,
        approval,
        &approval.exact_phrase,
    )
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn review_dev_artifacts(
    root: String,
    artifacts: Vec<DevArtifact>,
) -> Result<DevArtifactApproval, String> {
    review_current_selection(Path::new(&root), &artifacts, crate::commands::now_ms())
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn clean_dev_artifacts_bound(
    root: String,
    min_age_days: u64,
    artifacts: Vec<DevArtifact>,
    approval: DevArtifactApproval,
    confirmation_phrase: String,
    app: tauri::AppHandle,
) -> Result<Vec<crate::commands::CleanResult>, String> {
    let journal_path = crate::commands::journal_file_path(&app)?;
    Ok(clean_artifacts_with_confirmation(
        &artifacts,
        Path::new(&root),
        min_age_days,
        &journal_path,
        crate::commands::now_ms(),
        &approval,
        &confirmation_phrase,
    )
    .into_iter()
    .map(|result| crate::commands::CleanResult {
        path: result.path,
        ok: result.ok,
        error: result.error,
    })
    .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn artifact(path: &str, fingerprint: &str) -> DevArtifact {
        DevArtifact {
            path: path.into(),
            kind: "target".into(),
            project: "fixture".into(),
            bytes: 10,
            allocated_bytes: 4096,
            files: 1,
            skipped: 0,
            scan_complete: true,
            fingerprint: fingerprint.into(),
            object_id: "dev:ino".into(),
            age_days: 0,
        }
    }

    #[test]
    fn approval_fingerprint_is_order_independent_but_evidence_bound() {
        let temp = tempfile::tempdir().unwrap();
        let a = artifact("/tmp/a/target", "a");
        let b = artifact("/tmp/b/target", "b");
        let forward = selection_fingerprint(temp.path(), &[a.clone(), b.clone()]).unwrap();
        let reverse = selection_fingerprint(temp.path(), &[b.clone(), a.clone()]).unwrap();
        assert_eq!(forward, reverse);
        let changed = selection_fingerprint(temp.path(), &[a, artifact("/tmp/b/target", "c")]).unwrap();
        assert_ne!(forward, changed);
    }

    #[test]
    fn stale_review_is_rejected_before_cleanup() {
        let temp = tempfile::tempdir().unwrap();
        let request = artifact("/tmp/a/target", "a");
        let approval = review_selection(temp.path(), std::slice::from_ref(&request), 10).unwrap();
        let result = clean_artifacts_with_approval(
            &[request],
            temp.path(),
            0,
            &temp.path().join("journal.jsonl"),
            10 + MAX_REVIEW_AGE_MS + 1,
            &approval,
        );
        assert_eq!(result[0].error, "development-artifact-approval-invalid-or-stale");
    }

    #[cfg(not(coverage))]
    #[test]
    fn review_command_rejects_inventory_changed_since_listing() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("fixture");
        let target = project.join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(project.join("Cargo.toml"), b"[package]\nname='fixture'\nversion='0.1.0'").unwrap();
        fs::write(project.join("Cargo.lock"), b"version = 4").unwrap();
        fs::write(target.join("output.bin"), b"reviewed bytes").unwrap();
        let listed = dev_artifacts::find_artifacts(temp.path(), 0, u64::MAX);
        assert_eq!(listed.len(), 1);

        fs::write(target.join("output.bin"), b"changed after listing").unwrap();

        let result = review_dev_artifacts(temp.path().to_string_lossy().into_owned(), listed);
        assert_eq!(
            result.unwrap_err(),
            "development-artifact-selection-stale"
        );
    }
}
