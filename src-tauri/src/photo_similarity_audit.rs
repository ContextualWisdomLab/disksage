//! Bounded perceptual-photo evidence and explicit, reversible survivor quarantine.
//!
//! A DCT perceptual hash is candidate-discovery evidence, never proof that two images are the
//! same photograph. DiskSage uses the published pHash Hamming threshold only with an exact aspect
//! ratio, keeps byte-identical copies in the exact-duplicate workflow, and requires the user to
//! select a survivor before any other member can move to OS Trash.

use image::imageops::FilterType;
use image::{ColorType, ImageFormat, ImageReader};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, Metadata, OpenOptions};
use std::io::{Cursor, Read};
use std::path::{Component, Path};

pub const PHOTO_SIMILARITY_AUDIT_VERSION: u32 = 1;
pub const DEFAULT_MAX_ENTRIES: usize = 50_000;
pub const MAX_ENTRIES: usize = 250_000;
const PHASH_HAMMING_THRESHOLD: u32 = 22;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotoQualityEvidence {
    pub width_pixels: u32,
    pub height_pixels: u32,
    pub pixel_count: u64,
    pub bits_per_sample: u8,
    pub encoded_format: String,
    pub lossless_encoding: Option<bool>,
    pub encoded_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotoSimilarityMember {
    pub member_fingerprint: String,
    pub relative_path: String,
    pub content_blake3: String,
    pub perceptual_hash: String,
    pub aspect_ratio: String,
    pub quality: PhotoQualityEvidence,
    pub filesystem_modified_ms: u64,
    pub filesystem_object_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotoSimilarityGroup {
    pub group_fingerprint: String,
    pub perceptual_hash: String,
    pub aspect_ratio: String,
    pub max_pairwise_hamming_distance: u32,
    pub members: Vec<PhotoSimilarityMember>,
    pub pareto_dominant_survivor: Option<String>,
    pub survivor_rationale: String,
    pub requires_human_survivor_selection: bool,
    pub automatic_delete_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotoSimilarityAuditReport {
    pub schema_version: u32,
    pub observed_at_ms: u64,
    pub source_root: String,
    pub max_entries: usize,
    pub entries_seen: usize,
    pub decoded_photo_count: usize,
    pub group_count: usize,
    pub evidence_complete: bool,
    pub managed_library_excluded_count: usize,
    pub dataless_photo_excluded_count: usize,
    pub issue_counts: BTreeMap<String, u64>,
    pub perceptual_algorithm: String,
    pub grouping_policy: String,
    pub survivor_policy: String,
    pub automatic_delete_allowed: bool,
    pub mutation_performed: bool,
    pub audit_fingerprint: String,
    pub groups: Vec<PhotoSimilarityGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotoQuarantineSelection {
    pub group_fingerprint: String,
    pub survivor_relative_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotoQuarantinePlan {
    pub schema_version: u32,
    pub audit_fingerprint: String,
    pub plan_fingerprint: String,
    pub candidate_file_count: usize,
    pub logical_candidate_bytes: u64,
    pub selections: Vec<PhotoQuarantineSelection>,
    pub exact_approval_phrase: String,
    pub permanent_delete_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotoQuarantineItemReceipt {
    pub member_fingerprint: String,
    pub moved_to_os_trash: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhotoQuarantineReceipt {
    pub schema_version: u32,
    pub audit_fingerprint: String,
    pub plan_fingerprint: String,
    pub executed_at_ms: u64,
    pub rationale: String,
    pub candidate_file_count: usize,
    pub moved_file_count: usize,
    pub failed_file_count: usize,
    pub permanent_delete_performed: bool,
    pub items: Vec<PhotoQuarantineItemReceipt>,
}

fn quarantine_receipt_counts(items: &[PhotoQuarantineItemReceipt]) -> (usize, usize) {
    (
        items.iter().filter(|item| item.moved_to_os_trash).count(),
        items.iter().filter(|item| item.error.is_some()).count(),
    )
}

#[derive(Debug)]
struct DecodedPhoto {
    member: PhotoSimilarityMember,
}

fn increment_issue(issues: &mut BTreeMap<String, u64>, issue: &str) {
    *issues.entry(issue.into()).or_default() += 1;
}

fn system_time_ms(value: std::io::Result<std::time::SystemTime>) -> u64 {
    value
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn hash_value(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value);
}

fn valid_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

#[cfg(unix)]
fn metadata_has_external_alias(metadata: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    metadata.nlink() > 1
}

#[cfg(not(unix))]
fn metadata_has_external_alias(_metadata: &Metadata) -> bool {
    false
}

fn managed_photo_library(path: &Path) -> bool {
    path.components().any(|component| {
        let name = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        name.ends_with(".photoslibrary") || name.ends_with(".photolibrary")
    })
}

fn supported_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "tif" | "tiff" | "webp"
            )
        })
}

fn bits_per_sample(color: ColorType) -> u8 {
    match color {
        ColorType::L8 | ColorType::La8 | ColorType::Rgb8 | ColorType::Rgba8 => 8,
        ColorType::L16 | ColorType::La16 | ColorType::Rgb16 | ColorType::Rgba16 => 16,
        ColorType::Rgb32F | ColorType::Rgba32F => 32,
        _ => 0,
    }
}

fn format_name(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Jpeg => "jpeg",
        ImageFormat::Png => "png",
        ImageFormat::Tiff => "tiff",
        ImageFormat::WebP => "webp",
        _ => "unsupported",
    }
}

fn lossless_encoding(format: ImageFormat) -> Option<bool> {
    match format {
        ImageFormat::Jpeg => Some(false),
        ImageFormat::Png => Some(true),
        // TIFF is a container and can carry JPEG-compressed (lossy) image data. Without parsing
        // its compression tag, preservation is unknown and must not influence a survivor.
        ImageFormat::Tiff => None,
        ImageFormat::WebP => None,
        _ => None,
    }
}

fn gcd(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        (left, right) = (right, left % right);
    }
    left
}

fn aspect_ratio(width: u32, height: u32) -> String {
    let divisor = gcd(width, height);
    format!("{}:{}", width / divisor, height / divisor)
}

fn dct_perceptual_hash(image: &image::DynamicImage) -> String {
    let grayscale = image.to_luma8();
    let resized = image::imageops::resize(&grayscale, 32, 32, FilterType::Triangle);
    let basis = std::array::from_fn::<_, 8, _>(|frequency| {
        std::array::from_fn::<_, 32, _>(|position| {
            ((std::f64::consts::PI
                * f64::from(2 * position as u32 + 1)
                * f64::from(frequency as u32))
                / 64.0)
                .cos()
        })
    });
    let row_transform = std::array::from_fn::<_, 32, _>(|y| {
        std::array::from_fn::<_, 8, _>(|horizontal_frequency| {
            (0..32)
                .map(|x| {
                    f64::from(resized.get_pixel(x, y as u32).0[0])
                        * basis[horizontal_frequency][x as usize]
                })
                .sum::<f64>()
        })
    });
    let mut coefficients = Vec::with_capacity(64);
    for vertical_frequency in 0..8 {
        for horizontal_frequency in 0..8 {
            coefficients.push(
                (0..32)
                    .map(|y| row_transform[y][horizontal_frequency] * basis[vertical_frequency][y])
                    .sum::<f64>(),
            );
        }
    }
    let mut ordered = coefficients.clone();
    ordered.sort_by(f64::total_cmp);
    let median = ordered[ordered.len() / 2];
    let bits = coefficients
        .iter()
        .enumerate()
        .fold(0u64, |hash, (index, value)| {
            hash | (u64::from(*value > median) << index)
        });
    format!("{bits:016x}")
}

fn open_photo_nonhydrating(path: &Path) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK);
    }
    options
        .open(path)
        .map_err(|_| "photo-audit-nonhydrating-open-failed".to_string())
}

