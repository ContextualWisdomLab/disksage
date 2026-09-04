//! Evidence-bound photo duplicate audit.
//!
//! The current product boundary is intentionally conservative: exact decoded-pixel grouping is
//! available for bounded local PNG inputs, while perceptual grouping and permanent cleanup remain
//! unavailable until calibrated evidence and a unique keeper decision exist.

use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_ENCODED_IMAGE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_DECODED_IMAGE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_NORMALIZED_IMAGE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_AUDIT_INPUTS: usize = 4_096;
const MAX_AUDIT_DECLARED_BYTES: u64 = 16 * 1024 * 1024 * 1024;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceState {
    Unavailable,
    Observed,
    Verified,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExactPhotoGroup {
    pub content_digest: String,
    pub grouping_basis: String,
    pub members: Vec<PhotoEvidence>,
    pub keeper_path: Option<String>,
    pub keeper_blocker: Option<String>,
    pub execution_available: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PhotoDuplicateAudit {
    pub schema_kind: String,
    pub generated_at_ms: u64,
    pub exact_groups: Vec<ExactPhotoGroup>,
    pub inspected_input_count: u64,
    pub evidence_complete: bool,
    pub rejected_input_counts: std::collections::BTreeMap<String, u64>,
    pub perceptual_grouping_available: bool,
    pub perceptual_grouping_blocker: String,
    pub permanent_delete_available: bool,
    pub filesystem_mutation_executed: bool,
}

fn bit_depth_value(depth: png::BitDepth) -> u8 {
    match depth {
        png::BitDepth::One => 1,
        png::BitDepth::Two => 2,
        png::BitDepth::Four => 4,
        png::BitDepth::Eight => 8,
        png::BitDepth::Sixteen => 16,
    }
}

fn admission_blocker(path: &Path, metadata: &std::fs::Metadata) -> Option<&'static str> {
    if metadata.file_type().is_symlink() {
        return Some("photo-input-symlink-rejected");
    }
    if !metadata.is_file() {
        return Some("photo-input-not-regular-file");
    }
    if crate::cloud::metadata_is_dataless(metadata) {
        return Some("photo-input-dataless");
    }
    if crate::cloud::path_inside_managed_photo_library(path) {
        return Some("photo-input-managed-library");
    }
    if crate::cloud::path_inside_managed_file_provider_storage(path) {
        return Some("photo-input-provider-managed");
    }
    if metadata.len() > MAX_ENCODED_IMAGE_BYTES {
        return Some("photo-encoded-size-unsupported");
    }
    None
}

fn read_png_evidence(bytes: &[u8]) -> Result<(u32, u32, u8, u32, String), String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    // EXPAND makes palette/low-depth samples and tRNS transparency explicit before semantic
    // hashing. The source bit depth remains separately recorded as keeper evidence.
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder
        .read_info()
        .map_err(|_| "photo-codec-decode-failed".to_string())?;
    if reader.info().is_animated() {
        return Err("photo-animated-png-unsupported".into());
    }
    let source_bit_depth = bit_depth_value(reader.info().bit_depth);
    let width = reader.info().width;
    let height = reader.info().height;
    let pixel_count = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| "photo-decoded-size-unsupported".to_string())?;
    let normalized_bytes = pixel_count
        .checked_mul(8)
        .ok_or_else(|| "photo-decoded-size-unsupported".to_string())?;
    if normalized_bytes > MAX_NORMALIZED_IMAGE_BYTES {
        return Err("photo-decoded-size-unsupported".into());
    }
    let output_size = reader
        .output_buffer_size()
        .ok_or_else(|| "photo-decoded-size-unsupported".to_string())?;
    if u64::try_from(output_size).unwrap_or(u64::MAX) > MAX_DECODED_IMAGE_BYTES {
        return Err("photo-decoded-size-unsupported".into());
    }
    let mut decoded = vec![0; output_size];
    let output = reader
        .next_frame(&mut decoded)
        .map_err(|_| "photo-codec-decode-failed".to_string())?;
    let normalized = normalize_rgba16(
        &decoded[..output.buffer_size()],
        output.color_type,
        output.bit_depth,
    )?;
    // Count encoded metadata chunks rather than derived source values. png 0.18 derives source
    // gamma/chromaticities from one sRGB chunk, which would otherwise over-weight a single field.
    let metadata_field_count = u32::from(reader.info().gama_chunk.is_some())
        + u32::from(reader.info().chrm_chunk.is_some())
        + u32::from(reader.info().srgb.is_some())
        + u32::from(reader.info().icc_profile.is_some())
        + u32::try_from(reader.info().uncompressed_latin1_text.len()).unwrap_or(u32::MAX)
        + u32::try_from(reader.info().compressed_latin1_text.len()).unwrap_or(u32::MAX)
        + u32::try_from(reader.info().utf8_text.len()).unwrap_or(u32::MAX);
    let mut semantic = blake3::Hasher::new();
    semantic.update(b"disksage-png-rgba16-raster-v2\0");
    semantic.update(&output.width.to_be_bytes());
    semantic.update(&output.height.to_be_bytes());
    semantic.update(&normalized);
    Ok((
        output.width,
        output.height,
        source_bit_depth,
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

fn opened_file_object_id(file: &std::fs::File) -> Result<String, String> {
    #[cfg(windows)]
    {
        let info = winapi_util::file::information(file)
            .map_err(|_| "photo-input-identity-unavailable".to_string())?;
        return Ok(format!(
            "windows:{}:{}",
            info.volume_serial_number(),
            info.file_index()
        ));
    }

    #[cfg(not(windows))]
    {
        let metadata = file
            .metadata()
            .map_err(|_| "photo-input-metadata-unavailable".to_string())?;
        crate::safety::object_id_from_metadata(&metadata)
            .ok_or_else(|| "photo-input-identity-unavailable".to_string())
    }
}

fn hash_current_file(
    path: &Path,
    expected: &std::fs::Metadata,
    expected_identity: &str,
) -> Result<(String, Vec<u8>), String> {
    if expected.len() > MAX_ENCODED_IMAGE_BYTES {
        return Err("photo-encoded-size-unsupported".into());
    }
    let mut file = std::fs::File::open(path).map_err(|_| "photo-input-open-failed".to_string())?;
    let opened = file
        .metadata()
        .map_err(|_| "photo-input-metadata-unavailable".to_string())?;
    let opened_identity = opened_file_object_id(&file)?;
    if opened.len() > MAX_ENCODED_IMAGE_BYTES {
        return Err("photo-encoded-size-unsupported".into());
    }
    if opened.len() != expected.len()
        || opened.modified().ok() != expected.modified().ok()
        || !metadata_identity_matches(Some(opened_identity.as_str()), expected_identity)
    {
        return Err("photo-input-changed".into());
    }
    let capacity = usize::try_from(expected.len()).unwrap_or_default();
    let mut bytes = Vec::with_capacity(capacity);
    let mut bounded = (&mut file).take(MAX_ENCODED_IMAGE_BYTES + 1);
    bounded
        .read_to_end(&mut bytes)
        .map_err(|_| "photo-input-read-failed".to_string())?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_ENCODED_IMAGE_BYTES {
        return Err("photo-encoded-size-unsupported".into());
    }
    if bytes.len() as u64 != expected.len() {
        return Err("photo-input-changed".into());
    }
    let blake3 = blake3::hash(&bytes).to_hex().to_string();
    let after = std::fs::symlink_metadata(path).map_err(|_| "photo-input-changed".to_string())?;
    if after.len() != expected.len()
        || after.modified().ok() != expected.modified().ok()
        || crate::safety::filesystem_object_id(path).ok().as_deref() != Some(expected_identity)
    {
        return Err("photo-input-changed".into());
    }
    Ok((blake3, bytes))
}

fn metadata_identity_matches(observed: Option<&str>, expected: &str) -> bool {
    observed == Some(expected)
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
    // Audit is read-only. Active-use evidence belongs to a fresh execution preflight immediately
    // before any future mutation; requiring it here made valid cross-platform evidence unavailable.
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
    let mut declared_bytes = 0_u64;

    for (index, path) in paths.iter().enumerate() {
        if index >= MAX_AUDIT_INPUTS {
            *rejected_input_counts
                .entry("photo-audit-input-limit-exceeded".into())
                .or_default() += 1;
            continue;
        }
        if let Ok(metadata) = std::fs::symlink_metadata(path) {
            let next_declared = declared_bytes.checked_add(metadata.len());
            if next_declared.is_none_or(|value| value > MAX_AUDIT_DECLARED_BYTES) {
                *rejected_input_counts
                    .entry("photo-audit-byte-budget-exceeded".into())
                    .or_default() += 1;
                continue;
            }
            declared_bytes = next_declared.unwrap_or(declared_bytes);
        }
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
                grouping_basis: "decoded-pixel-rgba16-raster-exact-v2".into(),
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
        schema_kind: "disksage.photo-duplicate-audit.v2".into(),
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

    fn png_with_srgb(path: &Path, width: u32, height: u32, value: u8) {
        let file = std::fs::File::create(path).unwrap();
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
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

    fn png_with_trns(path: &Path, value: u8, transparent: bool) {
        let file = std::fs::File::create(path).unwrap();
        let mut encoder = png::Encoder::new(file, 4, 4);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        if transparent {
            encoder.set_trns(vec![0, value]);
        }
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&vec![value; 16]).unwrap();
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
        assert_eq!(audit.schema_kind, "disksage.photo-duplicate-audit.v2");
    }

    #[test]
    fn raster_dimensions_are_part_of_exact_identity() {
        let temp = tempfile::tempdir().unwrap();
        let row = temp.path().join("row.png");
        let square = temp.path().join("square.png");
        png(&row, 4, 1, 120);
        png(&square, 2, 2, 120);
        let audit = audit_photos(&[row, square], 7);
        assert!(audit.exact_groups.is_empty());
    }

    #[test]
    fn trns_transparency_is_part_of_normalized_pixel_identity() {
        let temp = tempfile::tempdir().unwrap();
        let opaque = temp.path().join("opaque.png");
        let transparent = temp.path().join("transparent.png");
        png_with_trns(&opaque, 5, false);
        png_with_trns(&transparent, 5, true);
        let audit = audit_photos(&[opaque, transparent], 7);
        assert!(audit.exact_groups.is_empty());
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
            "decoded-pixel-rgba16-raster-exact-v2"
        );
        assert_eq!(
            audit.exact_groups[0].keeper_path.as_deref(),
            Some(documented.to_string_lossy().as_ref())
        );
        assert!(!audit.exact_groups[0].execution_available);
    }

    #[test]
    fn srgb_chunk_counts_once_as_metadata_evidence() {
        let temp = tempfile::tempdir().unwrap();
        let plain = temp.path().join("plain.png");
        let srgb = temp.path().join("srgb.png");
        png(&plain, 12, 9, 80);
        png_with_srgb(&srgb, 12, 9, 80);

        let evidence = inspect_photo(&srgb).unwrap();
        assert_eq!(evidence.metadata_field_count, 1);

        let audit = audit_photos(&[plain, srgb.clone()], 7);
        assert_eq!(audit.exact_groups.len(), 1);
        assert_eq!(
            audit.exact_groups[0].keeper_path.as_deref(),
            Some(srgb.to_string_lossy().as_ref())
        );
    }

    #[test]
    fn higher_source_bit_depth_is_the_unique_pareto_keeper_for_identical_samples() {
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
    fn shared_managed_storage_classifiers_fail_closed_without_brand_substrings() {
        let temp = tempfile::tempdir().unwrap();
        let provider = temp
            .path()
            .join("Library/CloudStorage/OneDrive-Personal/image.png");
        let library = temp
            .path()
            .join("Pictures/Custom.photoslibrary/originals/image.png");
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
    fn provider_brand_in_an_ordinary_local_component_is_not_a_blocker() {
        let temp = tempfile::tempdir().unwrap();
        let local = temp.path().join("dropbox-exports/image.png");
        std::fs::create_dir_all(local.parent().unwrap()).unwrap();
        png(&local, 8, 8, 1);
        assert!(inspect_photo(&local).is_ok());
    }

    #[test]
    fn encoded_size_is_rejected_before_content_allocation() {
        let temp = tempfile::tempdir().unwrap();
        let oversized = temp.path().join("oversized.png");
        let file = std::fs::File::create(&oversized).unwrap();
        file.set_len(MAX_ENCODED_IMAGE_BYTES + 1).unwrap();
        assert_eq!(
            inspect_photo(&oversized).unwrap_err(),
            "photo-encoded-size-unsupported"
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
    fn unavailable_metadata_identity_never_authorizes_opened_bytes() {
        assert!(
            !metadata_identity_matches(None, "path-identity"),
            "missing open-handle identity must fail closed rather than authorizing path-race evidence"
        );
        assert!(metadata_identity_matches(
            Some("path-identity"),
            "path-identity"
        ));
        assert!(!metadata_identity_matches(
            Some("replacement"),
            "path-identity"
        ));
    }

    #[test]
    fn opened_handle_identity_remains_bound_when_path_is_replaced() {
        let temp = tempfile::tempdir().unwrap();
        let current = temp.path().join("current.png");
        let replacement = temp.path().join("replacement.png");
        let held = temp.path().join("held-original.png");
        png(&current, 8, 8, 1);
        png(&replacement, 8, 8, 2);

        let expected_identity = crate::safety::filesystem_object_id(&current).unwrap();
        let opened = std::fs::File::open(&current).unwrap();
        std::fs::rename(&current, &held).unwrap();
        std::fs::rename(&replacement, &current).unwrap();

        assert_eq!(opened_file_object_id(&opened).unwrap(), expected_identity);
        assert_ne!(
            crate::safety::filesystem_object_id(&current).unwrap(),
            expected_identity,
            "path replacement must not be mistaken for the already-open reviewed object"
        );
    }
}
