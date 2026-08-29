//! Evidence-separated photo duplicate audit.
//!
//! Exact equality uses BLAKE3 over current bytes. Perceptual grouping and no-reference IQA stay
//! unavailable until a versioned, checksummed calibration/model artifact is shipped; metadata
//! dimensions are never combined into an invented score.

use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_IMAGE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceState {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PhotoEvidence {
    pub path: String,
    pub object_id: String,
    pub bytes: u64,
    pub blake3: String,
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8,
    pub codec: String,
    pub codec_lossless: bool,
    pub metadata_field_count: u32,
    pub original_edit_lineage: EvidenceState,
    pub no_reference_iqa: EvidenceState,
    pub perceptual_descriptor: EvidenceState,
    pub blockers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ExactPhotoGroup {
    pub content_digest: String,
    pub members: Vec<PhotoEvidence>,
    pub keeper_path: Option<String>,
    pub keeper_blocker: Option<String>,
    pub execution_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct PhotoDuplicateAudit {
    pub schema_kind: String,
    pub generated_at_ms: u64,
    pub exact_groups: Vec<ExactPhotoGroup>,
    pub perceptual_grouping_available: bool,
    pub perceptual_grouping_blocker: String,
    pub permanent_delete_available: bool,
    pub filesystem_mutation_executed: bool,
}

fn admission_blocker(path: &Path, metadata: &std::fs::Metadata) -> Option<&'static str> {
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Some("photo-input-not-materialized-regular-file");
    }
    if crate::cloud::path_inside_managed_file_provider_storage(path) {
        return Some("photo-input-provider-managed");
    }
    if crate::cloud::path_inside_managed_photo_library(path) {
        return Some("photo-input-managed-library");
    }
    if crate::cloud::metadata_is_dataless(metadata) {
        return Some("photo-input-dataless");
    }
    if metadata.len() == 0 || metadata.len() > MAX_IMAGE_BYTES {
        return Some("photo-input-size-unsupported");
    }
    None
}

fn read_png_evidence(path: &Path) -> Result<(u32, u32, u8), String> {
    let file = std::fs::File::open(path).map_err(|_| "photo-input-open-failed".to_string())?;
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let reader = decoder
        .read_info()
        .map_err(|_| "photo-codec-unsupported".to_string())?;
    let info = reader.info();
    let bit_depth = match info.bit_depth {
        png::BitDepth::One => 1,
        png::BitDepth::Two => 2,
        png::BitDepth::Four => 4,
        png::BitDepth::Eight => 8,
        png::BitDepth::Sixteen => 16,
    };
    Ok((info.width, info.height, bit_depth))
}

fn hash_current_file(
    path: &Path,
    expected: &std::fs::Metadata,
    expected_identity: &str,
) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|_| "photo-input-open-failed".to_string())?;
    let opened = file
        .metadata()
        .map_err(|_| "photo-input-metadata-unavailable".to_string())?;
    if opened.len() != expected.len()
        || crate::safety::object_id_from_metadata(&opened).as_deref() != Some(expected_identity)
    {
        return Err("photo-input-changed".into());
    }
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|_| "photo-input-read-failed".to_string())?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let after = std::fs::symlink_metadata(path).map_err(|_| "photo-input-changed".to_string())?;
    if after.len() != expected.len()
        || crate::safety::filesystem_object_id(path).ok().as_deref() != Some(expected_identity)
    {
        return Err("photo-input-changed".into());
    }
    Ok(hasher.finalize().to_hex().to_string())
}

pub fn inspect_photo(path: &Path) -> Result<PhotoEvidence, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "photo-input-metadata-unavailable".to_string())?;
    if let Some(blocker) = admission_blocker(path, &metadata) {
        return Err(blocker.into());
    }
    let identity = crate::safety::filesystem_object_id(path)
        .map_err(|_| "photo-input-identity-unavailable".to_string())?;
    let active_use = crate::git_worktree::active_use_evidence(path, 2_000, 64, false);
    if !active_use.assessed || !active_use.evidence_complete {
        return Err("photo-input-active-use-evidence-incomplete".into());
    }
    if active_use.active {
        return Err("photo-input-active-use-detected".into());
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !extension.eq_ignore_ascii_case("png") {
        return Err("photo-codec-unsupported".into());
    }
    let (width, height, bit_depth) = read_png_evidence(path)?;
    let blake3 = hash_current_file(path, &metadata, &identity)?;
    if crate::safety::filesystem_object_id(path).ok().as_deref() != Some(identity.as_str()) {
        return Err("photo-input-changed".into());
    }
    Ok(PhotoEvidence {
        path: path.to_string_lossy().into_owned(),
        object_id: identity,
        bytes: metadata.len(),
        blake3,
        width,
        height,
        bit_depth,
        codec: "png".into(),
        codec_lossless: true,
        metadata_field_count: 3,
        original_edit_lineage: EvidenceState::Unavailable,
        no_reference_iqa: EvidenceState::Unavailable,
        perceptual_descriptor: EvidenceState::Unavailable,
        blockers: vec![
            "photo-original-edit-lineage-unavailable".into(),
            "photo-no-reference-iqa-model-unavailable".into(),
            "photo-perceptual-calibration-unavailable".into(),
        ],
    })
}