fn encoded_bits_per_sample(bytes: &[u8], format: ImageFormat, decoded: ColorType) -> u8 {
    if format == ImageFormat::Png
        && bytes.len() >= 25
        && bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        && &bytes[12..16] == b"IHDR"
    {
        return bytes[24];
    }
    bits_per_sample(decoded)
}

fn metadata_unchanged(before: &Metadata, after: &Metadata) -> bool {
    before.len() == after.len()
        && system_time_ms(before.modified()) == system_time_ms(after.modified())
        && crate::safety::object_id_from_metadata(before)
            == crate::safety::object_id_from_metadata(after)
}

fn decode_photo(root: &Path, path: &Path, metadata: &Metadata) -> Result<DecodedPhoto, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "photo-audit-relative-path-failed".to_string())?;
    if !valid_relative_path(relative) || managed_photo_library(relative) {
        return Err("photo-audit-path-unsafe".into());
    }
    let relative_path = relative
        .to_str()
        .ok_or_else(|| "photo-audit-path-non-unicode".to_string())?
        .replace('\\', "/");
    let object_id = crate::safety::filesystem_object_id(path)
        .map_err(|_| "photo-audit-object-identity-unavailable".to_string())?;
    let mut file = open_photo_nonhydrating(path)?;
    let opened_metadata = file
        .metadata()
        .map_err(|_| "photo-audit-opened-file-stat-failed".to_string())?;
    if !metadata_unchanged(metadata, &opened_metadata) {
        return Err("photo-audit-source-changed-before-read".into());
    }
    if metadata_has_external_alias(&opened_metadata) {
        return Err("photo-audit-hardlink-alias-excluded".into());
    }
    if crate::cloud::metadata_is_dataless(&opened_metadata) {
        return Err("photo-audit-dataless-excluded".into());
    }
    let decoder_limits = image::Limits::default();
    let encoded_byte_limit = decoder_limits.max_alloc.unwrap_or(0);
    if encoded_byte_limit == 0 || opened_metadata.len() > encoded_byte_limit {
        return Err("photo-audit-encoded-input-too-large".into());
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(encoded_byte_limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| "photo-audit-read-failed".to_string())?;
    if bytes.len() as u64 > encoded_byte_limit {
        return Err("photo-audit-encoded-input-too-large".into());
    }
    let after_read = file
        .metadata()
        .map_err(|_| "photo-audit-post-read-stat-failed".to_string())?;
    if !metadata_unchanged(&opened_metadata, &after_read)
        || crate::cloud::metadata_is_dataless(&after_read)
    {
        return Err("photo-audit-source-changed-during-read".into());
    }
    let content_blake3 = blake3::hash(&bytes).to_hex().to_string();
    let mut reader = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|_| "photo-audit-format-probe-failed".to_string())?;
    reader.limits(decoder_limits);
    let format = reader
        .format()
        .filter(|format| format_name(*format) != "unsupported")
        .ok_or_else(|| "photo-audit-format-unsupported".to_string())?;
    let image = reader
        .decode()
        .map_err(|_| "photo-audit-image-decode-failed".to_string())?;
    let after_decode = file
        .metadata()
        .map_err(|_| "photo-audit-post-decode-stat-failed".to_string())?;
    if !metadata_unchanged(&after_read, &after_decode)
        || crate::cloud::metadata_is_dataless(&after_decode)
    {
        return Err("photo-audit-source-changed-during-decode".into());
    }
    let width = image.width();
    let height = image.height();
    if width == 0 || height == 0 {
        return Err("photo-audit-dimensions-invalid".into());
    }
    let modified_ms = system_time_ms(metadata.modified());
    let perceptual_hash = dct_perceptual_hash(&image);
    let quality = PhotoQualityEvidence {
        width_pixels: width,
        height_pixels: height,
        pixel_count: u64::from(width).saturating_mul(u64::from(height)),
        bits_per_sample: encoded_bits_per_sample(&bytes, format, image.color()),
        encoded_format: format_name(format).into(),
        lossless_encoding: lossless_encoding(format),
        encoded_bytes: metadata.len(),
    };
    let ratio = aspect_ratio(width, height);
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-photo-similarity-member-v1\0");
    for value in [
        relative_path.as_bytes(),
        content_blake3.as_bytes(),
        perceptual_hash.as_bytes(),
        ratio.as_bytes(),
        object_id.as_bytes(),
    ] {
        hash_value(&mut hasher, value);
    }
    hasher.update(&metadata.len().to_le_bytes());
    hasher.update(&modified_ms.to_le_bytes());
    hasher.update(&width.to_le_bytes());
    hasher.update(&height.to_le_bytes());
    hasher.update(&[quality.bits_per_sample]);
    Ok(DecodedPhoto {
        member: PhotoSimilarityMember {
            member_fingerprint: hasher.finalize().to_hex().to_string(),
            relative_path,
            content_blake3,
            perceptual_hash,
            aspect_ratio: ratio,
            quality,
            filesystem_modified_ms: modified_ms,
            filesystem_object_id: object_id,
        },
    })
}

