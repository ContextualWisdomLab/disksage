//! Append-only operator decisions for cloud candidates that require metadata review.
//!
//! Decisions contain no file paths or metadata values. They are bound to both the stable candidate
//! fingerprint and the exact review-evidence fingerprint produced by the fresh planner.

use crate::cloud::{candidate_review_fingerprint, CloudCandidate};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const DECISION_VERSION: u32 = 1;
#[cfg(not(coverage))]
const MAX_DECISION_BYTES: u64 = 8 * 1024;
#[cfg(not(coverage))]
const MAX_DECISION_FILES: usize = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CloudReviewDisposition {
    Approved,
    Held,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudReviewDecision {
    pub version: u32,
    pub decision_id: String,
    pub candidate_fingerprint: String,
    pub review_fingerprint: String,
    pub disposition: CloudReviewDisposition,
    pub reviewed_at_ms: u64,
}

fn valid_fingerprint(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn decision_id_for(
    candidate_fingerprint: &str,
    review_fingerprint: &str,
    disposition: CloudReviewDisposition,
    reviewed_at_ms: u64,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&DECISION_VERSION.to_le_bytes());
    hasher.update(candidate_fingerprint.as_bytes());
    hasher.update(&[0]);
    hasher.update(review_fingerprint.as_bytes());
    hasher.update(&[0]);
    hasher.update(match disposition {
        CloudReviewDisposition::Approved => b"approved",
        CloudReviewDisposition::Held => b"held",
    });
    hasher.update(&reviewed_at_ms.to_le_bytes());
    hasher.finalize().to_hex().to_string()
}

pub(crate) fn validate_decision(decision: &CloudReviewDecision) -> Result<(), String> {
    if decision.version != DECISION_VERSION {
        return Err("cloud-review-decision-version-unsupported".into());
    }
    if !valid_fingerprint(&decision.candidate_fingerprint)
        || !valid_fingerprint(&decision.review_fingerprint)
        || !valid_fingerprint(&decision.decision_id)
    {
        return Err("cloud-review-decision-fingerprint-invalid".into());
    }
    if decision.decision_id
        != decision_id_for(
            &decision.candidate_fingerprint,
            &decision.review_fingerprint,
            decision.disposition,
            decision.reviewed_at_ms,
        )
    {
        return Err("cloud-review-decision-integrity-mismatch".into());
    }
    Ok(())
}

pub fn create_decision(
    candidate: &CloudCandidate,
    disposition: CloudReviewDisposition,
    reviewed_at_ms: u64,
) -> Result<CloudReviewDecision, String> {
    if !candidate.requires_review {
        return Err("cloud-review-not-required".into());
    }
    if !valid_fingerprint(&candidate.metadata_fingerprint)
        || !valid_fingerprint(&candidate.review_fingerprint)
    {
        return Err("cloud-review-candidate-fingerprint-invalid".into());
    }
    if candidate.review_fingerprint != candidate_review_fingerprint(candidate) {
        return Err("cloud-review-fingerprint-mismatch".into());
    }
    let decision_id = decision_id_for(
        &candidate.metadata_fingerprint,
        &candidate.review_fingerprint,
        disposition,
        reviewed_at_ms,
    );
    Ok(CloudReviewDecision {
        version: DECISION_VERSION,
        decision_id,
        candidate_fingerprint: candidate.metadata_fingerprint.clone(),
        review_fingerprint: candidate.review_fingerprint.clone(),
        disposition,
        reviewed_at_ms,
    })
}

#[cfg(not(coverage))]
fn decision_filename(decision: &CloudReviewDecision) -> String {
    format!(
        "{}-{:020}-{}.json",
        decision.candidate_fingerprint, decision.reviewed_at_ms, decision.decision_id
    )
}

#[cfg(not(coverage))]
fn secure_decision_directory(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|_| "cloud-review-directory-create-failed")?;
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| "cloud-review-directory-metadata-failed")?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("cloud-review-directory-unsafe".into());
    }
    Ok(())
}

