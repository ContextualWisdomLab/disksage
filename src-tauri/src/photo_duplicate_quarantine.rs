//! Adapter from exact decoded-photo groups to the shared reversible quarantine engine.

use crate::photo_duplicate::{audit_photos, ExactPhotoGroup, PhotoDuplicateAudit};
use crate::photo_similarity_audit::{
    execute_photo_quarantine_from_fresh_report, plan_photo_quarantine, PhotoQualityEvidence,
    PhotoQuarantinePlan, PhotoQuarantineReceipt, PhotoQuarantineSelection,
    PhotoSimilarityAuditReport, PhotoSimilarityGroup, PhotoSimilarityMember,
    PHOTO_SIMILARITY_AUDIT_VERSION,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use tauri::Manager;

fn exact_report(
    source_root: &Path,
    audit: &PhotoDuplicateAudit,
    customer_selections: &[PhotoQuarantineSelection],
) -> Result<(PhotoSimilarityAuditReport, Vec<PhotoQuarantineSelection>), String> {
    if !audit.evidence_complete
        || audit.exact_groups.is_empty()
        || audit.filesystem_mutation_executed
        || audit.permanent_delete_available
        || audit.perceptual_grouping_available
    {
        return Err("photo-exact-quarantine-evidence-incomplete".into());
    }
    let canonical_root = std::fs::canonicalize(source_root)
        .map_err(|_| "photo-exact-quarantine-root-unavailable".to_string())?;
    let supplied = customer_selections
        .iter()
        .map(|selection| (selection.group_fingerprint.as_str(), selection))
        .collect::<BTreeMap<_, _>>();
    if supplied.len() != customer_selections.len() {
        return Err("photo-exact-quarantine-selection-set-invalid".into());
    }
    let mut consumed = BTreeSet::new();
    let mut selections = Vec::with_capacity(audit.exact_groups.len());
    let mut groups = Vec::with_capacity(audit.exact_groups.len());
    for group in &audit.exact_groups {
        let selection = selection_for_group(&canonical_root, group, &supplied)?;
        if supplied.contains_key(group.content_digest.as_str()) {
            consumed.insert(group.content_digest.as_str());
        }
        selections.push(selection.clone());
        groups.push(map_group(&canonical_root, group)?);
    }
    if consumed.len() != supplied.len() {
        return Err("photo-exact-quarantine-selection-set-invalid".into());
    }
    Ok((
        PhotoSimilarityAuditReport {
            schema_version: PHOTO_SIMILARITY_AUDIT_VERSION,
            observed_at_ms: audit.generated_at_ms,
            source_root: canonical_root.to_string_lossy().into_owned(),
            max_entries: audit.inspected_input_count as usize,
            entries_seen: audit.inspected_input_count as usize,
            decoded_photo_count: audit.inspected_input_count as usize,
            group_count: groups.len(),
            evidence_complete: true,
            managed_library_excluded_count: 0,
            dataless_photo_excluded_count: 0,
            issue_counts: BTreeMap::new(),
            perceptual_algorithm: "none-exact-decoded-rgba16".into(),
            grouping_policy: "exact decoded RGBA16 digest only".into(),
            survivor_policy: "unique Pareto keeper or explicit customer selection".into(),
            automatic_delete_allowed: false,
            mutation_performed: false,
            audit_fingerprint: audit.audit_fingerprint.clone(),
            groups,
        },
        selections,
    ))
}

fn selection_for_group(
    root: &Path,
    group: &ExactPhotoGroup,
    supplied: &BTreeMap<&str, &PhotoQuarantineSelection>,
) -> Result<PhotoQuarantineSelection, String> {
    let survivor_relative = if let Some(keeper) = group.keeper_path.as_deref() {
        std::fs::canonicalize(keeper)
            .map_err(|_| "photo-exact-quarantine-member-unavailable".to_string())?
            .strip_prefix(root)
            .map_err(|_| "photo-exact-quarantine-member-outside-root".to_string())?
            .to_string_lossy()
            .into_owned()
    } else {
        supplied
            .get(group.content_digest.as_str())
            .map(|selection| selection.survivor_relative_path.clone())
            .ok_or_else(|| "photo-exact-quarantine-customer-selection-required".to_string())?
    };
    if !group.members.iter().any(|member| {
        std::fs::canonicalize(&member.path)
            .ok()
            .and_then(|path| path.strip_prefix(root).ok().map(PathBuf::from))
            .is_some_and(|relative| relative == Path::new(&survivor_relative))
    }) {
        return Err("photo-exact-quarantine-survivor-not-member".into());
    }
    Ok(PhotoQuarantineSelection {
        group_fingerprint: group.content_digest.clone(),
        survivor_relative_path: survivor_relative,
    })
}

fn map_group(root: &Path, group: &ExactPhotoGroup) -> Result<PhotoSimilarityGroup, String> {
    let mut members = Vec::with_capacity(group.members.len());
    for member in &group.members {
        let canonical_path = std::fs::canonicalize(&member.path)
            .map_err(|_| "photo-exact-quarantine-member-unavailable".to_string())?;
        let path = canonical_path.as_path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| "photo-exact-quarantine-member-outside-root".to_string())?;
        if relative.as_os_str().is_empty()
            || relative.components().any(|part| {
                matches!(
                    part,
                    std::path::Component::ParentDir | std::path::Component::RootDir
                )
            })
        {
            return Err("photo-exact-quarantine-member-path-invalid".into());
        }
        members.push(PhotoSimilarityMember {
            member_fingerprint: member.object_id.clone(),
            relative_path: relative.to_string_lossy().into_owned(),
            content_blake3: member.blake3.clone(),
            perceptual_hash: member.decoded_pixel_digest.clone(),
            aspect_ratio: format!("{}:{}", member.width, member.height),
            quality: PhotoQualityEvidence {
                width_pixels: member.width,
                height_pixels: member.height,
                pixel_count: u64::from(member.width).saturating_mul(u64::from(member.height)),
                bits_per_sample: member.bit_depth,
                encoded_format: member.codec.clone(),
                lossless_encoding: Some(member.codec_lossless),
                encoded_bytes: member.bytes,
            },
            filesystem_modified_ms: member.filesystem_modified_ms,
            filesystem_object_id: member.object_id.clone(),
        });
    }
    Ok(PhotoSimilarityGroup {
        group_fingerprint: group.content_digest.clone(),
        perceptual_hash: group.content_digest.clone(),
        aspect_ratio: members
            .first()
            .map(|member| member.aspect_ratio.clone())
            .unwrap_or_default(),
        max_pairwise_hamming_distance: 0,
        members,
        pareto_dominant_survivor: group
            .keeper_path
            .as_deref()
            .and_then(|path| std::fs::canonicalize(path).ok())
            .and_then(|path| path.strip_prefix(root).ok().map(PathBuf::from))
            .map(|path| path.to_string_lossy().into_owned()),
        survivor_rationale: "exact decoded group; preserve unique Pareto keeper".into(),
        requires_human_survivor_selection: group.keeper_path.is_none(),
        automatic_delete_allowed: false,
    })
}