fn quality_dominates(left: &PhotoQualityEvidence, right: &PhotoQualityEvidence) -> bool {
    let compression_at_least = match (left.lossless_encoding, right.lossless_encoding) {
        (Some(true), Some(false)) => true,
        (Some(false), Some(true)) => false,
        (left, right) => left == right,
    };
    let at_least = left.pixel_count >= right.pixel_count
        && left.bits_per_sample >= right.bits_per_sample
        && compression_at_least;
    let strictly = left.pixel_count > right.pixel_count
        || left.bits_per_sample > right.bits_per_sample
        || matches!(
            (left.lossless_encoding, right.lossless_encoding),
            (Some(true), Some(false))
        );
    at_least && strictly
}

fn group_from_members(mut members: Vec<PhotoSimilarityMember>) -> PhotoSimilarityGroup {
    members.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    let dominant = members
        .iter()
        .filter(|candidate| {
            members.iter().all(|other| {
                candidate.member_fingerprint == other.member_fingerprint
                    || quality_dominates(&candidate.quality, &other.quality)
            })
        })
        .collect::<Vec<_>>();
    let pareto_dominant_survivor = (dominant.len() == 1).then(|| dominant[0].relative_path.clone());
    let survivor_rationale = pareto_dominant_survivor.as_ref().map_or_else(
        || "측정된 해상도·표본 비트 깊이·압축 보존성이 서로 우열을 확정하지 못해 직접 원본을 선택해야 합니다.".into(),
        |path| format!("{path}은(는) 해상도·표본 비트 깊이·압축 보존성에서 다른 후보를 모두 Pareto 지배합니다. 삭제 권한은 아니며 원본을 직접 확인하세요."),
    );
    let perceptual_hash = members[0].perceptual_hash.clone();
    let ratio = members[0].aspect_ratio.clone();
    let max_pairwise_hamming_distance = members
        .iter()
        .enumerate()
        .flat_map(|(index, left)| members[index + 1..].iter().map(move |right| (left, right)))
        .map(|(left, right)| phash_distance(&left.perceptual_hash, &right.perceptual_hash))
        .max()
        .unwrap_or(0);
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-photo-similarity-group-v1\0");
    hash_value(&mut hasher, ratio.as_bytes());
    hasher.update(&max_pairwise_hamming_distance.to_le_bytes());
    for member in &members {
        hash_value(&mut hasher, member.member_fingerprint.as_bytes());
    }
    PhotoSimilarityGroup {
        group_fingerprint: hasher.finalize().to_hex().to_string(),
        perceptual_hash,
        aspect_ratio: ratio,
        max_pairwise_hamming_distance,
        members,
        pareto_dominant_survivor,
        survivor_rationale,
        requires_human_survivor_selection: true,
        automatic_delete_allowed: false,
    }
}

fn phash_distance(left: &str, right: &str) -> u32 {
    let left = u64::from_str_radix(left, 16).expect("internal perceptual hashes are hexadecimal");
    let right = u64::from_str_radix(right, 16).expect("internal perceptual hashes are hexadecimal");
    (left ^ right).count_ones()
}

fn verified_similarity_edge(left: &PhotoSimilarityMember, right: &PhotoSimilarityMember) -> bool {
    left.aspect_ratio == right.aspect_ratio
        && phash_distance(&left.perceptual_hash, &right.perceptual_hash) <= PHASH_HAMMING_THRESHOLD
}