#[cfg(not(coverage))]
pub fn write_immutable_decision(
    directory: &Path,
    decision: &CloudReviewDecision,
) -> Result<PathBuf, String> {
    use std::io::Write;

    validate_decision(decision)?;
    secure_decision_directory(directory)?;
    let path = directory.join(decision_filename(decision));
    let encoded = serde_json::to_vec_pretty(decision)
        .map_err(|_| "cloud-review-decision-json-invalid".to_string())?;
    if encoded.len() as u64 > MAX_DECISION_BYTES {
        return Err("cloud-review-decision-too-large".into());
    }
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|_| "cloud-review-decision-create-failed".to_string())?;
    let result = (|| -> Result<(), String> {
        file.write_all(&encoded)
            .and_then(|_| file.sync_all())
            .map_err(|_| "cloud-review-decision-write-failed".to_string())?;
        let mut permissions = file
            .metadata()
            .map_err(|_| "cloud-review-decision-metadata-failed".to_string())?
            .permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions)
            .map_err(|_| "cloud-review-decision-permissions-failed".to_string())?;
        #[cfg(unix)]
        std::fs::File::open(directory)
            .and_then(|dir| dir.sync_all())
            .map_err(|_| "cloud-review-directory-sync-failed".to_string())?;
        Ok(())
    })();
    if let Err(error) = result {
        drop(file);
        let _ = std::fs::remove_file(&path);
        return Err(error);
    }
    Ok(path)
}

#[cfg(not(coverage))]
fn same_decision_file_identity(expected: &std::fs::Metadata, observed: &std::fs::Metadata) -> bool {
    let common = expected.file_type().is_file()
        && observed.file_type().is_file()
        && !expected.file_type().is_symlink()
        && !observed.file_type().is_symlink()
        && expected.len() == observed.len()
        && expected.permissions().readonly()
        && observed.permissions().readonly()
        && expected.modified().ok() == observed.modified().ok();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        common && expected.dev() == observed.dev() && expected.ino() == observed.ino()
    }
    #[cfg(not(unix))]
    {
        common
    }
}

#[cfg(not(coverage))]
fn read_immutable_decision(path: &Path) -> Result<CloudReviewDecision, String> {
    use std::io::Read;

    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "cloud-review-decision-metadata-failed".to_string())?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || !metadata.permissions().readonly()
    {
        return Err("cloud-review-decision-must-be-read-only-regular-file".into());
    }
    if metadata.len() > MAX_DECISION_BYTES {
        return Err("cloud-review-decision-too-large".into());
    }
    let mut file =
        std::fs::File::open(path).map_err(|_| "cloud-review-decision-open-failed".to_string())?;
    let opened = file
        .metadata()
        .map_err(|_| "cloud-review-decision-metadata-failed".to_string())?;
    if !same_decision_file_identity(&metadata, &opened) {
        return Err("cloud-review-decision-changed-during-read".into());
    }
    let mut encoded = Vec::with_capacity(metadata.len() as usize);
    std::io::Read::by_ref(&mut file)
        .take(MAX_DECISION_BYTES + 1)
        .read_to_end(&mut encoded)
        .map_err(|_| "cloud-review-decision-read-failed".to_string())?;
    if encoded.len() as u64 > MAX_DECISION_BYTES {
        return Err("cloud-review-decision-too-large".into());
    }
    let after = std::fs::symlink_metadata(path)
        .map_err(|_| "cloud-review-decision-metadata-failed".to_string())?;
    if !same_decision_file_identity(&metadata, &after) {
        return Err("cloud-review-decision-changed-during-read".into());
    }
    let decision: CloudReviewDecision = serde_json::from_slice(&encoded)
        .map_err(|_| "cloud-review-decision-json-invalid".to_string())?;
    validate_decision(&decision)?;
    if path.file_name().and_then(|name| name.to_str()) != Some(&decision_filename(&decision)) {
        return Err("cloud-review-decision-filename-mismatch".into());
    }
    Ok(decision)
}

