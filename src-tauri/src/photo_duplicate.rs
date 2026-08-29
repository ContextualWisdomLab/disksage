//! Evidence-separated photo duplicate audit.
//!
//! Exact equality uses BLAKE3 over current bytes. Perceptual grouping and no-reference IQA stay
//! unavailable until a versioned, checksummed calibration/model artifact is shipped; metadata
//! dimensions are never combined into an invented score.

use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_IMAGE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_IMAGE_DIMENSION: u32 = 16_384;
const MAX_NORMALIZED_IMAGE_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
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
    pub decoded_pixel_digest: String,
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
    pub grouping_basis: String,
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
    pub inspected_input_count: u64,
    pub rejected_input_counts: std::collections::BTreeMap<String, u64>,
    pub evidence_complete: bool,
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

fn read_png_evidence(bytes: &[u8]) -> Result<(u32, u32, u8, u32, String), String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder
        .read_info()
        .map_err(|_| "photo-codec-unsupported".to_string())?;
    let info = reader.info();
    if info.is_animated() {
        return Err("photo-animation-unsupported".into());
    }
    let width = info.width;
    let height = info.height;
    let normalized_bytes = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(8))
        .ok_or_else(|| "photo-decoded-size-unsupported".to_string())?;
    if width == 0
        || height == 0
        || width > MAX_IMAGE_DIMENSION
        || height > MAX_IMAGE_DIMENSION
        || normalized_bytes > MAX_NORMALIZED_IMAGE_BYTES
    {
        return Err("photo-decoded-size-unsupported".into());
    }
    let output_buffer_size = reader
        .output_buffer_size()
        .ok_or("photo-decoded-size-unavailable")?;
    if u64::try_from(output_buffer_size).unwrap_or(u64::MAX) > MAX_NORMALIZED_IMAGE_BYTES {
        return Err("photo-decoded-size-unsupported".into());
    }
    let metadata_field_count = 3
        + u32::from(info.exif_metadata.is_some())
        + u32::from(info.icc_profile.is_some())
        + u32::from(info.pixel_dims.is_some())
        + u32::from(info.source_gamma.is_some())
        + u32::from(info.source_chromaticities.is_some())
        + info.uncompressed_latin1_text.len() as u32
        + info.compressed_latin1_text.len() as u32
        + info.utf8_text.len() as u32;
    let bit_depth = match info.bit_depth {
        png::BitDepth::One => 1,
        png::BitDepth::Two => 2,
        png::BitDepth::Four => 4,
        png::BitDepth::Eight => 8,
        png::BitDepth::Sixteen => 16,
    };
    let mut decoded = vec![0; output_buffer_size];
    let output = reader
        .next_frame(&mut decoded)
        .map_err(|_| "photo-decode-failed".to_string())?;
    decoded.truncate(output.buffer_size());
    let normalized = normalize_rgba16(&decoded, output.color_type, output.bit_depth)?;
    let mut semantic = blake3::Hasher::new();
    semantic.update(b"disksage-png-rgba16-v1\0");
    semantic.update(&width.to_be_bytes());
    semantic.update(&height.to_be_bytes());
    semantic.update(&normalized);
    Ok((
        width,
        height,
        bit_depth,
        metadata_field_count,
        semantic.finalize().to_hex().to_string(),
    ))
}