/// Build a shared reversible quarantine plan from exact decoded duplicate evidence.
pub fn plan_exact_photo_quarantine(
    source_root: &Path,
    audit: &PhotoDuplicateAudit,
    customer_selections: &[PhotoQuarantineSelection],
) -> Result<PhotoQuarantinePlan, String> {
    let paths = audit_paths(audit);
    let fresh = audit_photos(&paths, audit.generated_at_ms);
    if !fresh.evidence_complete
        || fresh.audit_fingerprint != audit.audit_fingerprint
        || fresh.exact_groups != audit.exact_groups
    {
        return Err("photo-exact-quarantine-audit-stale-or-forged".into());
    }
    let (report, selections) = exact_report(source_root, audit, customer_selections)?;
    plan_photo_quarantine(&report, &selections)
}

fn audit_paths(audit: &PhotoDuplicateAudit) -> Vec<PathBuf> {
    audit
        .exact_groups
        .iter()
        .flat_map(|group| {
            group
                .members
                .iter()
                .map(|member| PathBuf::from(&member.path))
        })
        .collect()
}

/// Re-audit every participant and delegate Trash, journal, and receipt handling to the shared engine.
#[cfg(not(coverage))]
pub fn execute_exact_photo_quarantine(
    source_root: &Path,
    reviewed_audit: &PhotoDuplicateAudit,
    plan: &PhotoQuarantinePlan,
    approval_phrase: &str,
    rationale: &str,
    journal_path: &Path,
    executed_at_ms: u64,
) -> Result<PhotoQuarantineReceipt, String> {
    let (reviewed, selections) = exact_report(source_root, reviewed_audit, &plan.selections)?;
    if selections != plan.selections {
        return Err("photo-exact-quarantine-plan-selection-invalid".into());
    }
    let paths = audit_paths(reviewed_audit);
    let fresh_audit = audit_photos(&paths, reviewed_audit.generated_at_ms);
    let (fresh, _) = exact_report(source_root, &fresh_audit, &plan.selections)?;
    execute_photo_quarantine_from_fresh_report(
        source_root,
        &reviewed,
        &fresh,
        plan,
        approval_phrase,
        rationale,
        journal_path,
        executed_at_ms,
    )
}