#[cfg(not(coverage))]
pub fn load_latest_decisions(directory: &Path) -> Result<Vec<CloudReviewDecision>, String> {
    if !directory.exists() {
        return Ok(Vec::new());
    }
    secure_decision_directory(directory)?;
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(directory)
        .map_err(|_| "cloud-review-directory-read-failed".to_string())?
    {
        let path = entry
            .map_err(|_| "cloud-review-directory-entry-failed".to_string())?
            .path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        if paths.len() == MAX_DECISION_FILES {
            return Err("cloud-review-decision-limit-exceeded".into());
        }
        paths.push(path);
    }
    paths.sort();
    let mut latest = std::collections::BTreeMap::<String, CloudReviewDecision>::new();
    for path in paths {
        let decision = read_immutable_decision(&path)?;
        let replace = latest
            .get(&decision.candidate_fingerprint)
            .map(|current| {
                (decision.reviewed_at_ms, &decision.decision_id)
                    > (current.reviewed_at_ms, &current.decision_id)
            })
            .unwrap_or(true);
        if replace {
            latest.insert(decision.candidate_fingerprint.clone(), decision);
        }
    }
    Ok(latest.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::{ArchiveKind, CloudAccountScope, CloudProvider, MetadataEvidence};

    fn candidate() -> CloudCandidate {
        let mut candidate = CloudCandidate {
            metadata_fingerprint: "a".repeat(64),
            review_fingerprint: String::new(),
            src: "/source/report.pdf".into(),
            dst: "/cloud/report.pdf".into(),
            provider: CloudProvider::Icloud,
            destination_account_scope: CloudAccountScope::Organization,
            kind: ArchiveKind::Document,
            bytes: 12,
            age_days: 90,
            created_ms: 1,
            modified_ms: 2,
            production_time_ms: 3,
            production_time_source: "embedded:exiftool:CreateDate".into(),
            production_time_confidence: "high".into(),
            source_root: "/source".into(),
            relative_path: "report.pdf".into(),
            source_context: ".".into(),
            requires_review: true,
            review_reasons: vec!["embedded-metadata-probe-incomplete".into()],
            content_title: Some("Report".into()),
            content_authors: vec!["Author".into()],
            content_context: Vec::new(),
            duration_ms: None,
            dataset_profile: None,
            metadata_evidence: vec![MetadataEvidence {
                field: "production-date".into(),
                value: "2026-01-01".into(),
                source: "embedded:exiftool:CreateDate".into(),
                confidence: "high".into(),
            }],
            blocked_reason: None,
        };
        candidate.review_fingerprint = candidate_review_fingerprint(&candidate);
        candidate
    }

    #[test]
    fn decision_is_bound_to_exact_review_evidence() {
        let original = candidate();
        let decision = create_decision(&original, CloudReviewDisposition::Approved, 10).unwrap();
        assert_eq!(
            decision.candidate_fingerprint,
            original.metadata_fingerprint
        );
        assert_eq!(decision.review_fingerprint, original.review_fingerprint);

        let mut changed = original;
        changed.review_reasons.push("new-evidence-warning".into());
        assert_ne!(
            decision.review_fingerprint,
            candidate_review_fingerprint(&changed)
        );

        let mut changed_scope = candidate();
        changed_scope.destination_account_scope = CloudAccountScope::Personal;
        assert_ne!(
            decision.review_fingerprint,
            candidate_review_fingerprint(&changed_scope)
        );
    }

    #[cfg(not(coverage))]
    #[test]
    fn append_only_decisions_round_trip_and_latest_wins() {
        let temp = tempfile::tempdir().unwrap();
        let candidate = candidate();
        let approved = create_decision(&candidate, CloudReviewDisposition::Approved, 10).unwrap();
        let held = create_decision(&candidate, CloudReviewDisposition::Held, 11).unwrap();
        let approved_path = write_immutable_decision(temp.path(), &approved).unwrap();
        write_immutable_decision(temp.path(), &held).unwrap();
        assert!(approved_path.metadata().unwrap().permissions().readonly());
        assert_eq!(
            load_latest_decisions(temp.path()).unwrap(),
            vec![held.clone()]
        );

        let mut tampered = held;
        tampered.reviewed_at_ms = 12;
        assert_eq!(
            validate_decision(&tampered).unwrap_err(),
            "cloud-review-decision-integrity-mismatch"
        );
    }
}