fn perceptual_groups(decoded: Vec<DecodedPhoto>) -> Vec<Vec<PhotoSimilarityMember>> {
    let mut by_ratio: BTreeMap<String, Vec<PhotoSimilarityMember>> = BTreeMap::new();
    for photo in decoded {
        by_ratio
            .entry(photo.member.aspect_ratio.clone())
            .or_default()
            .push(photo.member);
    }
    let mut groups = Vec::new();
    for mut members in by_ratio.into_values() {
        members.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        let mut parent = (0..members.len()).collect::<Vec<_>>();
        fn root(parent: &mut [usize], mut index: usize) -> usize {
            while parent[index] != index {
                parent[index] = parent[parent[index]];
                index = parent[index];
            }
            index
        }
        // ponytail: quadratic within one exact-aspect-ratio bucket; replace with a BK-tree only
        // if a measured photo corpus makes this bounded audit too slow.
        for left in 0..members.len() {
            for right in left + 1..members.len() {
                if verified_similarity_edge(&members[left], &members[right]) {
                    let left_root = root(&mut parent, left);
                    let right_root = root(&mut parent, right);
                    if left_root != right_root {
                        let (keep, merge) = if left_root < right_root {
                            (left_root, right_root)
                        } else {
                            (right_root, left_root)
                        };
                        parent[merge] = keep;
                    }
                }
            }
        }
        let mut components: BTreeMap<usize, Vec<PhotoSimilarityMember>> = BTreeMap::new();
        for (index, member) in members.into_iter().enumerate() {
            let component = root(&mut parent, index);
            components.entry(component).or_default().push(member);
        }
        groups.extend(components.into_values());
    }
    groups
}

/// Collect non-identical photo candidates without following links or entering managed libraries.
pub fn collect_photo_similarity_audit(
    source_root: &Path,
    observed_at_ms: u64,
    max_entries: usize,
) -> Result<PhotoSimilarityAuditReport, String> {
    if !source_root.is_absolute() {
        return Err("photo-audit-root-must-be-absolute".into());
    }
    if max_entries == 0 || max_entries > MAX_ENTRIES {
        return Err("photo-audit-max-entries-out-of-range".into());
    }
    let root_metadata = std::fs::symlink_metadata(source_root)
        .map_err(|_| "photo-audit-root-unavailable".to_string())?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err("photo-audit-root-unsafe".into());
    }
    let canonical_root = std::fs::canonicalize(source_root)
        .map_err(|_| "photo-audit-root-unavailable".to_string())?;
    if managed_photo_library(&canonical_root) {
        return Err("photo-audit-managed-library-root-rejected".into());
    }
    let source_root_text = canonical_root
        .to_str()
        .ok_or_else(|| "photo-audit-root-non-unicode".to_string())?
        .to_string();
    let mut entries_seen = 0usize;
    let mut managed_library_excluded_count = 0usize;
    let mut dataless_photo_excluded_count = 0usize;
    let mut evidence_complete = true;
    let mut issues = BTreeMap::new();
    let mut decoded = Vec::new();
    let walker = walkdir::WalkDir::new(&canonical_root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            let allowed = !entry.file_type().is_symlink() && !managed_photo_library(entry.path());
            if !allowed && managed_photo_library(entry.path()) {
                managed_library_excluded_count += 1;
            }
            allowed
        });
    for entry in walker {
        if entries_seen >= max_entries {
            evidence_complete = false;
            increment_issue(&mut issues, "photo-audit-entry-limit-reached");
            break;
        }
        entries_seen += 1;
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                evidence_complete = false;
                increment_issue(&mut issues, "photo-audit-directory-entry-failed");
                continue;
            }
        };
        if !entry.file_type().is_file() || !supported_extension(entry.path()) {
            continue;
        }
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(_) => {
                evidence_complete = false;
                increment_issue(&mut issues, "photo-audit-file-stat-failed");
                continue;
            }
        };
        match decode_photo(&canonical_root, entry.path(), &metadata) {
            Ok(photo) => decoded.push(photo),
            Err(issue) if issue == "photo-audit-dataless-excluded" => {
                dataless_photo_excluded_count += 1;
            }
            Err(issue) => {
                evidence_complete = false;
                increment_issue(&mut issues, &issue);
            }
        }
    }
    let decoded_photo_count = decoded.len();
    let mut groups = perceptual_groups(decoded)
        .into_iter()
        .map(|members| {
            let mut seen_content = BTreeSet::new();
            members
                .into_iter()
                .filter(|member| seen_content.insert(member.content_blake3.clone()))
                .collect::<Vec<_>>()
        })
        .filter(|members| members.len() >= 2)
        .map(group_from_members)
        .collect::<Vec<_>>();
    groups.sort_by(|left, right| left.group_fingerprint.cmp(&right.group_fingerprint));
    let mut audit_hasher = blake3::Hasher::new();
    audit_hasher.update(b"disksage-photo-similarity-audit-v1\0");
    hash_value(&mut audit_hasher, source_root_text.as_bytes());
    audit_hasher.update(&(max_entries as u64).to_le_bytes());
    audit_hasher.update(&(entries_seen as u64).to_le_bytes());
    audit_hasher.update(&(managed_library_excluded_count as u64).to_le_bytes());
    audit_hasher.update(&(dataless_photo_excluded_count as u64).to_le_bytes());
    audit_hasher.update(&[evidence_complete as u8]);
    for (issue, count) in &issues {
        hash_value(&mut audit_hasher, issue.as_bytes());
        audit_hasher.update(&count.to_le_bytes());
    }
    for group in &groups {
        hash_value(&mut audit_hasher, group.group_fingerprint.as_bytes());
    }
    let group_count = groups.len();
    Ok(PhotoSimilarityAuditReport {
        schema_version: PHOTO_SIMILARITY_AUDIT_VERSION,
        observed_at_ms,
        source_root: source_root_text,
        max_entries,
        entries_seen,
        decoded_photo_count,
        group_count,
        evidence_complete,
        managed_library_excluded_count,
        dataless_photo_excluded_count,
        issue_counts: issues,
        perceptual_algorithm: "Zauner DCT pHash: 32x32 luminance, 8x8 low-frequency DCT block, median bit quantization, 64-bit Hamming distance".into(),
        grouping_policy: "exact reduced aspect ratio plus pHash Hamming distance at most 22, the pHash reference implementation's published intra/inter-image separation threshold; distinct content digests only".into(),
        survivor_policy: "human selection required; an optional recommendation exists only for a unique Pareto-dominant resolution, sample depth, and known compression preservation tuple".into(),
        automatic_delete_allowed: false,
        mutation_performed: false,
        audit_fingerprint: audit_hasher.finalize().to_hex().to_string(),
        groups,
    })
}

