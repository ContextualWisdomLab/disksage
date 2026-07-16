//! Cloud-offload discovery and dry-run planning.
//!
//! This module is intentionally local and deterministic: it never uploads, moves, deletes,
//! hydrates, or calls a model.  The plan preserves enough source metadata to become the first
//! lineage record for a later verified move.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
#[cfg(not(coverage))]
use std::process::Command;
use unicode_normalization::UnicodeNormalization;

const ARCHIVE_DIR: &str = "DiskSage Archive";
const DAY_MS: u64 = 86_400_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CloudProvider {
    Icloud,
    Onedrive,
    GoogleDrive,
}

impl CloudProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Icloud => "icloud",
            Self::Onedrive => "onedrive",
            Self::GoogleDrive => "google-drive",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CloudRoot {
    pub id: String,
    pub provider: CloudProvider,
    pub label: String,
    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArchiveKind {
    Document,
    Media,
    Archive,
    Dataset,
    Backup,
    Creative,
}

impl ArchiveKind {
    fn folder(self) -> &'static str {
        match self {
            Self::Document => "documents",
            Self::Media => "media",
            Self::Archive => "archives",
            Self::Dataset => "datasets",
            Self::Backup => "backups",
            Self::Creative => "creative",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFact {
    pub path: PathBuf,
    pub bytes: u64,
    pub created_ms: u64,
    pub modified_ms: u64,
    pub content_metadata: ContentMetadata,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContentMetadata {
    pub production_time_ms: Option<u64>,
    pub production_time_source: Option<String>,
    pub production_time_confidence: Option<String>,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub context: Vec<String>,
    pub duration_ms: Option<u64>,
    pub evidence: Vec<MetadataEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MetadataEvidence {
    pub field: String,
    pub value: String,
    pub source: String,
    pub confidence: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloudPlanOptions {
    pub min_size_bytes: u64,
    pub min_age_days: u64,
    pub limit: usize,
}

impl Default for CloudPlanOptions {
    fn default() -> Self {
        Self {
            min_size_bytes: 256 * 1024 * 1024,
            min_age_days: 90,
            limit: 200,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CloudCandidate {
    /// Stable metadata fingerprint. This is not a content hash.
    pub metadata_fingerprint: String,
    pub src: String,
    pub dst: String,
    pub provider: CloudProvider,
    pub kind: ArchiveKind,
    pub bytes: u64,
    pub age_days: u64,
    pub created_ms: u64,
    pub modified_ms: u64,
    pub production_time_ms: u64,
    pub production_time_source: String,
    pub production_time_confidence: String,
    pub source_root: String,
    pub relative_path: String,
    pub source_context: String,
    pub requires_review: bool,
    pub review_reasons: Vec<String>,
    pub content_title: Option<String>,
    pub content_authors: Vec<String>,
    pub content_context: Vec<String>,
    pub duration_ms: Option<u64>,
    pub metadata_evidence: Vec<MetadataEvidence>,
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CloudPlanReport {
    pub cloud_root: CloudRoot,
    pub generated_at_ms: u64,
    pub candidates: Vec<CloudCandidate>,
    pub candidate_bytes: u64,
    pub potentially_reclaimable_bytes: u64,
    pub notices: Vec<String>,
}

#[cfg(not(coverage))]
fn is_writable_dir(path: &Path) -> bool {
    path.metadata()
        .map(|m| m.is_dir() && !m.permissions().readonly())
        .unwrap_or(false)
}

#[cfg(not(coverage))]
fn read_children_sorted(path: &Path, limit: usize) -> Vec<PathBuf> {
    let mut children: Vec<PathBuf> = std::fs::read_dir(path)
        .ok()
        .into_iter()
        .flatten()
        .take(limit)
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    children.sort();
    children
}

#[cfg(not(coverage))]
fn push_root(
    roots: &mut Vec<CloudRoot>,
    seen: &mut BTreeSet<PathBuf>,
    provider: CloudProvider,
    path: PathBuf,
    label: String,
) {
    let identity = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    if !is_writable_dir(&path) || !seen.insert(identity) {
        return;
    }
    let value = path.to_string_lossy().into_owned();
    roots.push(CloudRoot {
        id: value.clone(),
        provider,
        label,
        path: value,
    });
}

#[cfg(not(coverage))]
fn provider_account_label(prefix: &str, path: &Path) -> String {
    path.file_name()
        .map(|name| {
            name.to_string_lossy()
                .trim_start_matches(prefix)
                .to_string()
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "default".into())
}

/// Discover writable local File Provider roots without creating a probe file.
///
/// Google Drive's account root is read-only on macOS, so each writable direct child (for
/// example "My Drive" or a writable shared drive) is surfaced as a separate destination.
#[cfg(not(coverage))]
pub fn discover_cloud_roots(home: &Path) -> Vec<CloudRoot> {
    let mut roots = Vec::new();
    let mut seen = BTreeSet::new();

    push_root(
        &mut roots,
        &mut seen,
        CloudProvider::Icloud,
        home.join("Library/Mobile Documents/com~apple~CloudDocs"),
        "iCloud Drive".into(),
    );
    push_root(
        &mut roots,
        &mut seen,
        CloudProvider::Icloud,
        home.join("iCloudDrive"),
        "iCloud Drive".into(),
    );

    let cloud_storage = home.join("Library/CloudStorage");
    for account_root in read_children_sorted(&cloud_storage, 128) {
        let name = account_root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name.starts_with("OneDrive-") {
            let account = provider_account_label("OneDrive-", &account_root);
            push_root(
                &mut roots,
                &mut seen,
                CloudProvider::Onedrive,
                account_root,
                format!("OneDrive · {account}"),
            );
        } else if name.starts_with("GoogleDrive-") {
            let account = provider_account_label("GoogleDrive-", &account_root);
            for drive in read_children_sorted(&account_root, 128) {
                let drive_name = drive
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if drive_name.starts_with('.') {
                    continue;
                }
                push_root(
                    &mut roots,
                    &mut seen,
                    CloudProvider::GoogleDrive,
                    drive,
                    format!("Google Drive · {account} · {drive_name}"),
                );
            }
        }
    }

    // Windows and older clients commonly place provider roots directly under the home folder.
    for path in read_children_sorted(home, 128) {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name == "OneDrive" || name.starts_with("OneDrive - ") {
            push_root(
                &mut roots,
                &mut seen,
                CloudProvider::Onedrive,
                path,
                format!("OneDrive · {name}"),
            );
        } else if name == "Google Drive" || name.starts_with("Google Drive ") {
            push_root(
                &mut roots,
                &mut seen,
                CloudProvider::GoogleDrive,
                path,
                format!("Google Drive · {name}"),
            );
        }
    }

    roots.sort_by(|a, b| {
        (a.provider.as_str(), &a.label, &a.path).cmp(&(b.provider.as_str(), &b.label, &b.path))
    });
    roots
}

fn archive_kind(path: &Path) -> Option<ArchiveKind> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    match ext.as_str() {
        "pdf" | "doc" | "docx" | "ppt" | "pptx" | "xls" | "xlsx" | "odt" | "ods" | "odp"
        | "pages" | "numbers" | "key" | "epub" | "mobi" => Some(ArchiveKind::Document),
        "jpg" | "jpeg" | "png" | "heic" | "tif" | "tiff" | "gif" | "webp" | "raw" | "mov"
        | "mp4" | "m4v" | "mkv" | "avi" | "wav" | "mp3" | "m4a" | "flac" | "aiff" => {
            Some(ArchiveKind::Media)
        }
        "zip" | "7z" | "rar" | "tar" | "tgz" | "gz" | "bz2" | "xz" | "zst" | "dmg" | "iso" => {
            Some(ArchiveKind::Archive)
        }
        "csv" | "tsv" | "parquet" | "feather" | "arrow" | "sav" | "sas7bdat" | "dta" | "rdata"
        | "rds" | "sqlite" | "sqlite3" | "db" | "sql" | "jsonl" => Some(ArchiveKind::Dataset),
        "bak" | "backup" | "vhd" | "vhdx" | "qcow2" | "img" => Some(ArchiveKind::Backup),
        "psd" | "ai" | "indd" | "sketch" | "fig" | "blend" => Some(ArchiveKind::Creative),
        _ => None,
    }
}

#[cfg(not(coverage))]
fn pruned_directory(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    name.starts_with('.')
        || matches!(
            lower.as_str(),
            "library"
                | "applications"
                | "system"
                | "node_modules"
                | "target"
                | "venv"
                | ".venv"
                | "__pycache__"
                | "caches"
                | "cache"
        )
}

#[cfg(not(coverage))]
fn millis(time: std::io::Result<std::time::SystemTime>) -> u64 {
    time.ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Collect only archive-shaped regular files while pruning cloud roots and regenerable trees
/// before descent. Symlinks/reparse points are rejected by the shared scanner guard.
#[cfg(not(coverage))]
pub fn collect_archive_files(root: &Path, excluded_roots: &[PathBuf]) -> Vec<FileFact> {
    let excluded = excluded_roots.to_vec();
    let mut files: Vec<FileFact> = jwalk::WalkDir::new(root)
        .follow_links(false)
        .skip_hidden(false)
        .process_read_dir(move |_depth, _path, _state, children| {
            children.retain(|result| {
                result
                    .as_ref()
                    .map(|entry| {
                        let path = entry.path();
                        if excluded.iter().any(|cloud| path.starts_with(cloud)) {
                            return false;
                        }
                        if entry.file_type().is_dir()
                            && entry
                                .file_name()
                                .to_str()
                                .map(pruned_directory)
                                .unwrap_or(true)
                        {
                            return false;
                        }
                        crate::scanner::keep_entry(entry)
                    })
                    .unwrap_or(true)
            });
        })
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file() && archive_kind(&entry.path()).is_some())
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            let path = entry.path();
            Some(FileFact {
                path,
                bytes: metadata.len(),
                created_ms: millis(metadata.created()),
                modified_ms: millis(metadata.modified()),
                content_metadata: ContentMetadata::default(),
            })
        })
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files
}

/// Gregorian civil date from whole days since Unix epoch. The arithmetic is the
/// proleptic-Gregorian era decomposition; no locale or timezone is involved.
fn civil_from_days(days_since_epoch: i64) -> (i32, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year as i32, month as u32, day as u32)
}

fn date_parts(epoch_ms: u64) -> (i32, u32, u32) {
    civil_from_days((epoch_ms / DAY_MS) as i64)
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let mut year = i64::from(year);
    let month = i64::from(month);
    let day = i64::from(day);
    year -= i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn leap_year(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn valid_date(year: i32, month: u32, day: u32) -> bool {
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap_year(year) => 29,
        2 => 28,
        _ => return false,
    };
    (1970..=2100).contains(&year) && (1..=max_day).contains(&day)
}

fn date_epoch_ms(year: i32, month: u32, day: u32) -> Option<u64> {
    valid_date(year, month, day)
        .then(|| days_from_civil(year, month, day))
        .and_then(|days| u64::try_from(days).ok())
        .map(|days| days * DAY_MS)
}

fn digits(bytes: &[u8], start: usize, len: usize) -> Option<u32> {
    let slice = bytes.get(start..start + len)?;
    slice.iter().all(u8::is_ascii_digit).then(|| {
        slice
            .iter()
            .fold(0u32, |value, digit| value * 10 + u32::from(*digit - b'0'))
    })
}

fn token_boundary(bytes: &[u8], start: usize, end: usize) -> bool {
    start
        .checked_sub(1)
        .and_then(|i| bytes.get(i))
        .map(|b| !b.is_ascii_digit())
        .unwrap_or(true)
        && bytes.get(end).map(|b| !b.is_ascii_digit()).unwrap_or(true)
}

/// Extract common production-date tokens from a filename without reading file contents.
/// Supported shapes: YYYY-MM-DD, YYYY_MM_DD, YYYY.MM.DD, YYYYMMDD, and YYMMDD.
fn filename_date_ms(path: &Path) -> Option<u64> {
    let normalized: String = path.file_name()?.to_string_lossy().nfc().collect();
    let bytes = normalized.as_bytes();
    for start in 0..bytes.len() {
        if let (Some(year), Some(month), Some(day), Some(sep1), Some(sep2)) = (
            digits(bytes, start, 4),
            digits(bytes, start + 5, 2),
            digits(bytes, start + 8, 2),
            bytes.get(start + 4),
            bytes.get(start + 7),
        ) {
            if matches!(sep1, b'-' | b'_' | b'.')
                && sep1 == sep2
                && token_boundary(bytes, start, start + 10)
            {
                if let Some(ms) = date_epoch_ms(year as i32, month, day) {
                    return Some(ms);
                }
            }
        }
        if token_boundary(bytes, start, start + 8) {
            if let (Some(year), Some(month), Some(day)) = (
                digits(bytes, start, 4),
                digits(bytes, start + 4, 2),
                digits(bytes, start + 6, 2),
            ) {
                if let Some(ms) = date_epoch_ms(year as i32, month, day) {
                    return Some(ms);
                }
            }
        }
        if token_boundary(bytes, start, start + 6) {
            if let (Some(year), Some(month), Some(day)) = (
                digits(bytes, start, 2),
                digits(bytes, start + 2, 2),
                digits(bytes, start + 4, 2),
            ) {
                if let Some(ms) = date_epoch_ms(2000 + year as i32, month, day) {
                    return Some(ms);
                }
            }
        }
    }
    None
}

fn date_value(epoch_ms: u64) -> String {
    let (year, month, day) = date_parts(epoch_ms);
    format!("{year:04}-{month:02}-{day:02}")
}

fn add_evidence(
    metadata: &mut ContentMetadata,
    field: &str,
    value: impl Into<String>,
    source: &str,
    confidence: &str,
) {
    metadata.evidence.push(MetadataEvidence {
        field: field.into(),
        value: value.into(),
        source: source.into(),
        confidence: confidence.into(),
    });
}

fn set_production_time(
    metadata: &mut ContentMetadata,
    epoch_ms: u64,
    source: &str,
    confidence: &str,
) {
    add_evidence(
        metadata,
        "production-date",
        date_value(epoch_ms),
        source,
        confidence,
    );
    let confidence_rank = |value: Option<&str>| match value {
        Some("high") => 3,
        Some("medium") => 2,
        Some("low") => 1,
        _ => 0,
    };
    if metadata.production_time_ms.is_none()
        || confidence_rank(Some(confidence))
            > confidence_rank(metadata.production_time_confidence.as_deref())
    {
        metadata.production_time_ms = Some(epoch_ms);
        metadata.production_time_source = Some(source.into());
        metadata.production_time_confidence = Some(confidence.into());
    }
}

fn decoded_hex_ascii(value: &str) -> Option<String> {
    let compact: String = value.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    if compact.len() < 2
        || compact.len() % 2 != 0
        || !compact.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return None;
    }
    let bytes: Vec<u8> = compact
        .as_bytes()
        .chunks_exact(2)
        .filter_map(|pair| {
            let text = std::str::from_utf8(pair).ok()?;
            u8::from_str_radix(text, 16).ok()
        })
        .collect();
    (bytes.len() * 2 == compact.len()).then(|| String::from_utf8_lossy(&bytes).into_owned())
}

fn date_from_text(value: &str) -> Option<u64> {
    filename_date_ms(Path::new(value)).or_else(|| {
        let normalized = value.replace(':', "-");
        filename_date_ms(Path::new(&normalized))
    })
}

#[cfg(not(coverage))]
fn local_command(name: &str) -> Command {
    for directory in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"] {
        let path = Path::new(directory).join(name);
        if path.is_file() {
            return Command::new(path);
        }
    }
    Command::new(name)
}

fn json_strings(value: Option<&serde_json::Value>) -> Vec<String> {
    match value {
        Some(serde_json::Value::String(value)) if !value.trim().is_empty() => {
            vec![value.trim().to_string()]
        }
        Some(serde_json::Value::Array(values)) => values
            .iter()
            .filter_map(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect(),
        Some(value) if value.is_number() => vec![value.to_string()],
        _ => Vec::new(),
    }
}

fn push_context(metadata: &mut ContentMetadata, field: &str, value: &str, source: &str) {
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    let bounded: String = value.chars().take(500).collect();
    metadata.context.push(format!("{field}={bounded}"));
    add_evidence(metadata, field, bounded, source, "high");
}

#[cfg(not(coverage))]
fn exiftool_metadata(path: &Path) -> ContentMetadata {
    let mut metadata = ContentMetadata::default();
    let Ok(output) = local_command("exiftool")
        .args([
            "-j",
            "-n",
            "-DateTimeOriginal",
            "-CreateDate",
            "-CreationDate",
            "-MediaCreateDate",
            "-TrackCreateDate",
            "-Title",
            "-DocumentName",
            "-Author",
            "-Artist",
            "-Creator",
            "-Subject",
            "-Keywords",
            "-Description",
            "-Category",
            "-Duration",
            "-GPSLatitude",
            "-GPSLongitude",
            "-Location",
        ])
        .arg(path)
        .output()
    else {
        return metadata;
    };
    let Ok(document) = serde_json::from_slice::<Vec<serde_json::Value>>(&output.stdout) else {
        return metadata;
    };
    let Some(values) = document.first().and_then(|value| value.as_object()) else {
        return metadata;
    };

    for key in [
        "DateTimeOriginal",
        "MediaCreateDate",
        "TrackCreateDate",
        "CreateDate",
        "CreationDate",
    ] {
        for value in json_strings(values.get(key)) {
            if let Some(epoch_ms) = date_from_text(&value) {
                set_production_time(
                    &mut metadata,
                    epoch_ms,
                    &format!("embedded:exiftool:{key}"),
                    "high",
                );
            }
        }
    }
    for key in ["Title", "DocumentName"] {
        if let Some(value) = json_strings(values.get(key)).into_iter().next() {
            if metadata.title.is_none() {
                metadata.title = Some(value.clone());
            }
            add_evidence(
                &mut metadata,
                "title",
                &value,
                &format!("embedded:exiftool:{key}"),
                "high",
            );
            if metadata.production_time_ms.is_none() {
                if let Some(epoch_ms) = date_from_text(&value) {
                    set_production_time(
                        &mut metadata,
                        epoch_ms,
                        &format!("embedded:exiftool:{key}-date"),
                        "medium",
                    );
                }
            }
        }
    }
    for key in ["Author", "Artist", "Creator"] {
        for value in json_strings(values.get(key)) {
            if !metadata.authors.contains(&value) {
                metadata.authors.push(value.clone());
            }
            add_evidence(
                &mut metadata,
                "author",
                value,
                &format!("embedded:exiftool:{key}"),
                "high",
            );
        }
    }
    for key in ["Subject", "Keywords", "Description", "Category"] {
        for value in json_strings(values.get(key)) {
            push_context(
                &mut metadata,
                &key.to_ascii_lowercase(),
                &value,
                &format!("embedded:exiftool:{key}"),
            );
        }
    }
    if let Some(duration) = values.get("Duration").and_then(|value| value.as_f64()) {
        let duration_ms = (duration.max(0.0) * 1_000.0).round() as u64;
        metadata.duration_ms = Some(duration_ms);
        add_evidence(
            &mut metadata,
            "duration-ms",
            duration_ms.to_string(),
            "embedded:exiftool:Duration",
            "high",
        );
    }
    let latitude = json_strings(values.get("GPSLatitude")).into_iter().next();
    let longitude = json_strings(values.get("GPSLongitude")).into_iter().next();
    if latitude.is_some() || longitude.is_some() {
        add_evidence(
            &mut metadata,
            "geolocation",
            format!(
                "lat={}, lon={}",
                latitude.as_deref().unwrap_or("unknown"),
                longitude.as_deref().unwrap_or("unknown")
            ),
            "embedded:exiftool:gps",
            "high",
        );
    }
    for location in json_strings(values.get("Location")) {
        push_context(
            &mut metadata,
            "location",
            &location,
            "embedded:exiftool:Location",
        );
    }
    metadata
}

#[cfg(not(coverage))]
fn ffprobe_metadata(path: &Path) -> ContentMetadata {
    let mut metadata = ContentMetadata::default();
    let Ok(output) = local_command("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration:format_tags=creation_time,date,title,artist,comment,location",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
    else {
        return metadata;
    };
    let Ok(document) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
        return metadata;
    };
    let Some(format) = document.get("format") else {
        return metadata;
    };
    if let Some(duration) = format
        .get("duration")
        .and_then(|v| v.as_str())
        .and_then(|v| v.parse::<f64>().ok())
    {
        let duration_ms = (duration.max(0.0) * 1_000.0).round() as u64;
        metadata.duration_ms = Some(duration_ms);
        add_evidence(
            &mut metadata,
            "duration-ms",
            duration_ms.to_string(),
            "embedded:ffprobe:container",
            "high",
        );
    }
    let Some(tags) = format.get("tags").and_then(|v| v.as_object()) else {
        return metadata;
    };
    if let Some(title) = tags
        .get("title")
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
    {
        metadata.title = Some(title.into());
        add_evidence(
            &mut metadata,
            "title",
            title,
            "embedded:ffprobe:title",
            "high",
        );
    }
    if let Some(artist) = tags
        .get("artist")
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
    {
        metadata.authors.push(artist.into());
        add_evidence(
            &mut metadata,
            "author",
            artist,
            "embedded:ffprobe:artist",
            "medium",
        );
    }
    for key in ["creation_time", "date"] {
        if let Some(value) = tags
            .get(key)
            .and_then(|v| v.as_str())
            .and_then(date_from_text)
        {
            set_production_time(
                &mut metadata,
                value,
                &format!("embedded:ffprobe:{key}"),
                "high",
            );
        }
    }
    if let Some(comment) = tags.get("comment").and_then(|v| v.as_str()) {
        let decoded = decoded_hex_ascii(comment).unwrap_or_else(|| comment.into());
        if let Some(value) = date_from_text(&decoded) {
            set_production_time(
                &mut metadata,
                value,
                "embedded:ffprobe:comment-date",
                "high",
            );
        }
    }
    if let Some(title) = metadata.title.clone() {
        if let Some(value) = date_from_text(&title) {
            set_production_time(
                &mut metadata,
                value,
                "embedded:ffprobe:title-date",
                "medium",
            );
        }
    }
    if let Some(location) = tags
        .get("location")
        .and_then(|v| v.as_str())
        .filter(|v| !v.is_empty())
    {
        add_evidence(
            &mut metadata,
            "geolocation",
            location,
            "embedded:ffprobe:location",
            "high",
        );
    }
    metadata
}

fn pdf_date(value: &str) -> Option<u64> {
    let parts: Vec<&str> = value.split_whitespace().collect();
    if parts.len() < 5 {
        return date_from_text(value);
    }
    let month = match parts[1] {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return date_from_text(value),
    };
    date_epoch_ms(parts[4].parse().ok()?, month, parts[2].parse().ok()?)
}

#[cfg(not(coverage))]
fn pdfinfo_metadata(path: &Path) -> ContentMetadata {
    let mut metadata = ContentMetadata::default();
    let Ok(output) = local_command("pdfinfo").arg(path).output() else {
        return metadata;
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let Some((field, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match field.trim() {
            "Title" if !value.is_empty() => {
                metadata.title = Some(value.into());
                add_evidence(
                    &mut metadata,
                    "title",
                    value,
                    "embedded:pdfinfo:title",
                    "high",
                );
            }
            "Author" if !value.is_empty() => {
                metadata.authors.push(value.into());
                add_evidence(
                    &mut metadata,
                    "author",
                    value,
                    "embedded:pdfinfo:author",
                    "high",
                );
            }
            "Subject" | "Keywords" if !value.is_empty() => {
                push_context(
                    &mut metadata,
                    &field.trim().to_ascii_lowercase(),
                    value,
                    &format!("embedded:pdfinfo:{}", field.trim().to_ascii_lowercase()),
                );
            }
            "CreationDate" => {
                if let Some(epoch_ms) = pdf_date(value) {
                    set_production_time(
                        &mut metadata,
                        epoch_ms,
                        "embedded:pdfinfo:creation-date",
                        "high",
                    );
                }
            }
            "ModDate" => {
                if let Some(epoch_ms) = pdf_date(value) {
                    add_evidence(
                        &mut metadata,
                        "content-modification-date",
                        date_value(epoch_ms),
                        "embedded:pdfinfo:mod-date",
                        "high",
                    );
                }
            }
            _ => {}
        }
    }
    metadata
}

fn xml_value(xml: &str, local_name: &str) -> Option<String> {
    let marker = format!(":{local_name}");
    let marker_start = xml.find(&marker)?;
    let open_start = xml[..marker_start].rfind('<')?;
    let open_end = xml[marker_start..].find('>')? + marker_start;
    let tag_name = xml[open_start + 1..open_end].split_whitespace().next()?;
    let close = format!("</{tag_name}>");
    let value_start = open_end + 1;
    let value_end = xml[value_start..].find(&close)? + value_start;
    Some(xml[value_start..value_end].to_string())
}

#[cfg(not(coverage))]
fn zipped_document_metadata(path: &Path, entry: &str) -> ContentMetadata {
    let mut metadata = ContentMetadata::default();
    let Ok(output) = local_command("unzip")
        .args(["-p"])
        .arg(path)
        .arg(entry)
        .output()
    else {
        return metadata;
    };
    let xml = String::from_utf8_lossy(&output.stdout);
    if let Some(title) = xml_value(&xml, "title").filter(|v| !v.is_empty()) {
        metadata.title = Some(title.clone());
        add_evidence(
            &mut metadata,
            "title",
            title,
            "embedded:ooxml:core-properties",
            "high",
        );
    }
    if let Some(author) = xml_value(&xml, "creator").filter(|v| !v.is_empty()) {
        metadata.authors.push(author.clone());
        add_evidence(
            &mut metadata,
            "author",
            author,
            "embedded:ooxml:core-properties",
            "high",
        );
    }
    for field in ["subject", "keywords", "description"] {
        if let Some(value) = xml_value(&xml, field).filter(|value| !value.is_empty()) {
            push_context(
                &mut metadata,
                field,
                &value,
                &format!("embedded:zip-document:{entry}"),
            );
        }
    }
    if let Some(epoch_ms) = xml_value(&xml, "created").and_then(|v| date_from_text(&v)) {
        set_production_time(&mut metadata, epoch_ms, "embedded:ooxml:created", "high");
    }
    if let Some(epoch_ms) = xml_value(&xml, "modified").and_then(|v| date_from_text(&v)) {
        add_evidence(
            &mut metadata,
            "content-modification-date",
            date_value(epoch_ms),
            "embedded:ooxml:modified",
            "high",
        );
    }
    metadata
}

fn merge_metadata(mut primary: ContentMetadata, secondary: ContentMetadata) -> ContentMetadata {
    let confidence_rank = |value: Option<&str>| match value {
        Some("high") => 3,
        Some("medium") => 2,
        Some("low") => 1,
        _ => 0,
    };
    if primary.production_time_ms.is_none()
        || confidence_rank(secondary.production_time_confidence.as_deref())
            > confidence_rank(primary.production_time_confidence.as_deref())
    {
        primary.production_time_ms = secondary.production_time_ms;
        primary.production_time_source = secondary.production_time_source;
        primary.production_time_confidence = secondary.production_time_confidence;
    }
    if primary.title.is_none() {
        primary.title = secondary.title;
    }
    for author in secondary.authors {
        if !primary.authors.contains(&author) {
            primary.authors.push(author);
        }
    }
    for context in secondary.context {
        if !primary.context.contains(&context) {
            primary.context.push(context);
        }
    }
    if primary.duration_ms.is_none() {
        primary.duration_ms = secondary.duration_ms;
    }
    primary.evidence.extend(secondary.evidence);
    primary
}

#[cfg(not(coverage))]
fn probe_content_metadata(path: &Path) -> ContentMetadata {
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let general = exiftool_metadata(path);
    let format_specific = match extension.as_str() {
        "m4a" | "mp4" | "m4v" | "mov" | "mkv" | "avi" | "wav" | "mp3" | "flac" | "aiff" => {
            ffprobe_metadata(path)
        }
        "pdf" => pdfinfo_metadata(path),
        "docx" | "xlsx" | "pptx" => zipped_document_metadata(path, "docProps/core.xml"),
        "odt" | "ods" | "odp" => zipped_document_metadata(path, "meta.xml"),
        _ => ContentMetadata::default(),
    };
    merge_metadata(general, format_specific)
}

fn looks_like_coordinates(name: &str) -> bool {
    let values: Vec<f64> = name
        .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .map(|token| token.trim_matches('.'))
        .filter(|token| token.contains('.'))
        .filter_map(|token| token.parse().ok())
        .collect();
    values.iter().enumerate().any(|(index, latitude)| {
        (-90.0..=90.0).contains(latitude)
            && values[index + 1..]
                .iter()
                .any(|longitude| (-180.0..=180.0).contains(longitude) && longitude.abs() > 90.0)
    })
}

fn review_reasons(path: &Path, kind: ArchiveKind) -> Vec<String> {
    let mut reasons = Vec::new();
    if matches!(kind, ArchiveKind::Archive | ArchiveKind::Backup) {
        reasons.push("opaque-container-content-uninspected".into());
    }
    if kind == ArchiveKind::Dataset {
        reasons.push("structured-data-may-contain-personal-data".into());
    }
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if matches!(extension.as_str(), "wav" | "mp3" | "m4a" | "flac" | "aiff") {
        reasons.push("recording-may-contain-sensitive-speech".into());
    }
    let name: String = path
        .file_name()
        .map(|n| n.to_string_lossy().nfc().collect::<String>().to_lowercase())
        .unwrap_or_default();
    if [
        "meeting",
        "interview",
        "회의",
        "상담",
        "진료",
        "patient",
        "client",
        "고객",
        "infra",
        "인프라",
        "효성",
        "itx",
        "계약",
        "contract",
    ]
    .iter()
    .any(|term| name.contains(term))
    {
        reasons.push("filename-context-may-be-confidential".into());
    }
    if looks_like_coordinates(&name) {
        reasons.push("filename-contains-geolocation".into());
    }
    reasons.sort();
    reasons.dedup();
    reasons
}

fn metadata_fingerprint(file: &FileFact, relative: &Path) -> String {
    let input = format!(
        "{}\0{}\0{}\0{}",
        relative.to_string_lossy(),
        file.bytes,
        file.created_ms,
        file.modified_ms
    );
    blake3::hash(input.as_bytes()).to_hex().to_string()
}

/// Build a dry-run report. No filesystem mutation occurs.
pub fn plan_cloud_archive(
    files: &[FileFact],
    source_root: &Path,
    cloud_root: &CloudRoot,
    now_ms: u64,
    options: CloudPlanOptions,
) -> CloudPlanReport {
    let mut candidates = Vec::new();
    for file in files {
        if file.bytes < options.min_size_bytes || file.modified_ms == 0 {
            continue;
        }
        let age_days = now_ms.saturating_sub(file.modified_ms) / DAY_MS;
        if age_days < options.min_age_days {
            continue;
        }
        let Some(kind) = archive_kind(&file.path) else {
            continue;
        };
        let Ok(relative) = file.path.strip_prefix(source_root) else {
            continue;
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        let filename_ms = filename_date_ms(&file.path);
        let mut lineage_metadata = file.content_metadata.clone();
        // Coverage builds exercise the deterministic planning core. Content probing is an
        // external-process adapter (ExifTool/ffprobe/pdfinfo/unzip) covered by normal tests and
        // integration smoke runs, so it is kept outside the in-process line-coverage boundary.
        #[cfg(not(coverage))]
        if lineage_metadata == ContentMetadata::default() && file.path.is_file() {
            lineage_metadata = probe_content_metadata(&file.path);
        }
        let embedded_production_time_ms = lineage_metadata.production_time_ms;
        if let Some(value) = filename_ms {
            add_evidence(
                &mut lineage_metadata,
                "production-date",
                date_value(value),
                "filename-date",
                "low",
            );
        }
        if file.created_ms > 0 {
            add_evidence(
                &mut lineage_metadata,
                "filesystem-created-date",
                date_value(file.created_ms),
                "filesystem:created",
                "low",
            );
        }
        add_evidence(
            &mut lineage_metadata,
            "filesystem-modified-date",
            date_value(file.modified_ms),
            "filesystem:modified",
            "medium",
        );
        let (production_time_ms, production_time_source, production_time_confidence) =
            if let Some(embedded_ms) = lineage_metadata.production_time_ms {
                (
                    embedded_ms,
                    lineage_metadata
                        .production_time_source
                        .clone()
                        .unwrap_or_else(|| "embedded:unknown".into()),
                    lineage_metadata
                        .production_time_confidence
                        .clone()
                        .unwrap_or_else(|| "medium".into()),
                )
            } else if let Some(filename_ms) = filename_ms {
                (filename_ms, "filename-date".into(), "low".into())
            } else if file.created_ms > 0 {
                (file.created_ms, "filesystem:created".into(), "low".into())
            } else {
                (
                    file.modified_ms,
                    "filesystem:modified-fallback".into(),
                    "low".into(),
                )
            };
        let (year, month, _day) = date_parts(production_time_ms);
        let dst = Path::new(&cloud_root.path)
            .join(ARCHIVE_DIR)
            .join(format!("{year:04}"))
            .join(format!("{month:02}"))
            .join(kind.folder())
            .join(relative);
        let blocked_reason = dst.exists().then(|| "destination-exists".to_string());
        let source_context = relative
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".".into());
        let mut review_reasons = review_reasons(&file.path, kind);
        if !production_time_source.starts_with("embedded:") {
            review_reasons.push("production-date-not-from-embedded-metadata".into());
        }
        let embedded_dates: BTreeSet<&str> = lineage_metadata
            .evidence
            .iter()
            .filter(|evidence| {
                evidence.field == "production-date" && evidence.source.starts_with("embedded:")
            })
            .map(|evidence| evidence.value.as_str())
            .collect();
        if embedded_dates.len() > 1 {
            review_reasons.push("embedded-production-date-conflict".into());
        }
        if lineage_metadata
            .evidence
            .iter()
            .any(|e| e.field == "geolocation")
        {
            review_reasons.push("embedded-metadata-contains-geolocation".into());
        }
        if let (Some(embedded_ms), Some(filename_ms)) = (embedded_production_time_ms, filename_ms) {
            if embedded_ms.abs_diff(filename_ms) > DAY_MS {
                review_reasons.push("embedded-and-filename-date-conflict".into());
            }
        }
        review_reasons.sort();
        review_reasons.dedup();
        candidates.push(CloudCandidate {
            metadata_fingerprint: metadata_fingerprint(file, relative),
            src: file.path.to_string_lossy().into_owned(),
            dst: dst.to_string_lossy().into_owned(),
            provider: cloud_root.provider,
            kind,
            bytes: file.bytes,
            age_days,
            created_ms: file.created_ms,
            modified_ms: file.modified_ms,
            production_time_ms,
            production_time_source,
            production_time_confidence,
            source_root: source_root.to_string_lossy().into_owned(),
            relative_path: relative.to_string_lossy().into_owned(),
            source_context,
            requires_review: !review_reasons.is_empty(),
            review_reasons,
            content_title: lineage_metadata.title,
            content_authors: lineage_metadata.authors,
            content_context: lineage_metadata.context,
            duration_ms: lineage_metadata.duration_ms,
            metadata_evidence: lineage_metadata.evidence,
            blocked_reason,
        });
    }
    candidates.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.src.cmp(&b.src)));
    candidates.truncate(options.limit);
    let candidate_bytes = candidates.iter().map(|c| c.bytes).sum();
    let potentially_reclaimable_bytes = candidates
        .iter()
        .filter(|c| c.blocked_reason.is_none())
        .map(|c| c.bytes)
        .sum();
    CloudPlanReport {
        cloud_root: cloud_root.clone(),
        generated_at_ms: now_ms,
        candidates,
        candidate_bytes,
        potentially_reclaimable_bytes,
        notices: vec![
            "dry-run-only".into(),
            "cloud-quota-unverified".into(),
            "cloud-sync-unverified".into(),
            "content-hash-pending".into(),
        ],
    }
}

#[cfg(not(coverage))]
pub fn system_now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn writable_dir(path: &Path) {
        std::fs::create_dir_all(path).unwrap();
    }

    fn root(provider: CloudProvider, path: &Path) -> CloudRoot {
        CloudRoot {
            id: path.to_string_lossy().into_owned(),
            provider,
            label: "test".into(),
            path: path.to_string_lossy().into_owned(),
        }
    }

    #[cfg(not(coverage))]
    #[test]
    fn discovers_icloud_onedrive_and_writable_google_children() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        writable_dir(&home.join("Library/Mobile Documents/com~apple~CloudDocs"));
        writable_dir(&home.join("Library/CloudStorage/OneDrive-Personal"));
        let google = home.join("Library/CloudStorage/GoogleDrive-me@example.com");
        writable_dir(&google.join("My Drive"));
        writable_dir(&google.join(".Trash"));
        let roots = discover_cloud_roots(home);
        assert_eq!(roots.len(), 3);
        assert!(roots.iter().any(|r| r.provider == CloudProvider::Icloud));
        assert!(roots.iter().any(|r| r.provider == CloudProvider::Onedrive));
        assert!(roots
            .iter()
            .any(|r| r.provider == CloudProvider::GoogleDrive && r.path.ends_with("My Drive")));
        assert!(!roots.iter().any(|r| r.path.ends_with(".Trash")));
    }

    #[cfg(not(coverage))]
    #[test]
    fn discovers_direct_home_provider_roots_without_duplicates() {
        let tmp = tempfile::tempdir().unwrap();
        writable_dir(&tmp.path().join("OneDrive"));
        writable_dir(&tmp.path().join("Google Drive local"));
        writable_dir(&tmp.path().join("iCloudDrive"));
        let roots = discover_cloud_roots(tmp.path());
        assert_eq!(roots.len(), 3);
        assert_eq!(
            roots
                .iter()
                .filter(|r| r.provider == CloudProvider::Icloud)
                .count(),
            1
        );
    }

    #[cfg(all(unix, not(coverage)))]
    #[test]
    fn canonical_identity_deduplicates_provider_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("Library/CloudStorage/OneDrive-Personal");
        writable_dir(&target);
        std::os::unix::fs::symlink(&target, tmp.path().join("OneDrive")).unwrap();
        let roots = discover_cloud_roots(tmp.path());
        assert_eq!(
            roots
                .iter()
                .filter(|r| r.provider == CloudProvider::Onedrive)
                .count(),
            1
        );
    }

    #[cfg(all(unix, not(coverage)))]
    #[test]
    fn ignores_readonly_provider_root() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let one = tmp.path().join("OneDrive");
        writable_dir(&one);
        std::fs::set_permissions(&one, std::fs::Permissions::from_mode(0o500)).unwrap();
        assert!(discover_cloud_roots(tmp.path()).is_empty());
    }

    #[cfg(not(coverage))]
    #[test]
    fn collects_only_archive_shapes_and_prunes_cloud_and_generated_trees() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("home");
        let cloud = root.join("Library/CloudStorage/OneDrive-Personal");
        writable_dir(&cloud);
        writable_dir(&root.join("Documents"));
        writable_dir(&root.join("project/node_modules"));
        std::fs::write(root.join("Documents/report.pdf"), b"pdf").unwrap();
        std::fs::write(root.join("Documents/code.rs"), b"rust").unwrap();
        std::fs::write(root.join("project/node_modules/bundle.zip"), b"zip").unwrap();
        std::fs::write(cloud.join("already.mp4"), b"video").unwrap();
        let files = collect_archive_files(&root, std::slice::from_ref(&cloud));
        assert_eq!(files.len(), 1);
        assert!(files[0].path.ends_with("report.pdf"));
        assert!(files[0].modified_ms > 0);
    }

    #[cfg(all(unix, not(coverage)))]
    #[test]
    fn collector_excludes_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let scan_root = tmp.path().join("scan");
        writable_dir(&scan_root);
        let real = scan_root.join("real.pdf");
        std::fs::write(&real, b"pdf").unwrap();
        std::os::unix::fs::symlink(&real, scan_root.join("link.pdf")).unwrap();
        let files = collect_archive_files(&scan_root, &[]);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, real);
    }

    #[test]
    fn civil_date_math_handles_epoch_and_leap_day() {
        assert_eq!(date_parts(0), (1970, 1, 1));
        assert_eq!(date_parts(1_582_934_400_000), (2020, 2, 29));
        assert_eq!(civil_from_days(-719_468), (0, 3, 1));
        assert_eq!(
            date_parts(date_epoch_ms(2024, 2, 29).unwrap()),
            (2024, 2, 29)
        );
        assert_eq!(date_epoch_ms(2023, 2, 29), None);
    }

    #[test]
    fn filename_dates_override_filesystem_creation_time() {
        assert_eq!(
            date_parts(filename_date_ms(Path::new("2026-04-28T10_00.m4a")).unwrap()),
            (2026, 4, 28)
        );
        assert_eq!(
            date_parts(filename_date_ms(Path::new("report_20240131.pdf")).unwrap()),
            (2024, 1, 31)
        );
        assert_eq!(
            date_parts(filename_date_ms(Path::new("251210_0926.m4a")).unwrap()),
            (2025, 12, 10)
        );
        assert_eq!(filename_date_ms(Path::new("bad_20230229.pdf")), None);
    }

    #[test]
    fn metadata_helpers_extract_embedded_dates_and_namespaced_values() {
        let decoded =
            decoded_hex_ascii("323032352d31312d31375430393a32363a30342b30393a3030").unwrap();
        assert_eq!(decoded, "2025-11-17T09:26:04+09:00");
        assert_eq!(
            date_parts(date_from_text(&decoded).unwrap()),
            (2025, 11, 17)
        );
        assert_eq!(
            date_parts(pdf_date("Wed Mar 4 10:49:07 2026 KST").unwrap()),
            (2026, 3, 4)
        );
        let xml = r#"<cp:coreProperties><dc:title>Quarterly report</dc:title><dcterms:created xsi:type="dcterms:W3CDTF">2026-02-03T12:00:00Z</dcterms:created></cp:coreProperties>"#;
        assert_eq!(xml_value(xml, "title").as_deref(), Some("Quarterly report"));
        assert_eq!(
            xml_value(xml, "created").as_deref(),
            Some("2026-02-03T12:00:00Z")
        );

        let mut medium = ContentMetadata::default();
        set_production_time(&mut medium, 1, "embedded:title-date", "medium");
        let mut high = ContentMetadata::default();
        set_production_time(&mut high, 2, "embedded:container-date", "high");
        let merged = merge_metadata(medium, high);
        assert_eq!(merged.production_time_ms, Some(2));
        assert_eq!(
            merged.production_time_source.as_deref(),
            Some("embedded:container-date")
        );
        assert_eq!(merged.production_time_confidence.as_deref(), Some("high"));
    }

    #[test]
    fn metadata_helpers_reject_malformed_values_and_cover_confidence_precedence() {
        assert_eq!(decoded_hex_ascii(""), None);
        assert_eq!(decoded_hex_ascii("0"), None);
        assert_eq!(decoded_hex_ascii("GG"), None);
        assert_eq!(
            date_parts(date_from_text("2026:03:04 10:49:07").unwrap()),
            (2026, 3, 4)
        );

        let mut metadata = ContentMetadata {
            production_time_ms: Some(1),
            production_time_confidence: Some("low".into()),
            ..ContentMetadata::default()
        };
        set_production_time(&mut metadata, 2, "embedded:unknown", "unknown");
        assert_eq!(metadata.production_time_ms, Some(1));
        set_production_time(&mut metadata, 3, "embedded:high", "high");
        assert_eq!(metadata.production_time_ms, Some(3));

        metadata.production_time_confidence = None;
        set_production_time(&mut metadata, 4, "embedded:medium", "medium");
        assert_eq!(metadata.production_time_ms, Some(4));

        assert_eq!(
            json_strings(Some(&serde_json::json!("  Alice  "))),
            ["Alice"]
        );
        assert_eq!(
            json_strings(Some(&serde_json::json!([" Alice ", "", 7, "Bob"]))),
            ["Alice", "Bob"]
        );
        assert_eq!(json_strings(Some(&serde_json::json!(42))), ["42"]);
        assert!(json_strings(Some(&serde_json::Value::Null)).is_empty());
        assert!(json_strings(None).is_empty());

        let mut context = ContentMetadata::default();
        push_context(&mut context, "subject", "   ", "embedded:test");
        assert!(context.context.is_empty());
        let oversized = "x".repeat(501);
        push_context(&mut context, "subject", &oversized, "embedded:test");
        assert_eq!(context.context[0].len(), "subject=".len() + 500);
        assert_eq!(context.evidence[0].value.len(), 500);
    }

    #[test]
    fn date_parsers_cover_invalid_tokens_and_all_pdf_months() {
        assert_eq!(date_epoch_ms(2024, 13, 1), None);
        assert_eq!(filename_date_ms(Path::new("2023-02-29.pdf")), None);
        assert_eq!(filename_date_ms(Path::new("230229.pdf")), None);
        assert_eq!(archive_kind(Path::new("x.unknown")), None);

        for (month_name, month) in [
            ("Jan", 1),
            ("Feb", 2),
            ("Mar", 3),
            ("Apr", 4),
            ("May", 5),
            ("Jun", 6),
            ("Jul", 7),
            ("Aug", 8),
            ("Sep", 9),
            ("Oct", 10),
            ("Nov", 11),
            ("Dec", 12),
        ] {
            let value = format!("Wed {month_name} 4 10:49:07 2026 KST");
            assert_eq!(date_parts(pdf_date(&value).unwrap()), (2026, month, 4));
        }
        assert_eq!(date_parts(pdf_date("2026-03-04").unwrap()), (2026, 3, 4));
        assert_eq!(
            date_parts(pdf_date("Wed Xxx 4 2026-03-04 2026 KST").unwrap()),
            (2026, 3, 4)
        );
    }

    #[test]
    fn metadata_merge_preserves_primary_and_adds_distinct_values() {
        let primary = ContentMetadata {
            production_time_ms: Some(10),
            production_time_source: Some("embedded:primary".into()),
            production_time_confidence: Some("high".into()),
            title: Some("Primary".into()),
            authors: vec!["Alice".into()],
            context: vec!["subject=one".into()],
            duration_ms: Some(10),
            evidence: vec![],
        };
        let secondary = ContentMetadata {
            production_time_ms: Some(20),
            production_time_source: Some("embedded:secondary".into()),
            production_time_confidence: Some("low".into()),
            title: Some("Secondary".into()),
            authors: vec!["Alice".into(), "Bob".into()],
            context: vec!["subject=one".into(), "subject=two".into()],
            duration_ms: Some(20),
            evidence: vec![MetadataEvidence {
                field: "title".into(),
                value: "Secondary".into(),
                source: "embedded:test".into(),
                confidence: "low".into(),
            }],
        };
        let merged = merge_metadata(primary, secondary);
        assert_eq!(merged.production_time_ms, Some(10));
        assert_eq!(merged.title.as_deref(), Some("Primary"));
        assert_eq!(merged.authors, ["Alice", "Bob"]);
        assert_eq!(merged.context, ["subject=one", "subject=two"]);
        assert_eq!(merged.duration_ms, Some(10));
        assert_eq!(merged.evidence.len(), 1);

        let merged = merge_metadata(
            ContentMetadata {
                production_time_ms: Some(10),
                ..ContentMetadata::default()
            },
            ContentMetadata {
                production_time_ms: Some(20),
                production_time_source: Some("embedded:low".into()),
                production_time_confidence: Some("low".into()),
                title: Some("Secondary".into()),
                duration_ms: Some(20),
                ..ContentMetadata::default()
            },
        );
        assert_eq!(merged.production_time_ms, Some(20));
        assert_eq!(merged.title.as_deref(), Some("Secondary"));
        assert_eq!(merged.duration_ms, Some(20));
    }

    #[test]
    fn planner_prefers_embedded_metadata_and_preserves_conflicting_evidence() {
        let source = PathBuf::from("/source");
        let cloud = root(CloudProvider::GoogleDrive, Path::new("/cloud"));
        let embedded_ms = date_epoch_ms(2024, 1, 2).unwrap();
        let modified_ms = date_epoch_ms(2026, 6, 1).unwrap();
        let report = plan_cloud_archive(
            &[FileFact {
                path: source.join("2026-04-28 meeting.m4a"),
                bytes: 1_000,
                created_ms: modified_ms,
                modified_ms,
                content_metadata: ContentMetadata {
                    production_time_ms: Some(embedded_ms),
                    production_time_source: Some("embedded:test:creation-date".into()),
                    production_time_confidence: Some("high".into()),
                    title: Some("Actual recording title".into()),
                    authors: vec!["Recorder".into()],
                    context: vec!["subject=Planning".into()],
                    duration_ms: Some(60_000),
                    evidence: vec![
                        MetadataEvidence {
                            field: "production-date".into(),
                            value: "2024-01-02".into(),
                            source: "embedded:test:creation-date".into(),
                            confidence: "high".into(),
                        },
                        MetadataEvidence {
                            field: "production-date".into(),
                            value: "2024-01-03".into(),
                            source: "embedded:test:modification-date".into(),
                            confidence: "medium".into(),
                        },
                    ],
                },
            }],
            &source,
            &cloud,
            modified_ms,
            CloudPlanOptions {
                min_size_bytes: 0,
                min_age_days: 0,
                limit: 10,
            },
        );
        let candidate = &report.candidates[0];
        assert_eq!(candidate.production_time_ms, embedded_ms);
        assert_eq!(
            candidate.production_time_source,
            "embedded:test:creation-date"
        );
        assert_eq!(candidate.production_time_confidence, "high");
        assert!(candidate
            .dst
            .contains("DiskSage Archive/2024/01/media/2026-04-28 meeting.m4a"));
        assert!(candidate
            .review_reasons
            .contains(&"embedded-and-filename-date-conflict".to_string()));
        assert!(candidate
            .review_reasons
            .contains(&"embedded-production-date-conflict".to_string()));
        assert!(!candidate
            .review_reasons
            .contains(&"production-date-not-from-embedded-metadata".to_string()));
        assert_eq!(candidate.content_context, ["subject=Planning"]);
        assert!(candidate.metadata_evidence.iter().any(|evidence| {
            evidence.source == "filename-date" && evidence.confidence == "low"
        }));
    }

    #[test]
    fn review_reasons_flag_opaque_recording_context_and_location() {
        let archive = review_reasons(Path::new("bundle.zip"), ArchiveKind::Archive);
        assert!(archive.contains(&"opaque-container-content-uninspected".to_string()));
        let recording = review_reasons(
            Path::new("2026-04-28 meeting 37.53 126.89.m4a"),
            ArchiveKind::Media,
        );
        assert!(recording.contains(&"recording-may-contain-sensitive-speech".to_string()));
        assert!(recording.contains(&"filename-context-may-be-confidential".to_string()));
        assert!(recording.contains(&"filename-contains-geolocation".to_string()));
        assert!(review_reasons(Path::new("photo.jpg"), ArchiveKind::Media).is_empty());
    }

    #[test]
    fn plans_lineage_layout_age_risk_sort_limit_and_collision() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let cloud = tmp.path().join("cloud");
        writable_dir(&source.join("research"));
        writable_dir(&cloud);
        let old = 1_672_531_200_000; // 2023-01-01
        let now = old + 200 * DAY_MS;
        let report = plan_cloud_archive(
            &[
                FileFact {
                    path: source.join("research/data.csv"),
                    bytes: 500,
                    created_ms: old,
                    modified_ms: old,
                    content_metadata: ContentMetadata::default(),
                },
                FileFact {
                    path: source.join("research/paper.pdf"),
                    bytes: 900,
                    created_ms: 0,
                    modified_ms: old,
                    content_metadata: ContentMetadata::default(),
                },
                FileFact {
                    path: source.join("research/new.zip"),
                    bytes: 2_000,
                    created_ms: now,
                    modified_ms: now,
                    content_metadata: ContentMetadata::default(),
                },
                FileFact {
                    path: PathBuf::from("/outside/movie.mp4"),
                    bytes: 5_000,
                    created_ms: old,
                    modified_ms: old,
                    content_metadata: ContentMetadata::default(),
                },
            ],
            &source,
            &root(CloudProvider::Onedrive, &cloud),
            now,
            CloudPlanOptions {
                min_size_bytes: 100,
                min_age_days: 90,
                limit: 2,
            },
        );
        assert_eq!(report.candidates.len(), 2);
        assert!(report.candidates[0].src.ends_with("paper.pdf"));
        assert_eq!(
            report.candidates[0].production_time_source,
            "filesystem:modified-fallback"
        );
        assert_eq!(report.candidates[0].source_context, "research");
        assert!(report.candidates[0]
            .dst
            .contains("DiskSage Archive/2023/01/documents/research/paper.pdf"));
        assert!(report.candidates[1].requires_review);
        assert!(report.candidates[1]
            .review_reasons
            .contains(&"structured-data-may-contain-personal-data".to_string()));
        assert_eq!(report.candidate_bytes, 1_400);
        assert_eq!(report.potentially_reclaimable_bytes, 1_400);

        let collision = PathBuf::from(&report.candidates[0].dst);
        writable_dir(collision.parent().unwrap());
        let mut file = std::fs::File::create(&collision).unwrap();
        file.write_all(b"existing").unwrap();
        let rerun = plan_cloud_archive(
            &[FileFact {
                path: source.join("research/paper.pdf"),
                bytes: 900,
                created_ms: 0,
                modified_ms: old,
                content_metadata: ContentMetadata::default(),
            }],
            &source,
            &root(CloudProvider::Onedrive, &cloud),
            now,
            CloudPlanOptions {
                min_size_bytes: 0,
                min_age_days: 0,
                limit: 10,
            },
        );
        assert_eq!(
            rerun.candidates[0].blocked_reason.as_deref(),
            Some("destination-exists")
        );
        assert_eq!(rerun.potentially_reclaimable_bytes, 0);
    }

    #[test]
    fn planner_skips_missing_timestamp_small_unknown_and_future_files() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let cloud = tmp.path().join("cloud");
        writable_dir(&source);
        writable_dir(&cloud);
        let files = vec![
            FileFact {
                path: source.join("missing.pdf"),
                bytes: 10,
                created_ms: 0,
                modified_ms: 0,
                content_metadata: ContentMetadata::default(),
            },
            FileFact {
                path: source.join("small.pdf"),
                bytes: 1,
                created_ms: 1,
                modified_ms: 1,
                content_metadata: ContentMetadata::default(),
            },
            FileFact {
                path: source.join("unknown.bin"),
                bytes: 10,
                created_ms: 1,
                modified_ms: 1,
                content_metadata: ContentMetadata::default(),
            },
            FileFact {
                path: source.join("future.pdf"),
                bytes: 10,
                created_ms: 10_000,
                modified_ms: 10_000,
                content_metadata: ContentMetadata::default(),
            },
        ];
        let report = plan_cloud_archive(
            &files,
            &source,
            &root(CloudProvider::Icloud, &cloud),
            100,
            CloudPlanOptions {
                min_size_bytes: 5,
                min_age_days: 1,
                limit: 10,
            },
        );
        assert!(report.candidates.is_empty());
        assert_eq!(report.notices.len(), 4);
    }

    #[test]
    fn planner_covers_filename_dates_geolocation_and_invalid_relative_paths() {
        let source = PathBuf::from("/source.pdf");
        let now = date_epoch_ms(2026, 7, 1).unwrap();
        let report = plan_cloud_archive(
            &[
                FileFact {
                    path: PathBuf::from("/source.pdf"),
                    bytes: 10,
                    created_ms: 1,
                    modified_ms: 1,
                    content_metadata: ContentMetadata::default(),
                },
                FileFact {
                    path: PathBuf::from("/source.pdf/2025-12-10 report.pdf"),
                    bytes: 20,
                    created_ms: 1,
                    modified_ms: 1,
                    content_metadata: ContentMetadata {
                        evidence: vec![MetadataEvidence {
                            field: "geolocation".into(),
                            value: "37.5,126.9".into(),
                            source: "embedded:test:gps".into(),
                            confidence: "high".into(),
                        }],
                        ..ContentMetadata::default()
                    },
                },
                FileFact {
                    path: PathBuf::from("/source.pdf/unknown.bin"),
                    bytes: 30,
                    created_ms: 1,
                    modified_ms: 1,
                    content_metadata: ContentMetadata::default(),
                },
            ],
            &source,
            &root(CloudProvider::Icloud, Path::new("/cloud")),
            now,
            CloudPlanOptions {
                min_size_bytes: 0,
                min_age_days: 0,
                limit: 10,
            },
        );
        assert_eq!(report.candidates.len(), 1);
        let candidate = &report.candidates[0];
        assert_eq!(candidate.production_time_source, "filename-date");
        assert_eq!(date_parts(candidate.production_time_ms), (2025, 12, 10));
        assert!(candidate
            .review_reasons
            .contains(&"embedded-metadata-contains-geolocation".to_string()));
    }

    #[test]
    fn planner_defaults_missing_embedded_labels_and_tie_breaks_equal_sizes() {
        let source = PathBuf::from("/source");
        let embedded_ms = date_epoch_ms(2025, 1, 2).unwrap();
        let metadata = ContentMetadata {
            production_time_ms: Some(embedded_ms),
            ..ContentMetadata::default()
        };
        let report = plan_cloud_archive(
            &[
                FileFact {
                    path: source.join("b.pdf"),
                    bytes: 10,
                    created_ms: 1,
                    modified_ms: 1,
                    content_metadata: metadata.clone(),
                },
                FileFact {
                    path: source.join("a.pdf"),
                    bytes: 10,
                    created_ms: 1,
                    modified_ms: 1,
                    content_metadata: metadata,
                },
            ],
            &source,
            &root(CloudProvider::Icloud, Path::new("/cloud")),
            embedded_ms,
            CloudPlanOptions {
                min_size_bytes: 0,
                min_age_days: 0,
                limit: 10,
            },
        );
        assert_eq!(report.candidates.len(), 2);
        assert!(report.candidates[0].src.ends_with("a.pdf"));
        assert_eq!(
            report.candidates[0].production_time_source,
            "embedded:unknown"
        );
        assert_eq!(report.candidates[0].production_time_confidence, "medium");
    }

    #[test]
    fn provider_and_kind_cover_stable_wire_names() {
        assert_eq!(CloudPlanOptions::default().limit, 200);
        assert_eq!(CloudProvider::Icloud.as_str(), "icloud");
        assert_eq!(CloudProvider::Onedrive.as_str(), "onedrive");
        assert_eq!(CloudProvider::GoogleDrive.as_str(), "google-drive");
        for (ext, expected) in [
            ("x.pdf", ArchiveKind::Document),
            ("x.mp4", ArchiveKind::Media),
            ("x.zip", ArchiveKind::Archive),
            ("x.csv", ArchiveKind::Dataset),
            ("x.bak", ArchiveKind::Backup),
            ("x.psd", ArchiveKind::Creative),
        ] {
            assert_eq!(archive_kind(Path::new(ext)), Some(expected));
            assert!(!expected.folder().is_empty());
        }
        assert_eq!(archive_kind(Path::new("README")), None);
    }
}