pub fn audit_photos(paths: &[PathBuf], generated_at_ms: u64) -> PhotoDuplicateAudit {
    let mut by_digest = std::collections::BTreeMap::<String, Vec<PhotoEvidence>>::new();
    for path in paths {
        if let Ok(evidence) = inspect_photo(path) {
            by_digest
                .entry(evidence.blake3.clone())
                .or_default()
                .push(evidence);
        }
    }
    let exact_groups = by_digest
        .into_iter()
        .filter(|(_, members)| members.len() > 1)
        .map(|(content_digest, mut members)| {
            members.sort_by(|left, right| left.path.cmp(&right.path));
            ExactPhotoGroup {
                content_digest,
                members,
                keeper_path: None,
                keeper_blocker: Some(
                    "photo-quality-evidence-does-not-identify-unique-keeper".into(),
                ),
                execution_available: false,
            }
        })
        .collect();
    PhotoDuplicateAudit {
        schema_kind: "disksage.photo-duplicate-audit.v1".into(),
        generated_at_ms,
        exact_groups,
        perceptual_grouping_available: false,
        perceptual_grouping_blocker: "photo-perceptual-calibration-unavailable".into(),
        permanent_delete_available: false,
        filesystem_mutation_executed: false,
    }
}

pub fn execute_photo_duplicate_cleanup(
    _audit: &PhotoDuplicateAudit,
    _approval: &str,
) -> Result<(), String> {
    Err("photo-duplicate-execution-unavailable-without-unique-evidence-backed-keeper".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(path: &Path, width: u32, height: u32, value: u8) {
        let file = std::fs::File::create(path).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&vec![value; (width * height) as usize])
            .unwrap();
    }

    #[test]
    fn exact_duplicates_are_grouped_without_inventing_a_keeper() {
        let temp = tempfile::tempdir().unwrap();
        let original = temp.path().join("original.png");
        let copy = temp.path().join("edited.png");
        png(&original, 32, 24, 120);
        std::fs::copy(&original, &copy).unwrap();
        let audit = audit_photos(&[original, copy], 7);
        assert_eq!(audit.exact_groups.len(), 1);
        assert!(audit.exact_groups[0].keeper_path.is_none());
        assert!(!audit.exact_groups[0].execution_available);
        assert!(!audit.perceptual_grouping_available);
        assert!(!audit.permanent_delete_available);
    }

    #[test]
    fn different_quality_images_are_not_joined_without_calibration() {
        let temp = tempfile::tempdir().unwrap();
        let high = temp.path().join("high.png");
        let low = temp.path().join("low.png");
        png(&high, 64, 48, 120);
        png(&low, 16, 12, 120);
        let audit = audit_photos(&[high, low], 7);
        assert!(audit.exact_groups.is_empty());
        assert_eq!(
            audit.perceptual_grouping_blocker,
            "photo-perceptual-calibration-unavailable"
        );
    }

    #[test]
    fn provider_and_photos_library_paths_fail_closed() {
        let temp = tempfile::tempdir().unwrap();
        let provider = temp
            .path()
            .join("Library/CloudStorage/OneDrive-Personal/image.png");
        let library = temp
            .path()
            .join("Pictures/Photos Library.photoslibrary/originals/image.png");
        std::fs::create_dir_all(provider.parent().unwrap()).unwrap();
        std::fs::create_dir_all(library.parent().unwrap()).unwrap();
        png(&provider, 8, 8, 1);
        png(&library, 8, 8, 1);
        assert_eq!(
            inspect_photo(&provider).unwrap_err(),
            "photo-input-provider-managed"
        );
        assert_eq!(
            inspect_photo(&library).unwrap_err(),
            "photo-input-managed-library"
        );
    }

    #[test]
    fn execution_remains_unavailable_without_unique_keeper() {
        let audit = audit_photos(&[], 1);
        assert_eq!(
            execute_photo_duplicate_cleanup(&audit, "approved").unwrap_err(),
            "photo-duplicate-execution-unavailable-without-unique-evidence-backed-keeper"
        );
    }
}