/// Bind one explicit survivor per candidate group into an immutable quarantine plan.
pub fn plan_photo_quarantine(
    report: &PhotoSimilarityAuditReport,
    selections: &[PhotoQuarantineSelection],
) -> Result<PhotoQuarantinePlan, String> {
    if !report.evidence_complete || report.groups.is_empty() {
        return Err("photo-quarantine-evidence-incomplete".into());
    }
    let by_group = selections
        .iter()
        .map(|selection| (selection.group_fingerprint.as_str(), selection))
        .collect::<BTreeMap<_, _>>();
    if by_group.len() != report.groups.len() || by_group.len() != selections.len() {
        return Err("photo-quarantine-selection-set-invalid".into());
    }
    let mut ordered = Vec::with_capacity(report.groups.len());
    let mut logical_candidate_bytes = 0u64;
    let mut candidate_file_count = 0usize;
    for group in &report.groups {
        let selection = by_group
            .get(group.group_fingerprint.as_str())
            .ok_or_else(|| "photo-quarantine-selection-missing".to_string())?;
        if !group
            .members
            .iter()
            .any(|member| member.relative_path == selection.survivor_relative_path)
        {
            return Err("photo-quarantine-survivor-not-member".into());
        }
        for member in &group.members {
            if member.relative_path != selection.survivor_relative_path {
                candidate_file_count += 1;
                logical_candidate_bytes =
                    logical_candidate_bytes.saturating_add(member.quality.encoded_bytes);
            }
        }
        ordered.push((*selection).clone());
    }
    ordered.sort_by(|left, right| left.group_fingerprint.cmp(&right.group_fingerprint));
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-photo-quarantine-plan-v1\0");
    hash_value(&mut hasher, report.audit_fingerprint.as_bytes());
    for selection in &ordered {
        hash_value(&mut hasher, selection.group_fingerprint.as_bytes());
        hash_value(&mut hasher, selection.survivor_relative_path.as_bytes());
    }
    let plan_fingerprint = hasher.finalize().to_hex().to_string();
    let exact_approval_phrase =
        format!("DiskSage photo quarantine {candidate_file_count} 승인 {plan_fingerprint}");
    Ok(PhotoQuarantinePlan {
        schema_version: PHOTO_SIMILARITY_AUDIT_VERSION,
        audit_fingerprint: report.audit_fingerprint.clone(),
        plan_fingerprint,
        candidate_file_count,
        logical_candidate_bytes,
        selections: ordered,
        exact_approval_phrase,
        permanent_delete_allowed: false,
    })
}

fn quarantine_plan_matches_report(
    report: &PhotoSimilarityAuditReport,
    plan: &PhotoQuarantinePlan,
) -> bool {
    plan_photo_quarantine(report, &plan.selections).as_ref() == Ok(plan)
}

fn quarantine_candidates_fail_fast<F>(
    canonical_root: &Path,
    candidates: &[&PhotoSimilarityMember],
    active: &std::collections::BTreeSet<std::path::PathBuf>,
    mut trash: F,
) -> Vec<PhotoQuarantineItemReceipt>
where
    F: FnMut(&Path, &PhotoSimilarityMember) -> (bool, Option<String>),
{
    let mut halted = false;
    candidates
        .iter()
        .map(|member| {
            let path = canonical_root.join(&member.relative_path);
            let (moved_to_os_trash, error) = if halted {
                (false, Some("photo-quarantine-skipped-after-failure".into()))
            } else if active.contains(&path) {
                (false, Some("photo-quarantine-candidate-active".into()))
            } else {
                trash(&path, member)
            };
            halted |= error.is_some();
            PhotoQuarantineItemReceipt {
                member_fingerprint: member.member_fingerprint.clone(),
                moved_to_os_trash,
                error,
            }
        })
        .collect()
}

fn quarantine_participant_paths<'a>(
    canonical_root: &Path,
    groups: impl Iterator<Item = &'a PhotoSimilarityGroup>,
) -> Result<Vec<std::path::PathBuf>, String> {
    let mut paths = Vec::new();
    let mut object_ids = BTreeSet::new();
    for member in groups.flat_map(|group| group.members.iter()) {
        let path = canonical_root.join(&member.relative_path);
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|_| "photo-quarantine-participant-unavailable".to_string())?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() != member.quality.encoded_bytes
            || system_time_ms(metadata.modified()) != member.filesystem_modified_ms
            || crate::cloud::metadata_is_dataless(&metadata)
            || metadata_has_external_alias(&metadata)
            || crate::safety::object_id_from_metadata(&metadata).as_deref()
                != Some(member.filesystem_object_id.as_str())
        {
            return Err("photo-quarantine-participant-changed".into());
        }
        if !object_ids.insert(member.filesystem_object_id.as_str()) {
            return Err("photo-quarantine-participant-alias-detected".into());
        }
        paths.push(path);
    }
    Ok(paths)
}