fn normalize_rgba16(
    bytes: &[u8],
    color: png::ColorType,
    depth: png::BitDepth,
) -> Result<Vec<u8>, String> {
    if !matches!(depth, png::BitDepth::Eight | png::BitDepth::Sixteen) {
        return Err("photo-normalized-depth-unsupported".into());
    }
    let channels = match color {
        png::ColorType::Grayscale => 1usize,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err("photo-indexed-expansion-incomplete".into()),
    };
    let sample_bytes = usize::from(depth == png::BitDepth::Sixteen) + 1;
    let pixel_stride = channels
        .checked_mul(sample_bytes)
        .ok_or_else(|| "photo-decoded-size-unsupported".to_string())?;
    if bytes.len() % pixel_stride != 0 {
        return Err("photo-decoded-buffer-invalid".into());
    }
    let pixel_count = bytes.len() / pixel_stride;
    let normalized_capacity = pixel_count
        .checked_mul(8)
        .ok_or_else(|| "photo-decoded-size-unsupported".to_string())?;
    if u64::try_from(normalized_capacity).unwrap_or(u64::MAX) > MAX_NORMALIZED_IMAGE_BYTES {
        return Err("photo-decoded-size-unsupported".into());
    }
    let sample = |pixel: &[u8], channel: usize| -> u16 {
        let offset = channel * sample_bytes;
        if sample_bytes == 1 {
            u16::from(pixel[offset]) * 257
        } else {
            u16::from_be_bytes([pixel[offset], pixel[offset + 1]])
        }
    };
    let mut normalized = Vec::with_capacity(normalized_capacity);
    for pixel in bytes.chunks_exact(pixel_stride) {
        let values = match color {
            png::ColorType::Grayscale => {
                let gray = sample(pixel, 0);
                [gray, gray, gray, u16::MAX]
            }
            png::ColorType::GrayscaleAlpha => {
                let gray = sample(pixel, 0);
                [gray, gray, gray, sample(pixel, 1)]
            }
            png::ColorType::Rgb => [
                sample(pixel, 0),
                sample(pixel, 1),
                sample(pixel, 2),
                u16::MAX,
            ],
            png::ColorType::Rgba => [
                sample(pixel, 0),
                sample(pixel, 1),
                sample(pixel, 2),
                sample(pixel, 3),
            ],
            png::ColorType::Indexed => unreachable!(),
        };
        for value in values {
            normalized.extend_from_slice(&value.to_be_bytes());
        }
    }
    Ok(normalized)
}

fn hash_current_file(
    path: &Path,
    expected: &std::fs::Metadata,
    expected_identity: &str,
) -> Result<(String, Vec<u8>), String> {
    let mut file = std::fs::File::open(path).map_err(|_| "photo-input-open-failed".to_string())?;
    let opened = file
        .metadata()
        .map_err(|_| "photo-input-metadata-unavailable".to_string())?;
    let opened_identity = crate::safety::object_id_from_metadata(&opened);
    if opened.len() != expected.len()
        || opened.modified().ok() != expected.modified().ok()
        || !metadata_identity_matches(opened_identity.as_deref(), expected_identity)
    {
        return Err("photo-input-changed".into());
    }
    let mut hasher = blake3::Hasher::new();
    let mut bytes = Vec::with_capacity(expected.len() as usize);
    file.read_to_end(&mut bytes)
        .map_err(|_| "photo-input-read-failed".to_string())?;
    if bytes.len() as u64 != expected.len() {
        return Err("photo-input-changed".into());
    }
    hasher.update(&bytes);
    let after = std::fs::symlink_metadata(path).map_err(|_| "photo-input-changed".to_string())?;
    if after.len() != expected.len()
        || after.modified().ok() != expected.modified().ok()
        || crate::safety::filesystem_object_id(path).ok().as_deref() != Some(expected_identity)
    {
        return Err("photo-input-changed".into());
    }
    Ok((hasher.finalize().to_hex().to_string(), bytes))
}

/// Compare handle metadata identity when the platform exposes it.
///
/// Windows path identity remains bound by `filesystem_object_id` before and after the read; its
/// standard metadata object does not expose the same identity and therefore contributes no
/// contradictory value here.
fn metadata_identity_matches(observed: Option<&str>, expected: &str) -> bool {
    observed.is_none_or(|identity| identity == expected)
}