/// Tauri boundary for a read-only exact-duplicate quarantine plan.
#[cfg(not(coverage))]
#[tauri::command]
pub fn plan_exact_photo_duplicate_quarantine(
    source_root: String,
    audit: PhotoDuplicateAudit,
    selections: Vec<PhotoQuarantineSelection>,
) -> Result<PhotoQuarantinePlan, String> {
    plan_exact_photo_quarantine(Path::new(&source_root), &audit, &selections)
}

/// Decode the supplied byte-duplicate candidates without mutating them.
#[cfg(not(coverage))]
#[tauri::command]
pub async fn audit_exact_photo_duplicates(
    paths: Vec<String>,
) -> Result<PhotoDuplicateAudit, String> {
    let generated_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "photo-exact-audit-clock-unavailable".to_string())?
        .as_millis() as u64;
    let paths = paths.into_iter().map(PathBuf::from).collect::<Vec<_>>();
    tauri::async_runtime::spawn_blocking(move || Ok(audit_photos(&paths, generated_at_ms)))
        .await
        .map_err(|_| "photo-exact-audit-worker-unavailable".to_string())?
}

/// Tauri boundary for the shared reversible quarantine executor.
#[cfg(not(coverage))]
#[tauri::command]
pub async fn execute_exact_photo_duplicate_quarantine(
    app: tauri::AppHandle,
    source_root: String,
    audit: PhotoDuplicateAudit,
    plan: PhotoQuarantinePlan,
    approval_phrase: String,
    rationale: String,
    executed_at_ms: u64,
) -> Result<PhotoQuarantineReceipt, String> {
    let journal_path = app
        .path()
        .app_data_dir()
        .map_err(|_| "photo-exact-quarantine-journal-unavailable".to_string())?
        .join("photo-quarantine.jsonl");
    tauri::async_runtime::spawn_blocking(move || {
        execute_exact_photo_quarantine(
            Path::new(&source_root),
            &audit,
            &plan,
            &approval_phrase,
            &rationale,
            &journal_path,
            executed_at_ms,
        )
    })
    .await
    .map_err(|_| "photo-exact-quarantine-worker-unavailable".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(path: &Path, value: u8, text: &str) {
        let file = std::fs::File::create(path).unwrap();
        let mut encoder = png::Encoder::new(file, 8, 8);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .add_text_chunk("Description".into(), text.into())
            .unwrap();
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[value; 64]).unwrap();
    }

    #[test]
    fn ties_require_selection_before_a_plan_exists() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first.png");
        let second = root.path().join("second.png");
        png(&first, 42, "one");
        png(&second, 42, "two");
        let audit = audit_photos(&[first, second], 1);
        assert_eq!(
            plan_exact_photo_quarantine(root.path(), &audit, &[]).unwrap_err(),
            "photo-exact-quarantine-customer-selection-required"
        );
    }

    #[test]
    fn customer_selection_produces_shared_reversible_plan() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first.png");
        let second = root.path().join("second.png");
        png(&first, 42, "one");
        png(&second, 42, "two");
        let audit = audit_photos(&[first, second], 7);
        let group = &audit.exact_groups[0];
        let selection = PhotoQuarantineSelection {
            group_fingerprint: group.content_digest.clone(),
            survivor_relative_path: "first.png".into(),
        };
        let plan = plan_exact_photo_quarantine(root.path(), &audit, &[selection]).unwrap();
        assert_eq!(plan.selections[0].survivor_relative_path, "first.png");
        assert!(plan.exact_approval_phrase.contains(&plan.plan_fingerprint));
    }

    #[test]
    fn forged_audit_is_rejected_before_planning() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first.png");
        let second = root.path().join("second.png");
        png(&first, 42, "one");
        png(&second, 42, "two");
        let mut audit = audit_photos(&[first, second], 7);
        audit.audit_fingerprint = "0".repeat(64);
        assert_eq!(
            plan_exact_photo_quarantine(root.path(), &audit, &[]).unwrap_err(),
            "photo-exact-quarantine-audit-stale-or-forged"
        );
    }

    #[test]
    fn unrelated_inspected_photo_does_not_invalidate_duplicate_authority() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first.png");
        let second = root.path().join("second.png");
        let unique = root.path().join("unique.png");
        png(&first, 42, "one");
        png(&second, 42, "two");
        png(&unique, 7, "unique");
        let audit = audit_photos(&[first, second, unique], 7);
        let group = &audit.exact_groups[0];
        let selection = PhotoQuarantineSelection {
            group_fingerprint: group.content_digest.clone(),
            survivor_relative_path: "first.png".into(),
        };
        assert!(plan_exact_photo_quarantine(root.path(), &audit, &[selection]).is_ok());
    }
}