fn require_inactive_quarantine_group<F>(
    participant_paths: &[std::path::PathBuf],
    active_use_probe: F,
) -> Result<(), String>
where
    F: FnOnce(&[std::path::PathBuf]) -> Result<BTreeSet<std::path::PathBuf>, String>,
{
    if active_use_probe(participant_paths)?.is_empty() {
        Ok(())
    } else {
        Err("photo-quarantine-group-member-active".into())
    }
}

/// Re-audit exact bytes and identities, then move only non-survivors to OS Trash with receipts.
#[cfg(not(coverage))]
pub fn execute_photo_quarantine(
    source_root: &Path,
    reviewed_report: &PhotoSimilarityAuditReport,
    plan: &PhotoQuarantinePlan,
    approval_phrase: &str,
    rationale: &str,
    journal_path: &Path,
    executed_at_ms: u64,
) -> Result<PhotoQuarantineReceipt, String> {
    if rationale.trim() != rationale || rationale.is_empty() || rationale.chars().count() > 1_000 {
        return Err("photo-quarantine-rationale-invalid".into());
    }
    if approval_phrase != plan.exact_approval_phrase {
        return Err("photo-quarantine-approval-mismatch".into());
    }
    if !quarantine_plan_matches_report(reviewed_report, plan) {
        return Err("photo-quarantine-plan-integrity-invalid".into());
    }
    let fresh =
        collect_photo_similarity_audit(source_root, executed_at_ms, reviewed_report.max_entries)?;
    if fresh.audit_fingerprint != reviewed_report.audit_fingerprint {
        return Err("photo-quarantine-source-changed".into());
    }
    if !quarantine_plan_matches_report(&fresh, plan) {
        return Err("photo-quarantine-reviewed-report-integrity-invalid".into());
    }
    let selections = plan
        .selections
        .iter()
        .map(|selection| {
            (
                &selection.group_fingerprint,
                &selection.survivor_relative_path,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let canonical_root = Path::new(&fresh.source_root);
    let participant_paths = quarantine_participant_paths(canonical_root, fresh.groups.iter())?;
    require_inactive_quarantine_group(&participant_paths, |paths| {
        crate::duplicate_audit::active_duplicate_candidates(paths)
    })?;
    let post_probe_paths = quarantine_participant_paths(canonical_root, fresh.groups.iter())?;
    if post_probe_paths != participant_paths {
        return Err("photo-quarantine-participant-changed".into());
    }
    let candidates = fresh
        .groups
        .iter()
        .flat_map(|group| {
            let survivor = selections[&group.group_fingerprint];
            group
                .members
                .iter()
                .filter(move |member| &member.relative_path != survivor)
        })
        .collect::<Vec<_>>();
    let active = BTreeSet::new();
    let items =
        quarantine_candidates_fail_fast(canonical_root, &candidates, &active, |path, member| {
            match crate::safety::trash_delete_if_identity_with_outcome(
                path,
                &member.filesystem_object_id,
                member.quality.encoded_bytes,
                journal_path,
                executed_at_ms,
            ) {
                Ok(outcome) => (
                    outcome.moved_to_trash,
                    outcome
                        .terminal_journal_error
                        .map(|_| "photo-quarantine-terminal-journal-failed".into())
                        .or_else(|| {
                            outcome
                                .staging_cleanup_error
                                .map(|_| "photo-quarantine-staging-cleanup-failed".into())
                        }),
                ),
                Err(_) => (false, Some("photo-quarantine-trash-failed".into())),
            }
        });
    let (moved_file_count, failed_file_count) = quarantine_receipt_counts(&items);
    Ok(PhotoQuarantineReceipt {
        schema_version: PHOTO_SIMILARITY_AUDIT_VERSION,
        audit_fingerprint: fresh.audit_fingerprint,
        plan_fingerprint: plan.plan_fingerprint.clone(),
        executed_at_ms,
        rationale: rationale.into(),
        candidate_file_count: items.len(),
        moved_file_count,
        failed_file_count,
        permanent_delete_performed: false,
        items,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::jpeg::JpegEncoder;
    use image::{DynamicImage, ImageBuffer, ImageEncoder, Rgb};

    fn scene(width: u32, height: u32) -> DynamicImage {
        DynamicImage::ImageRgb8(ImageBuffer::from_fn(width, height, |x, y| {
            let stripe = if (x / (width / 4).max(1) + y / (height / 4).max(1)) % 2 == 0 {
                220
            } else {
                30
            };
            Rgb([
                stripe,
                ((x * 255) / width.max(1)) as u8,
                ((y * 255) / height.max(1)) as u8,
            ])
        }))
    }

    fn write_png(path: &Path, image: &DynamicImage) {
        image.save_with_format(path, ImageFormat::Png).unwrap();
    }

    fn write_jpeg(path: &Path, image: &DynamicImage, quality: u8) {
        let mut bytes = Vec::new();
        JpegEncoder::new_with_quality(&mut bytes, quality)
            .write_image(
                image.as_bytes(),
                image.width(),
                image.height(),
                image.color().into(),
            )
            .unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn participant_member(root: &Path, name: &str) -> PhotoSimilarityMember {
        let path = root.join(name);
        let metadata = std::fs::symlink_metadata(&path).unwrap();
        PhotoSimilarityMember {
            member_fingerprint: blake3::hash(name.as_bytes()).to_hex().to_string(),
            relative_path: name.into(),
            content_blake3: blake3::hash(&std::fs::read(&path).unwrap())
                .to_hex()
                .to_string(),
            perceptual_hash: "0".repeat(16),
            aspect_ratio: "1:1".into(),
            quality: PhotoQualityEvidence {
                width_pixels: 1,
                height_pixels: 1,
                pixel_count: 1,
                bits_per_sample: 8,
                encoded_format: "png".into(),
                lossless_encoding: Some(true),
                encoded_bytes: metadata.len(),
            },
            filesystem_modified_ms: system_time_ms(metadata.modified()),
            filesystem_object_id: crate::safety::object_id_from_metadata(&metadata).unwrap(),
        }
    }

    fn participant_group(members: Vec<PhotoSimilarityMember>) -> PhotoSimilarityGroup {
        PhotoSimilarityGroup {
            group_fingerprint: "group".into(),
            perceptual_hash: "0".repeat(16),
            aspect_ratio: "1:1".into(),
            max_pairwise_hamming_distance: 0,
            members,
            pareto_dominant_survivor: None,
            survivor_rationale: "human selection required".into(),
            requires_human_survivor_selection: true,
            automatic_delete_allowed: false,
        }
    }

    #[test]
    fn groups_real_quality_variants_and_requires_selected_survivor() {
        let root = tempfile::tempdir().unwrap();
        let original = scene(128, 96);
        let smaller = original.resize_exact(64, 48, FilterType::Lanczos3);
        write_png(&root.path().join("original.png"), &original);
        write_jpeg(&root.path().join("compressed.jpg"), &original, 55);
        write_png(&root.path().join("smaller.png"), &smaller);
        let report = collect_photo_similarity_audit(root.path(), 42, 100).unwrap();
        assert!(report.evidence_complete, "{:?}", report.issue_counts);
        assert_eq!(
            report.group_count,
            1,
            "hashes: original={} smaller={} jpeg={}",
            dct_perceptual_hash(&original),
            dct_perceptual_hash(&smaller),
            dct_perceptual_hash(
                &ImageReader::open(root.path().join("compressed.jpg"))
                    .unwrap()
                    .decode()
                    .unwrap()
            )
        );
        let group = &report.groups[0];
        assert_eq!(group.members.len(), 3);
        assert_eq!(
            group.pareto_dominant_survivor.as_deref(),
            Some("original.png")
        );
        assert!(group.requires_human_survivor_selection);
        assert!(!group.automatic_delete_allowed);
        let plan = plan_photo_quarantine(
            &report,
            &[PhotoQuarantineSelection {
                group_fingerprint: group.group_fingerprint.clone(),
                survivor_relative_path: "original.png".into(),
            }],
        )
        .unwrap();
        assert_eq!(plan.candidate_file_count, 2);
        assert!(!plan.permanent_delete_allowed);
        let mut tampered = report.clone();
        tampered.groups[0].members[0].relative_path = "forged.png".into();
        let forged = plan_photo_quarantine(
            &tampered,
            &[PhotoQuarantineSelection {
                group_fingerprint: tampered.groups[0].group_fingerprint.clone(),
                survivor_relative_path: tampered.groups[0].members[0].relative_path.clone(),
            }],
        )
        .unwrap();
        assert!(!quarantine_plan_matches_report(&report, &forged));
    }

    #[test]
    fn verified_similarity_edges_form_one_transitive_component() {
        let member = |path: &str, hash: u64, pixels: u64| DecodedPhoto {
            member: PhotoSimilarityMember {
                member_fingerprint: blake3::hash(path.as_bytes()).to_hex().to_string(),
                relative_path: path.into(),
                content_blake3: blake3::hash(format!("content-{path}").as_bytes())
                    .to_hex()
                    .to_string(),
                perceptual_hash: format!("{hash:016x}"),
                aspect_ratio: "4:3".into(),
                quality: PhotoQualityEvidence {
                    width_pixels: pixels as u32,
                    height_pixels: 1,
                    pixel_count: pixels,
                    bits_per_sample: 8,
                    encoded_format: "png".into(),
                    lossless_encoding: Some(true),
                    encoded_bytes: pixels,
                },
                filesystem_modified_ms: 1,
                filesystem_object_id: format!("object-{path}"),
            },
        };
        let a = 0_u64;
        let b = (1_u64 << PHASH_HAMMING_THRESHOLD) - 1;
        let c = b | (((1_u64 << PHASH_HAMMING_THRESHOLD) - 1) << PHASH_HAMMING_THRESHOLD);
        assert_eq!((a ^ b).count_ones(), PHASH_HAMMING_THRESHOLD);
        assert_eq!((b ^ c).count_ones(), PHASH_HAMMING_THRESHOLD);
        assert!((a ^ c).count_ones() > PHASH_HAMMING_THRESHOLD);

        let components = perceptual_groups(vec![
            member("a.png", a, 300),
            member("b.png", b, 200),
            member("c.png", c, 100),
        ]);
        assert_eq!(components.len(), 1);
        let group = group_from_members(components.into_iter().next().unwrap());
        assert_eq!(group.members.len(), 3);
        assert_eq!(group.pareto_dominant_survivor.as_deref(), Some("a.png"));
        assert_eq!(group.max_pairwise_hamming_distance, 44);
    }

    #[test]
    fn excludes_managed_photo_library_and_exact_byte_duplicates() {
        let root = tempfile::tempdir().unwrap();
        let managed = root.path().join("Photos Library.photoslibrary");
        std::fs::create_dir(&managed).unwrap();
        let image = scene(64, 48);
        write_png(&managed.join("managed.png"), &image);
        write_png(&root.path().join("one.png"), &image);
        std::fs::copy(root.path().join("one.png"), root.path().join("two.png")).unwrap();
        let report = collect_photo_similarity_audit(root.path(), 42, 100).unwrap();
        assert!(report.evidence_complete);
        assert_eq!(report.managed_library_excluded_count, 1);
        assert!(report.groups.is_empty());
    }

    #[test]
    fn oversized_encoded_photo_is_rejected_before_allocation() {
        let root = tempfile::tempdir().unwrap();
        let oversized = root.path().join("oversized.png");
        let limit = image::Limits::default().max_alloc.unwrap();
        let file = File::create(&oversized).unwrap();
        file.set_len(limit + 1).unwrap();
        let report = collect_photo_similarity_audit(root.path(), 42, 100).unwrap();
        assert!(!report.evidence_complete);
        assert_eq!(
            report
                .issue_counts
                .get("photo-audit-encoded-input-too-large"),
            Some(&1)
        );
        assert_eq!(report.decoded_photo_count, 0);
    }

    #[test]
    fn journal_failure_counts_as_failure_after_truthful_move() {
        let items = [PhotoQuarantineItemReceipt {
            member_fingerprint: "a".repeat(64),
            moved_to_os_trash: true,
            error: Some("photo-quarantine-terminal-journal-failed".into()),
        }];
        assert_eq!(quarantine_receipt_counts(&items), (1, 1));
    }

    #[test]
    fn quarantine_stops_invoking_trash_after_first_failure() {
        let member = |name: &str| PhotoSimilarityMember {
            member_fingerprint: blake3::hash(name.as_bytes()).to_hex().to_string(),
            relative_path: name.into(),
            content_blake3: "a".repeat(64),
            perceptual_hash: "0".repeat(16),
            aspect_ratio: "1:1".into(),
            quality: PhotoQualityEvidence {
                width_pixels: 1,
                height_pixels: 1,
                pixel_count: 1,
                bits_per_sample: 8,
                encoded_format: "png".into(),
                lossless_encoding: Some(true),
                encoded_bytes: 1,
            },
            filesystem_modified_ms: 1,
            filesystem_object_id: format!("object-{name}"),
        };
        let candidates = [member("one.png"), member("two.png"), member("three.png")];
        let candidate_refs = candidates.iter().collect::<Vec<_>>();
        let mut calls = 0;
        let receipts = quarantine_candidates_fail_fast(
            Path::new("/photos"),
            &candidate_refs,
            &BTreeSet::new(),
            |_, _| {
                calls += 1;
                (false, Some("photo-quarantine-trash-failed".into()))
            },
        );
        assert_eq!(calls, 1);
        assert_eq!(
            receipts[0].error.as_deref(),
            Some("photo-quarantine-trash-failed")
        );
        assert!(receipts[1..]
            .iter()
            .all(|item| item.error.as_deref() == Some("photo-quarantine-skipped-after-failure")));
    }

    #[test]
    fn quarantine_preflight_includes_survivor_and_candidate_identities() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("survivor.png"), b"survivor").unwrap();
        std::fs::write(root.path().join("candidate.png"), b"candidate").unwrap();
        let group = participant_group(vec![
            participant_member(root.path(), "survivor.png"),
            participant_member(root.path(), "candidate.png"),
        ]);

        let paths = quarantine_participant_paths(root.path(), std::iter::once(&group)).unwrap();

        assert_eq!(paths.len(), 2);
        assert!(paths.contains(&root.path().join("survivor.png")));
        assert!(paths.contains(&root.path().join("candidate.png")));
    }

    #[test]
    fn active_survivor_blocks_the_entire_group_before_trash() {
        let survivor = std::path::PathBuf::from("/fixture/survivor.png");
        let candidate = std::path::PathBuf::from("/fixture/candidate.png");
        let participants = vec![survivor.clone(), candidate];
        let error = require_inactive_quarantine_group(&participants, |observed| {
            assert_eq!(observed, participants);
            Ok(BTreeSet::from([survivor]))
        })
        .unwrap_err();

        assert_eq!(error, "photo-quarantine-group-member-active");
    }

    #[test]
    fn quarantine_preflight_rejects_replaced_survivor() {
        let root = tempfile::tempdir().unwrap();
        let survivor = root.path().join("survivor.png");
        std::fs::write(&survivor, b"reviewed survivor").unwrap();
        let group = participant_group(vec![participant_member(root.path(), "survivor.png")]);
        let replacement = root.path().join("replacement.png");
        std::fs::write(&replacement, b"replacement with different identity").unwrap();
        std::fs::rename(&replacement, &survivor).unwrap();

        assert_eq!(
            quarantine_participant_paths(root.path(), std::iter::once(&group)).unwrap_err(),
            "photo-quarantine-participant-changed"
        );
    }

    #[cfg(unix)]
    #[test]
    fn quarantine_preflight_rejects_hardlink_alias() {
        let root = tempfile::tempdir().unwrap();
        let survivor = root.path().join("survivor.png");
        std::fs::write(&survivor, b"reviewed survivor").unwrap();
        let group = participant_group(vec![participant_member(root.path(), "survivor.png")]);
        std::fs::hard_link(&survivor, root.path().join("external-alias.png")).unwrap();

        assert_eq!(
            quarantine_participant_paths(root.path(), std::iter::once(&group)).unwrap_err(),
            "photo-quarantine-participant-changed"
        );
    }
}