pub fn inspect_photo(path: &Path) -> Result<PhotoEvidence, String> {
    if path.to_str().is_none() {
        return Err("photo-input-path-encoding-unsupported".into());
    }
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "photo-input-metadata-unavailable".to_string())?;
    if let Some(blocker) = admission_blocker(path, &metadata) {
        return Err(blocker.into());
    }
    let identity = crate::safety::filesystem_object_id(path)
        .map_err(|_| "photo-input-identity-unavailable".to_string())?;
    // Audit is read-only. Active-use evidence belongs to the fresh execution preflight; requiring
    // Unix `lsof` here made otherwise valid Windows evidence impossible to collect.
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !extension.eq_ignore_ascii_case("png") {
        return Err("photo-codec-unsupported".into());
    }
    let (blake3, current_bytes) = hash_current_file(path, &metadata, &identity)?;
    let (width, height, bit_depth, metadata_field_count, decoded_pixel_digest) =
        read_png_evidence(&current_bytes)?;
    let final_metadata = std::fs::symlink_metadata(path).ok();
    if crate::safety::filesystem_object_id(path).ok().as_deref() != Some(identity.as_str())
        || final_metadata.as_ref().map(std::fs::Metadata::len) != Some(metadata.len())
        || final_metadata
            .as_ref()
            .and_then(|value| value.modified().ok())
            != metadata.modified().ok()
    {
        return Err("photo-input-changed".into());
    }
    Ok(PhotoEvidence {
        path: path.to_string_lossy().into_owned(),
        object_id: identity,
        bytes: metadata.len(),
        blake3,
        decoded_pixel_digest,
        width,
        height,
        bit_depth,
        codec: "png".into(),
        codec_lossless: true,
        metadata_field_count,
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
    let mut rejected_input_counts = std::collections::BTreeMap::<String, u64>::new();
    let mut inspected_input_count = 0_u64;
    for path in paths {
        match inspect_photo(path) {
            Ok(evidence) => {
                inspected_input_count += 1;
                by_digest
                    .entry(evidence.decoded_pixel_digest.clone())
                    .or_default()
                    .push(evidence);
            }
            Err(reason) => *rejected_input_counts.entry(reason).or_default() += 1,
        }
    }
    let exact_groups = by_digest
        .into_iter()
        .filter_map(|(content_digest, mut members)| {
            members.sort_by(|left, right| left.path.cmp(&right.path));
            let mut seen_object_ids = std::collections::BTreeSet::new();
            members.retain(|member| seen_object_ids.insert(member.object_id.clone()));
            if members.len() < 2 {
                return None;
            }
            let keeper_path = unique_pareto_keeper(&members).map(|member| member.path.clone());
            Some(ExactPhotoGroup {
                content_digest,
                grouping_basis: "decoded-pixel-rgba16-exact".into(),
                members,
                keeper_path: keeper_path.clone(),
                keeper_blocker: keeper_path
                    .is_none()
                    .then(|| "photo-quality-evidence-requires-customer-selection".into()),
                execution_available: false,
            })
        })
        .collect();
    PhotoDuplicateAudit {
        schema_kind: "disksage.photo-duplicate-audit.v1".into(),
        generated_at_ms,
        exact_groups,
        inspected_input_count,
        evidence_complete: rejected_input_counts.is_empty(),
        rejected_input_counts,
        perceptual_grouping_available: false,
        perceptual_grouping_blocker: "photo-perceptual-calibration-unavailable".into(),
        permanent_delete_available: false,
        filesystem_mutation_executed: false,
    }
}

fn unique_pareto_keeper(members: &[PhotoEvidence]) -> Option<&PhotoEvidence> {
    let dominates = |left: &PhotoEvidence, right: &PhotoEvidence| {
        let no_worse = left.codec_lossless >= right.codec_lossless
            && left.bit_depth >= right.bit_depth
            && left.metadata_field_count >= right.metadata_field_count
            && left.original_edit_lineage >= right.original_edit_lineage;
        let better = left.codec_lossless > right.codec_lossless
            || left.bit_depth > right.bit_depth
            || left.metadata_field_count > right.metadata_field_count
            || left.original_edit_lineage > right.original_edit_lineage;
        no_worse && better
    };
    let candidates: Vec<_> = members
        .iter()
        .filter(|candidate| {
            members
                .iter()
                .all(|other| std::ptr::eq(*candidate, other) || dominates(candidate, other))
        })
        .collect();
    if candidates.len() == 1 {
        Some(candidates[0])
    } else {
        None
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

    fn png_with_text(path: &Path, width: u32, height: u32, value: u8, text: Option<&str>) {
        let file = std::fs::File::create(path).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        if let Some(text) = text {
            encoder
                .add_text_chunk("Description".into(), text.into())
                .unwrap();
        }
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&vec![value; (width * height) as usize])
            .unwrap();
    }

    fn png_16(path: &Path, width: u32, height: u32, value: u8) {
        let file = std::fs::File::create(path).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Sixteen);
        let sample = (u16::from(value) * 257).to_be_bytes();
        let bytes: Vec<_> = std::iter::repeat_n(sample, (width * height) as usize)
            .flatten()
            .collect();
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&bytes).unwrap();
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
    fn same_decoded_pixels_group_across_metadata_and_select_pareto_keeper() {
        let temp = tempfile::tempdir().unwrap();
        let plain = temp.path().join("plain.png");
        let documented = temp.path().join("documented.png");
        png_with_text(&plain, 12, 9, 80, None);
        png_with_text(&documented, 12, 9, 80, Some("export provenance"));
        let audit = audit_photos(&[plain, documented.clone()], 7);
        assert_eq!(audit.exact_groups.len(), 1);
        assert_eq!(
            audit.exact_groups[0].grouping_basis,
            "decoded-pixel-rgba16-exact"
        );
        assert_eq!(
            audit.exact_groups[0].keeper_path.as_deref(),
            Some(documented.to_string_lossy().as_ref())
        );
        assert!(!audit.exact_groups[0].execution_available);
    }

    #[test]
    fn higher_bit_depth_is_the_unique_pareto_keeper_for_identical_samples() {
        let temp = tempfile::tempdir().unwrap();
        let eight = temp.path().join("eight.png");
        let sixteen = temp.path().join("sixteen.png");
        png(&eight, 10, 8, 42);
        png_16(&sixteen, 10, 8, 42);
        let audit = audit_photos(&[eight, sixteen.clone()], 7);
        assert_eq!(audit.exact_groups.len(), 1);
        assert_eq!(
            audit.exact_groups[0].keeper_path.as_deref(),
            Some(sixteen.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn equal_metadata_evidence_keeps_customer_selection_required() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first.png");
        let second = temp.path().join("second.png");
        png_with_text(&first, 10, 8, 42, Some("one"));
        png_with_text(&second, 10, 8, 42, Some("two"));
        let audit = audit_photos(&[first, second], 7);
        assert_eq!(audit.exact_groups.len(), 1);
        assert!(audit.exact_groups[0].keeper_path.is_none());
        assert_eq!(
            audit.exact_groups[0].keeper_blocker.as_deref(),
            Some("photo-quality-evidence-requires-customer-selection")
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

    #[test]
    fn rejected_inputs_are_reported_and_make_evidence_incomplete() {
        let audit = audit_photos(&[PathBuf::from("/missing/photo.png")], 1);
        assert_eq!(audit.inspected_input_count, 0);
        assert!(!audit.evidence_complete);
        assert_eq!(
            audit
                .rejected_input_counts
                .get("photo-input-metadata-unavailable"),
            Some(&1)
        );
    }

    #[test]
    fn repeated_path_does_not_invent_a_duplicate_group() {
        let temp = tempfile::tempdir().unwrap();
        let photo = temp.path().join("single.png");
        png(&photo, 8, 8, 4);
        let audit = audit_photos(&[photo.clone(), photo], 1);
        assert!(audit.exact_groups.is_empty());
    }

    #[test]
    fn unavailable_metadata_identity_defers_to_path_identity_rechecks() {
        assert!(metadata_identity_matches(None, "path-identity"));
        assert!(metadata_identity_matches(
            Some("path-identity"),
            "path-identity"
        ));
        assert!(!metadata_identity_matches(
            Some("replacement"),
            "path-identity"
        ));
    }
}
