//! Cloud-offload discovery and dry-run planning.
//!
//! This module is intentionally local and deterministic: it never uploads, moves, deletes,
//! hydrates, or calls a model.  The plan preserves enough source metadata to become the first
//! lineage record for a later verified move.

#[cfg(not(coverage))]
use crate::content_digest::{ContentDigests, ContentHasher};
#[cfg(not(coverage))]
use crate::dataset_metadata::profile_dataset;
use crate::dataset_metadata::DatasetProfile;
#[cfg(not(coverage))]
use std::collections::BTreeMap;
use std::collections::BTreeSet;
#[cfg(not(coverage))]
use std::io::Read;
use std::path::{Path, PathBuf};
#[cfg(not(coverage))]
use std::process::{Command, Stdio};
#[cfg(not(coverage))]
use std::time::{Duration, Instant};
use unicode_normalization::UnicodeNormalization;

const ARCHIVE_DIR: &str = "DiskSage Archive";
const DAY_MS: u64 = 86_400_000;
#[cfg(not(coverage))]
const METADATA_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(not(coverage))]
const METADATA_PROBE_OUTPUT_LIMIT: usize = 1024 * 1024;

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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CloudAccountScope {
    Personal,
    Organization,
    Shared,
    #[default]
    Unknown,
}

impl CloudAccountScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Personal => "personal",
            Self::Organization => "organization",
            Self::Shared => "shared",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CloudRoot {
    pub id: String,
    pub provider: CloudProvider,
    pub account_scope: CloudAccountScope,
    pub label: String,
    pub path: String,
    /// Readability observed during the latest bounded discovery pass.
    ///
    /// This is only a snapshot. Every operation revalidates the directory before use.
    #[serde(default)]
    pub readable: bool,
    /// Stable, non-sensitive reason for a failed discovery-time access probe.
    #[serde(default)]
    pub access_issue: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CloudRootDiscoveryIssue {
    pub provider: Option<CloudProvider>,
    pub account_scope: CloudAccountScope,
    pub label: String,
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CloudRootDiscoveryReport {
    pub roots: Vec<CloudRoot>,
    pub issues: Vec<CloudRootDiscoveryIssue>,
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
    IncompleteDownload,
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
            Self::IncompleteDownload => "incomplete-downloads",
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
    pub dataset_profile: Option<DatasetProfile>,
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
    /// Stable digest of the metadata evidence shown to an operator for an approve/hold decision.
    pub review_fingerprint: String,
    pub src: String,
    pub dst: String,
    pub provider: CloudProvider,
    pub destination_account_scope: CloudAccountScope,
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
    pub dataset_profile: Option<DatasetProfile>,
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
    pub exact_duplicates: ExactDuplicateSummary,
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExactDuplicateSummary {
    pub cluster_count: usize,
    pub candidate_count: usize,
    pub candidate_bytes: u64,
    pub redundant_bytes: u64,
}

/// Fail closed when the selected source cannot be enumerated. Filesystem metadata alone is not
/// sufficient on platforms such as macOS where privacy controls may allow `stat` but deny
/// directory traversal.
pub fn validate_source_root_readable(root: &Path) -> Result<(), String> {
    if !root.is_dir() {
        return Err(format!("source-root-not-directory:{}", root.display()));
    }
    std::fs::read_dir(root)
        .map(|_| ())
        .map_err(|error| format!("source-root-unreadable:{}:{error}", root.display()))
}

/// Revalidate a selected destination immediately before it is used.
pub fn validate_cloud_root_readable(root: &CloudRoot) -> Result<(), String> {
    if !root.readable {
        return Err(format!(
            "cloud-root-unreadable:{}:{}",
            root.path,
            root.access_issue.as_deref().unwrap_or("not-verified")
        ));
    }
    std::fs::read_dir(&root.path)
        .map(|_| ())
        .map_err(|error| format!("cloud-root-unreadable:{}:{error}", root.path))
}

#[cfg(not(coverage))]
fn access_issue_for_error(error: &std::io::Error) -> String {
    match error.kind() {
        std::io::ErrorKind::PermissionDenied => "permission-denied",
        std::io::ErrorKind::NotFound => "not-found",
        std::io::ErrorKind::NotADirectory => "not-a-directory",
        _ => "read-dir-failed",
    }
    .into()
}

#[cfg(not(coverage))]
fn directory_access_issue(path: &Path) -> Option<String> {
    std::fs::read_dir(path)
        .err()
        .map(|error| access_issue_for_error(&error))
}

#[cfg(not(coverage))]
fn read_children_sorted(path: &Path, limit: usize) -> Result<Vec<PathBuf>, String> {
    let entries = std::fs::read_dir(path).map_err(|error| access_issue_for_error(&error))?;
    let mut children = Vec::new();
    for entry in entries.take(limit) {
        children.push(
            entry
                .map_err(|error| access_issue_for_error(&error))?
                .path(),
        );
    }
    children.sort();
    Ok(children)
}

#[cfg(not(coverage))]
fn push_discovery_issue(
    report: &mut CloudRootDiscoveryReport,
    provider: Option<CloudProvider>,
    account_scope: CloudAccountScope,
    path: &Path,
    label: String,
    reason: String,
) {
    report.issues.push(CloudRootDiscoveryIssue {
        provider,
        account_scope,
        label,
        path: path.to_string_lossy().into_owned(),
        reason,
    });
}

#[cfg(not(coverage))]
fn push_root(
    report: &mut CloudRootDiscoveryReport,
    seen: &mut BTreeSet<PathBuf>,
    provider: CloudProvider,
    account_scope: CloudAccountScope,
    path: PathBuf,
    label: String,
) {
    let metadata = match path.metadata() {
        Ok(metadata) if metadata.is_dir() => metadata,
        Ok(_) => {
            push_discovery_issue(
                report,
                Some(provider),
                account_scope,
                &path,
                label,
                "not-a-directory".into(),
            );
            return;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            push_discovery_issue(
                report,
                Some(provider),
                account_scope,
                &path,
                label,
                access_issue_for_error(&error),
            );
            return;
        }
    };
    if metadata.permissions().readonly() {
        push_discovery_issue(
            report,
            Some(provider),
            account_scope,
            &path,
            label,
            "read-only".into(),
        );
        return;
    }
    let identity = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    if !seen.insert(identity) {
        return;
    }
    let access_issue = directory_access_issue(&path);
    let readable = access_issue.is_none();
    let value = path.to_string_lossy().into_owned();
    report.roots.push(CloudRoot {
        id: value.clone(),
        provider,
        account_scope,
        label: label.clone(),
        path: value,
        readable,
        access_issue: access_issue.clone(),
    });
    if let Some(reason) = access_issue {
        push_discovery_issue(
            report,
            Some(provider),
            account_scope,
            &path,
            label,
            reason,
        );
    }
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

fn normalized_account_text(value: &str) -> String {
    value.nfc().flat_map(char::to_lowercase).collect()
}

fn account_email_scope(account: &str) -> CloudAccountScope {
    let normalized = normalized_account_text(account);
    let Some((_, domain)) = normalized.rsplit_once('@') else {
        return CloudAccountScope::Unknown;
    };
    if matches!(
        domain,
        "gmail.com" | "googlemail.com" | "outlook.com" | "hotmail.com" | "live.com"
    ) {
        CloudAccountScope::Personal
    } else if domain.contains('.') {
        CloudAccountScope::Organization
    } else {
        CloudAccountScope::Unknown
    }
}

fn account_scope(
    provider: CloudProvider,
    account: &str,
    drive_name: Option<&str>,
) -> CloudAccountScope {
    if provider == CloudProvider::Icloud {
        return CloudAccountScope::Unknown;
    }
    let account = normalized_account_text(account);
    if provider == CloudProvider::GoogleDrive {
        let drive = normalized_account_text(drive_name.unwrap_or_default());
        if contains_any(&drive, &["shared drive", "shared drives", "공유 드라이브"]) {
            return CloudAccountScope::Shared;
        }
        return account_email_scope(&account);
    }
    if contains_any(&account, &["personal", "consumer", "개인"]) {
        return CloudAccountScope::Personal;
    }
    let email_scope = account_email_scope(&account);
    if email_scope != CloudAccountScope::Unknown {
        return email_scope;
    }
    if matches!(account.as_str(), "" | "default" | "onedrive") {
        CloudAccountScope::Unknown
    } else {
        CloudAccountScope::Organization
    }
}

/// Discover permission-writable local File Provider roots without creating a probe file, and
/// attach a bounded readability snapshot so privacy-controlled destinations remain visible but
/// fail closed before selection.
///
/// Google Drive's account root is read-only on macOS, so each writable direct child (for
/// example "My Drive" or a writable shared drive) is surfaced as a separate destination.
#[cfg(not(coverage))]
pub fn discover_cloud_roots_report(home: &Path) -> CloudRootDiscoveryReport {
    let mut report = CloudRootDiscoveryReport::default();
    let mut seen = BTreeSet::new();

    push_root(
        &mut report,
        &mut seen,
        CloudProvider::Icloud,
        CloudAccountScope::Unknown,
        home.join("Library/Mobile Documents/com~apple~CloudDocs"),
        "iCloud Drive".into(),
    );
    push_root(
        &mut report,
        &mut seen,
        CloudProvider::Icloud,
        CloudAccountScope::Unknown,
        home.join("iCloudDrive"),
        "iCloud Drive".into(),
    );

    let cloud_storage = home.join("Library/CloudStorage");
    let account_roots = match read_children_sorted(&cloud_storage, 128) {
        Ok(account_roots) => account_roots,
        Err(reason) if reason == "not-found" => Vec::new(),
        Err(reason) => {
            push_discovery_issue(
                &mut report,
                None,
                CloudAccountScope::Unknown,
                &cloud_storage,
                "Cloud File Provider storage".into(),
                reason,
            );
            Vec::new()
        }
    };
    for account_root in account_roots {
        let name = account_root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name.starts_with("OneDrive-") {
            let account = provider_account_label("OneDrive-", &account_root);
            push_root(
                &mut report,
                &mut seen,
                CloudProvider::Onedrive,
                account_scope(CloudProvider::Onedrive, &account, None),
                account_root,
                format!("OneDrive · {account}"),
            );
        } else if name.starts_with("GoogleDrive-") {
            let account = provider_account_label("GoogleDrive-", &account_root);
            let scope = account_scope(CloudProvider::GoogleDrive, &account, None);
            let drives = match read_children_sorted(&account_root, 128) {
                Ok(drives) => drives,
                Err(reason) => {
                    push_discovery_issue(
                        &mut report,
                        Some(CloudProvider::GoogleDrive),
                        scope,
                        &account_root,
                        "Google Drive account".into(),
                        reason,
                    );
                    continue;
                }
            };
            for drive in drives {
                let drive_name = drive
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if drive_name.starts_with('.') {
                    continue;
                }
                push_root(
                    &mut report,
                    &mut seen,
                    CloudProvider::GoogleDrive,
                    account_scope(CloudProvider::GoogleDrive, &account, Some(&drive_name)),
                    drive,
                    format!("Google Drive · {account} · {drive_name}"),
                );
            }
        }
    }

    // Windows and older clients commonly place provider roots directly under the home folder.
    let home_children = match read_children_sorted(home, 128) {
        Ok(children) => children,
        Err(reason) => {
            push_discovery_issue(
                &mut report,
                None,
                CloudAccountScope::Unknown,
                home,
                "Home provider-root discovery".into(),
                reason,
            );
            Vec::new()
        }
    };
    for path in home_children {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name == "OneDrive" || name.starts_with("OneDrive - ") {
            push_root(
                &mut report,
                &mut seen,
                CloudProvider::Onedrive,
                account_scope(CloudProvider::Onedrive, &name, None),
                path,
                format!("OneDrive · {name}"),
            );
        } else if name == "Google Drive" || name.starts_with("Google Drive ") {
            push_root(
                &mut report,
                &mut seen,
                CloudProvider::GoogleDrive,
                account_scope(CloudProvider::GoogleDrive, &name, None),
                path,
                format!("Google Drive · {name}"),
            );
        }
    }

    report.roots.sort_by(|a, b| {
        (a.provider.as_str(), &a.label, &a.path).cmp(&(b.provider.as_str(), &b.label, &b.path))
    });
    report.issues.sort_by(|a, b| {
        (
            a.provider.map(CloudProvider::as_str).unwrap_or(""),
            a.account_scope.as_str(),
            &a.label,
            &a.path,
            &a.reason,
        )
            .cmp(&(
                b.provider.map(CloudProvider::as_str).unwrap_or(""),
                b.account_scope.as_str(),
                &b.label,
                &b.path,
                &b.reason,
            ))
    });
    report.issues.dedup();
    report
}

#[cfg(not(coverage))]
pub fn discover_cloud_roots(home: &Path) -> Vec<CloudRoot> {
    discover_cloud_roots_report(home).roots
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
        "crdownload" => Some(ArchiveKind::IncompleteDownload),
        _ if multipart_archive_part(path).is_some() => Some(ArchiveKind::Archive),
        _ => None,
    }
}

fn multipart_archive_part(path: &Path) -> Option<(String, u32)> {
    let name = path.file_name()?.to_string_lossy();
    let normalized = name.to_ascii_lowercase();
    let (base, part) = normalized.rsplit_once(".part")?;
    if !base.ends_with(".zip") || part.len() != 3 || !part.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Some((base.to_string(), part.parse().ok()?))
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

/// Extract common date tokens from a filename as a low-confidence provisional date hint.
/// Embedded metadata always wins, and this hint can never authorize a copy without review.
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

#[cfg(not(coverage))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MetadataProbeFailure {
    Spawn,
    Wait,
    Timeout,
    Exit,
    Read,
    OutputTooLarge,
    InvalidOutput,
}

#[cfg(not(coverage))]
impl MetadataProbeFailure {
    fn code(self) -> &'static str {
        match self {
            Self::Spawn => "spawn-failed",
            Self::Wait => "wait-failed",
            Self::Timeout => "timeout",
            Self::Exit => "nonzero-exit",
            Self::Read => "output-read-failed",
            Self::OutputTooLarge => "output-limit-exceeded",
            Self::InvalidOutput => "invalid-output",
        }
    }
}

#[cfg(not(coverage))]
fn run_metadata_command_with_limits(
    mut command: Command,
    timeout: Duration,
    output_limit: usize,
) -> Result<Vec<u8>, MetadataProbeFailure> {
    command.stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = command.spawn().map_err(|_| MetadataProbeFailure::Spawn)?;
    let mut stdout = child.stdout.take().ok_or(MetadataProbeFailure::Read)?;
    let output_reader = std::thread::spawn(move || {
        let mut retained = Vec::with_capacity(output_limit.min(64 * 1024));
        let mut truncated = false;
        let mut buffer = [0_u8; 8192];
        loop {
            let read = stdout
                .read(&mut buffer)
                .map_err(|_| MetadataProbeFailure::Read)?;
            if read == 0 {
                break;
            }
            let remaining = output_limit.saturating_sub(retained.len());
            let keep = remaining.min(read);
            retained.extend_from_slice(&buffer[..keep]);
            truncated |= keep < read;
        }
        Ok::<_, MetadataProbeFailure>((retained, truncated))
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = output_reader.join();
                return Err(MetadataProbeFailure::Timeout);
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = output_reader.join();
                return Err(MetadataProbeFailure::Wait);
            }
        }
    };
    let (stdout, truncated) = output_reader
        .join()
        .map_err(|_| MetadataProbeFailure::Read)??;
    if !status.success() {
        return Err(MetadataProbeFailure::Exit);
    }
    if truncated {
        return Err(MetadataProbeFailure::OutputTooLarge);
    }
    Ok(stdout)
}

#[cfg(not(coverage))]
fn run_metadata_command(command: Command) -> Result<Vec<u8>, MetadataProbeFailure> {
    run_metadata_command_with_limits(command, METADATA_PROBE_TIMEOUT, METADATA_PROBE_OUTPUT_LIMIT)
}

#[cfg(not(coverage))]
fn add_probe_warning(metadata: &mut ContentMetadata, tool: &str, failure: MetadataProbeFailure) {
    add_evidence(
        metadata,
        "metadata-probe-warning",
        format!("{tool}:{}", failure.code()),
        &format!("local:metadata-probe:{tool}"),
        "high",
    );
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

fn origin_host(value: &str) -> Option<String> {
    let (_, remainder) = value.trim().split_once("://")?;
    let authority = remainder.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit('@').next()?;
    let host = if authority.starts_with('[') {
        authority.split(']').next()?.trim_start_matches('[')
    } else {
        authority.split(':').next()?
    };
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

fn decode_hex_ascii(value: &[u8]) -> Option<Vec<u8>> {
    fn nibble(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }

    let digits: Vec<u8> = value
        .iter()
        .copied()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if digits.is_empty() || digits.len() % 2 != 0 {
        return None;
    }
    digits
        .chunks_exact(2)
        .map(|pair| Some((nibble(pair[0])? << 4) | nibble(pair[1])?))
        .collect()
}

fn quarantine_record(value: &str) -> Option<(u64, String)> {
    let mut fields = value.trim().split(';');
    let _flags = fields.next()?;
    let acquired_seconds = u64::from_str_radix(fields.next()?, 16).ok()?;
    let agent = fields.next()?.trim();
    if agent.is_empty() {
        return None;
    }
    Some((acquired_seconds, agent.to_string()))
}

#[cfg(all(not(coverage), target_os = "macos"))]
fn macos_file_provenance_metadata(path: &Path) -> ContentMetadata {
    let mut metadata = ContentMetadata::default();

    let mut where_froms = local_command("xattr");
    where_froms
        .args(["-px", "com.apple.metadata:kMDItemWhereFroms"])
        .arg(path);
    if let Ok(output) = run_metadata_command(where_froms) {
        if let Some(bytes) = decode_hex_ascii(&output) {
            if let Ok(plist::Value::Array(values)) =
                plist::Value::from_reader(std::io::Cursor::new(bytes))
            {
                let hosts: BTreeSet<String> = values
                    .iter()
                    .filter_map(plist::Value::as_string)
                    .filter_map(origin_host)
                    .collect();
                for host in hosts {
                    push_context(
                        &mut metadata,
                        "download-origin-host",
                        &host,
                        "filesystem:macos-where-froms",
                    );
                }
            }
        }
    }

    let mut quarantine = local_command("xattr");
    quarantine.args(["-p", "com.apple.quarantine"]).arg(path);
    if let Ok(output) = run_metadata_command(quarantine) {
        if let Some((acquired_seconds, agent)) =
            quarantine_record(&String::from_utf8_lossy(&output))
        {
            push_context(
                &mut metadata,
                "download-agent",
                &agent,
                "filesystem:macos-quarantine",
            );
            add_evidence(
                &mut metadata,
                "download-acquired-date",
                date_value(acquired_seconds.saturating_mul(1_000)),
                "filesystem:macos-quarantine",
                "medium",
            );
        }
    }
    metadata
}

#[cfg(all(not(coverage), not(target_os = "macos")))]
fn macos_file_provenance_metadata(_path: &Path) -> ContentMetadata {
    ContentMetadata::default()
}

#[cfg(not(coverage))]
fn exiftool_metadata(path: &Path) -> ContentMetadata {
    let mut metadata = ContentMetadata::default();
    let mut command = local_command("exiftool");
    command
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
            "-Application",
            "-AppVersion",
            "-Software",
            "-CreatorTool",
            "-Producer",
            "-Template",
            "-Duration",
            "-GPSLatitude",
            "-GPSLongitude",
            "-Location",
        ])
        .arg(path);
    let output = match run_metadata_command(command) {
        Ok(output) => output,
        Err(failure) => {
            add_probe_warning(&mut metadata, "exiftool", failure);
            return metadata;
        }
    };
    let document = match serde_json::from_slice::<Vec<serde_json::Value>>(&output) {
        Ok(document) => document,
        Err(_) => {
            add_probe_warning(
                &mut metadata,
                "exiftool",
                MetadataProbeFailure::InvalidOutput,
            );
            return metadata;
        }
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
    for key in [
        "Application",
        "AppVersion",
        "Software",
        "CreatorTool",
        "Producer",
        "Template",
    ] {
        for value in json_strings(values.get(key)) {
            push_context(
                &mut metadata,
                &format!("generator-{}", key.to_ascii_lowercase()),
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
    let mut command = local_command("ffprobe");
    command
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration:format_tags=creation_time,date,title,artist,comment,location",
            "-of",
            "json",
        ])
        .arg(path);
    let output = match run_metadata_command(command) {
        Ok(output) => output,
        Err(failure) => {
            add_probe_warning(&mut metadata, "ffprobe", failure);
            return metadata;
        }
    };
    let document = match serde_json::from_slice::<serde_json::Value>(&output) {
        Ok(document) => document,
        Err(_) => {
            add_probe_warning(
                &mut metadata,
                "ffprobe",
                MetadataProbeFailure::InvalidOutput,
            );
            return metadata;
        }
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
    let mut command = local_command("pdfinfo");
    command.arg(path);
    let output = match run_metadata_command(command) {
        Ok(output) => output,
        Err(failure) => {
            add_probe_warning(&mut metadata, "pdfinfo", failure);
            return metadata;
        }
    };
    let stdout = String::from_utf8_lossy(&output);
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
    let mut command = local_command("unzip");
    command.args(["-p"]).arg(path).arg(entry);
    let output = match run_metadata_command(command) {
        Ok(output) => output,
        Err(failure) => {
            add_probe_warning(&mut metadata, "unzip", failure);
            return metadata;
        }
    };
    let xml = String::from_utf8_lossy(&output);
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

#[cfg(not(coverage))]
fn zip_archive_metadata(path: &Path) -> ContentMetadata {
    let mut metadata = ContentMetadata::default();
    let mut command = local_command("zipinfo");
    command.arg("-h").arg(path);
    let output = match run_metadata_command(command) {
        Ok(output) => output,
        Err(failure) => {
            add_probe_warning(&mut metadata, "zipinfo", failure);
            return metadata;
        }
    };
    add_evidence(
        &mut metadata,
        "archive-index-status",
        "readable",
        "embedded:zip-central-directory",
        "high",
    );
    let stdout = String::from_utf8_lossy(&output);
    if let Some(entries) = stdout.lines().find_map(|line| {
        let (_, value) = line.split_once("number of entries:")?;
        value.trim().split_whitespace().next()?.parse::<u64>().ok()
    }) {
        add_evidence(
            &mut metadata,
            "archive-entry-count",
            entries.to_string(),
            "embedded:zip-central-directory",
            "high",
        );
    }
    metadata
}

#[cfg(not(coverage))]
fn multipart_archive_metadata(path: &Path) -> ContentMetadata {
    let mut metadata = ContentMetadata::default();
    let Some((base, current_part)) = multipart_archive_part(path) else {
        return metadata;
    };
    let present: BTreeSet<u32> = path
        .parent()
        .and_then(|parent| std::fs::read_dir(parent).ok())
        .into_iter()
        .flatten()
        .take(4_096)
        .filter_map(Result::ok)
        .filter_map(|entry| multipart_archive_part(&entry.path()))
        .filter_map(|(candidate_base, part)| (candidate_base == base).then_some(part))
        .collect();
    let max_part = present.iter().next_back().copied().unwrap_or(current_part);
    let missing: Vec<u32> = (0..=max_part)
        .filter(|part| !present.contains(part))
        .collect();
    add_evidence(
        &mut metadata,
        "multipart-archive-current-part",
        format!("{current_part:03}"),
        "filesystem:multipart-sibling-set",
        "high",
    );
    add_evidence(
        &mut metadata,
        "multipart-archive-present-parts",
        present
            .iter()
            .map(|part| format!("{part:03}"))
            .collect::<Vec<_>>()
            .join(","),
        "filesystem:multipart-sibling-set",
        "high",
    );
    if !missing.is_empty() {
        add_evidence(
            &mut metadata,
            "multipart-archive-missing-parts",
            missing
                .iter()
                .map(|part| format!("{part:03}"))
                .collect::<Vec<_>>()
                .join(","),
            "filesystem:multipart-sibling-set",
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
    if primary.dataset_profile.is_none() {
        primary.dataset_profile = secondary.dataset_profile;
    }
    primary.evidence.extend(secondary.evidence);
    primary
}

#[cfg(not(coverage))]
fn dataset_content_metadata(path: &Path) -> ContentMetadata {
    let mut metadata = ContentMetadata::default();
    let profile = profile_dataset(path);
    let source = format!("embedded:dataset-profile:{}", profile.format);
    add_evidence(
        &mut metadata,
        "dataset-format",
        profile.format.clone(),
        &source,
        "high",
    );
    add_evidence(
        &mut metadata,
        "dataset-sampled-rows",
        profile.sampled_rows.to_string(),
        &source,
        "medium",
    );
    add_evidence(
        &mut metadata,
        "dataset-column-count",
        profile.columns.len().to_string(),
        &source,
        "medium",
    );
    for column in &profile.columns {
        add_evidence(
            &mut metadata,
            "dataset-column",
            format!(
                "{}:{} observed={} missing={} sensitive-name={}",
                column.name,
                column.inferred_type,
                column.observed_values,
                column.missing_values,
                column.sensitive_name
            ),
            &source,
            "medium",
        );
    }
    for warning in &profile.quality_warnings {
        add_evidence(
            &mut metadata,
            "dataset-quality-warning",
            warning,
            &source,
            "high",
        );
    }
    metadata.dataset_profile = Some(profile);
    metadata
}

#[cfg(not(coverage))]
fn probe_content_metadata(path: &Path) -> ContentMetadata {
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    // Transient downloads and raw multipart members do not represent standalone payloads.
    // ExifTool can spend the full timeout trying to infer their format, so retain only the
    // lightweight acquisition/sibling-set evidence for these fail-closed candidates.
    let general = if should_probe_general_metadata(path) {
        exiftool_metadata(path)
    } else {
        ContentMetadata::default()
    };
    let format_specific = match extension.as_str() {
        "m4a" | "mp4" | "m4v" | "mov" | "mkv" | "avi" | "wav" | "mp3" | "flac" | "aiff" => {
            ffprobe_metadata(path)
        }
        "pdf" => pdfinfo_metadata(path),
        "zip" => zip_archive_metadata(path),
        "docx" | "xlsx" | "pptx" => zipped_document_metadata(path, "docProps/core.xml"),
        "odt" | "ods" | "odp" => zipped_document_metadata(path, "meta.xml"),
        "csv" | "tsv" | "parquet" | "feather" | "arrow" | "sav" | "sas7bdat" | "dta" | "rdata"
        | "rds" | "sqlite" | "sqlite3" | "db" | "sql" | "jsonl" => dataset_content_metadata(path),
        _ if multipart_archive_part(path).is_some() => multipart_archive_metadata(path),
        _ => ContentMetadata::default(),
    };
    merge_metadata(
        merge_metadata(general, format_specific),
        macos_file_provenance_metadata(path),
    )
}

fn should_probe_general_metadata(path: &Path) -> bool {
    archive_kind(path) != Some(ArchiveKind::IncompleteDownload)
        && multipart_archive_part(path).is_none()
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

fn normalized_metadata_text(metadata: &ContentMetadata) -> String {
    std::iter::once(metadata.title.as_deref().unwrap_or_default())
        .chain(metadata.authors.iter().map(String::as_str))
        .chain(metadata.context.iter().map(String::as_str))
        .flat_map(|value| value.nfc())
        .flat_map(char::to_lowercase)
        .collect()
}

fn contains_any(text: &str, terms: &[&str]) -> bool {
    terms.iter().any(|term| text.contains(term))
}

fn embedded_metadata_review_reasons(
    path: &Path,
    metadata: &ContentMetadata,
    production_time_ms: u64,
    filesystem_modified_ms: u64,
) -> Vec<String> {
    let mut reasons = Vec::new();
    let text = normalized_metadata_text(metadata);
    if contains_any(
        &text,
        &[
            "client",
            "customer",
            "employee",
            "personnel",
            "applicant",
            "resume",
            "patient",
            "고객",
            "직원",
            "인사",
            "입사지원",
            "이력서",
            "경력기술",
            "주민",
            "환자",
            "진료",
            "사유서",
            "시말서",
        ],
    ) {
        reasons.push("embedded-metadata-may-contain-personal-context".into());
    }
    if contains_any(
        &text,
        &[
            "confidential",
            "internal",
            "security",
            "contract",
            "evaluation",
            "hyosung",
            "내부",
            "보안",
            "계약",
            "평가",
            "실적",
            "업무망",
            "망분리",
        ],
    ) {
        reasons.push("embedded-metadata-context-may-be-confidential".into());
    }

    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let production_date = date_value(production_time_ms);
    let known_python_docx_default = production_date == "2013-12-23"
        && (text.contains("python-docx") || text.contains("generated by python-docx"));
    let known_powerpoint_template_default = extension == "pptx"
        && production_date == "2006-08-16"
        && (text.contains("powerpoint")
            || metadata
                .evidence
                .iter()
                .any(|evidence| evidence.source == "embedded:ooxml:created"));
    if known_python_docx_default || known_powerpoint_template_default {
        reasons.push("embedded-production-date-known-template-default".into());
    }
    if production_time_ms > filesystem_modified_ms.saturating_add(DAY_MS) {
        reasons.push("embedded-production-date-after-filesystem-modified".into());
    }
    if metadata
        .evidence
        .iter()
        .any(|evidence| evidence.field == "metadata-probe-warning")
    {
        reasons.push("embedded-metadata-probe-incomplete".into());
    }
    if metadata
        .evidence
        .iter()
        .any(|evidence| evidence.field == "download-origin-host")
    {
        reasons.push("download-origin-needs-destination-review".into());
    }
    reasons
}

fn review_reasons(path: &Path, kind: ArchiveKind) -> Vec<String> {
    let mut reasons = Vec::new();
    if matches!(kind, ArchiveKind::Archive | ArchiveKind::Backup) {
        reasons.push("opaque-container-content-uninspected".into());
    }
    if kind == ArchiveKind::Dataset {
        reasons.push("structured-data-may-contain-personal-data".into());
    }
    if kind == ArchiveKind::IncompleteDownload {
        reasons.push("incomplete-download-extension".into());
    }
    if multipart_archive_part(path).is_some() {
        reasons.push("multipart-archive-member".into());
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
        "직원",
        "employee",
        "인사",
        "personnel",
        "입사지원",
        "applicant",
        "이력서",
        "resume",
        "경력기술",
        "사유서",
        "시말서",
        "내부",
        "internal",
        "보안",
        "security",
        "평가",
        "evaluation",
        "실적",
        "업무망",
        "망분리",
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

fn destination_scope_review_reasons(
    scope: CloudAccountScope,
    existing_reasons: &[String],
) -> Vec<String> {
    let sensitive_context = existing_reasons.iter().any(|reason| {
        matches!(
            reason.as_str(),
            "opaque-container-content-uninspected"
                | "structured-data-may-contain-personal-data"
                | "recording-may-contain-sensitive-speech"
                | "filename-context-may-be-confidential"
                | "filename-contains-geolocation"
                | "embedded-metadata-may-contain-personal-context"
                | "embedded-metadata-context-may-be-confidential"
                | "embedded-metadata-contains-geolocation"
                | "dataset-schema-profile-missing"
                | "dataset-schema-profile-incomplete"
                | "dataset-sensitive-column-name-detected"
        )
    });
    match scope {
        CloudAccountScope::Unknown => vec!["destination-account-scope-unknown".into()],
        CloudAccountScope::Shared => vec!["shared-destination-access-needs-review".into()],
        CloudAccountScope::Personal if sensitive_context => {
            vec!["personal-cloud-sensitive-context-needs-explicit-approval".into()]
        }
        CloudAccountScope::Personal | CloudAccountScope::Organization => Vec::new(),
    }
}

fn planner_blocked_reason(
    path: &Path,
    kind: ArchiveKind,
    metadata: &ContentMetadata,
    destination: &Path,
) -> Option<String> {
    if destination.exists() {
        return Some("destination-exists".into());
    }
    if kind == ArchiveKind::IncompleteDownload {
        return Some("incomplete-download".into());
    }
    if multipart_archive_part(path).is_some() {
        return Some("multipart-archive-atomic-copy-required".into());
    }
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if extension == "zip"
        && metadata.evidence.iter().any(|evidence| {
            evidence.field == "metadata-probe-warning"
                && evidence.source == "local:metadata-probe:zipinfo"
        })
    {
        return Some("archive-index-unreadable".into());
    }
    None
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

#[cfg(not(coverage))]
fn hash_duplicate_candidate(path: &Path, expected_bytes: u64) -> Result<ContentDigests, String> {
    let before =
        std::fs::metadata(path).map_err(|_| "duplicate-content-metadata-unreadable".to_string())?;
    if !before.is_file() {
        return Err("duplicate-content-source-not-file".into());
    }
    if before.len() != expected_bytes {
        return Err("duplicate-content-size-changed".into());
    }
    let before_modified_ms = millis(before.modified());
    let mut source =
        std::fs::File::open(path).map_err(|_| "duplicate-content-open-failed".to_string())?;
    let mut hasher = ContentHasher::default();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let read = source
            .read(&mut buffer)
            .map_err(|_| "duplicate-content-read-failed".to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let after =
        std::fs::metadata(path).map_err(|_| "duplicate-content-metadata-unreadable".to_string())?;
    if after.len() != expected_bytes || millis(after.modified()) != before_modified_ms {
        return Err("duplicate-content-source-changed".into());
    }
    Ok(hasher.finalize())
}

#[cfg(not(coverage))]
fn push_candidate_evidence(
    candidate: &mut CloudCandidate,
    field: &str,
    value: impl Into<String>,
    source: &str,
    confidence: &str,
) {
    candidate.metadata_evidence.push(MetadataEvidence {
        field: field.into(),
        value: value.into(),
        source: source.into(),
        confidence: confidence.into(),
    });
}

/// Hash only non-blocked candidates that share a byte length. Exact duplicates remain movable,
/// but require an operator to select the canonical lineage instead of silently copying every path.
#[cfg(not(coverage))]
fn mark_exact_duplicate_candidates(candidates: &mut [CloudCandidate]) -> ExactDuplicateSummary {
    let mut summary = ExactDuplicateSummary::default();
    let mut by_size: BTreeMap<u64, Vec<usize>> = BTreeMap::new();
    for (index, candidate) in candidates.iter().enumerate() {
        if candidate.blocked_reason.is_none() {
            by_size.entry(candidate.bytes).or_default().push(index);
        }
    }

    for same_size in by_size.values().filter(|indices| indices.len() > 1) {
        let mut by_digest: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
        for &index in same_size {
            let candidate = &candidates[index];
            match hash_duplicate_candidate(Path::new(&candidate.src), candidate.bytes) {
                Ok(digests) => by_digest
                    .entry((digests.sha256, digests.blake3))
                    .or_default()
                    .push(index),
                Err(reason) => {
                    let candidate = &mut candidates[index];
                    candidate
                        .review_reasons
                        .push("exact-duplicate-content-probe-incomplete".into());
                    push_candidate_evidence(
                        candidate,
                        "metadata-probe-warning",
                        reason,
                        "local:content-hash",
                        "high",
                    );
                }
            }
        }

        for ((sha256, blake3), exact_matches) in by_digest
            .into_iter()
            .filter(|(_, indices)| indices.len() > 1)
        {
            let bytes_per_candidate = candidates[exact_matches[0]].bytes;
            summary.cluster_count += 1;
            summary.candidate_count += exact_matches.len();
            summary.candidate_bytes = summary.candidate_bytes.saturating_add(
                bytes_per_candidate.saturating_mul(exact_matches.len() as u64),
            );
            summary.redundant_bytes = summary.redundant_bytes.saturating_add(
                bytes_per_candidate.saturating_mul((exact_matches.len() - 1) as u64),
            );
            let candidate_count = exact_matches.len().to_string();
            for index in exact_matches {
                let candidate = &mut candidates[index];
                candidate
                    .review_reasons
                    .push("exact-duplicate-content-needs-canonical-selection".into());
                push_candidate_evidence(
                    candidate,
                    "exact-duplicate-content-sha256",
                    sha256.clone(),
                    "local:content-hash",
                    "high",
                );
                push_candidate_evidence(
                    candidate,
                    "exact-duplicate-content-blake3",
                    blake3.clone(),
                    "local:content-hash",
                    "high",
                );
                push_candidate_evidence(
                    candidate,
                    "exact-duplicate-candidate-count",
                    candidate_count.clone(),
                    "planner:exact-content-cluster",
                    "high",
                );
            }
        }
    }

    for candidate in candidates {
        candidate.review_reasons.sort();
        candidate.review_reasons.dedup();
        candidate.requires_review = !candidate.review_reasons.is_empty();
        candidate.review_fingerprint = candidate_review_fingerprint(candidate);
    }
    summary
}

fn hash_review_value(hasher: &mut blake3::Hasher, value: &[u8]) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value);
}

/// Bind an operator review to the exact metadata evidence and destination context they saw.
/// Volatile fields such as plan generation time and `age_days` are intentionally excluded.
pub fn candidate_review_fingerprint(candidate: &CloudCandidate) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-cloud-review-v1\0");
    for value in [
        candidate.metadata_fingerprint.as_bytes(),
        candidate.provider.as_str().as_bytes(),
        candidate.destination_account_scope.as_str().as_bytes(),
        candidate.src.as_bytes(),
        candidate.dst.as_bytes(),
        candidate.kind.folder().as_bytes(),
        candidate.production_time_source.as_bytes(),
        candidate.production_time_confidence.as_bytes(),
        candidate.source_root.as_bytes(),
        candidate.relative_path.as_bytes(),
        candidate.source_context.as_bytes(),
        if candidate.requires_review {
            b"1"
        } else {
            b"0"
        },
    ] {
        hash_review_value(&mut hasher, value);
    }
    hash_review_value(&mut hasher, &candidate.bytes.to_le_bytes());
    hash_review_value(&mut hasher, &candidate.created_ms.to_le_bytes());
    hash_review_value(&mut hasher, &candidate.modified_ms.to_le_bytes());
    hash_review_value(&mut hasher, &candidate.production_time_ms.to_le_bytes());
    for reason in &candidate.review_reasons {
        hash_review_value(&mut hasher, reason.as_bytes());
    }
    hash_review_value(
        &mut hasher,
        candidate
            .content_title
            .as_deref()
            .unwrap_or_default()
            .as_bytes(),
    );
    for author in &candidate.content_authors {
        hash_review_value(&mut hasher, author.as_bytes());
    }
    for context in &candidate.content_context {
        hash_review_value(&mut hasher, context.as_bytes());
    }
    hash_review_value(
        &mut hasher,
        &candidate.duration_ms.unwrap_or_default().to_le_bytes(),
    );
    let dataset = serde_json::to_vec(&candidate.dataset_profile).unwrap_or_default();
    hash_review_value(&mut hasher, &dataset);
    for evidence in &candidate.metadata_evidence {
        for value in [
            evidence.field.as_bytes(),
            evidence.value.as_bytes(),
            evidence.source.as_bytes(),
            evidence.confidence.as_bytes(),
        ] {
            hash_review_value(&mut hasher, value);
        }
    }
    hasher.finalize().to_hex().to_string()
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
                "filename-date-hint",
                date_value(value),
                "filename:path-token",
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
            // Without embedded metadata, an explicit filename date is the next provisional value
            // for archive-preview placement, followed by filesystem creation and modification.
            // Every non-embedded value remains low confidence and review-required.
            } else if let Some(filename_ms) = filename_ms {
                (filename_ms, "filename:path-token".into(), "low".into())
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
        let blocked_reason = planner_blocked_reason(&file.path, kind, &lineage_metadata, &dst);
        let source_context = relative
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".".into());
        let mut review_reasons = review_reasons(&file.path, kind);
        review_reasons.extend(embedded_metadata_review_reasons(
            &file.path,
            &lineage_metadata,
            production_time_ms,
            file.modified_ms,
        ));
        if kind == ArchiveKind::Dataset {
            match lineage_metadata.dataset_profile.as_ref() {
                None => review_reasons.push("dataset-schema-profile-missing".into()),
                Some(profile) => {
                    if !profile.profile_complete {
                        review_reasons.push("dataset-schema-profile-incomplete".into());
                    }
                    if profile.columns.iter().any(|column| column.sensitive_name) {
                        review_reasons.push("dataset-sensitive-column-name-detected".into());
                    }
                    if !profile.quality_warnings.is_empty() {
                        review_reasons.push("dataset-quality-warning-present".into());
                    }
                }
            }
        }
        if !production_time_source.starts_with("embedded:") {
            review_reasons.push("production-date-not-from-embedded-metadata".into());
        } else if production_time_confidence != "high" {
            review_reasons.push("embedded-production-date-confidence-not-high".into());
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
        review_reasons.extend(destination_scope_review_reasons(
            cloud_root.account_scope,
            &review_reasons,
        ));
        review_reasons.sort();
        review_reasons.dedup();
        let mut candidate = CloudCandidate {
            metadata_fingerprint: metadata_fingerprint(file, relative),
            review_fingerprint: String::new(),
            src: file.path.to_string_lossy().into_owned(),
            dst: dst.to_string_lossy().into_owned(),
            provider: cloud_root.provider,
            destination_account_scope: cloud_root.account_scope,
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
            dataset_profile: lineage_metadata.dataset_profile,
            metadata_evidence: lineage_metadata.evidence,
            blocked_reason,
        };
        candidate.review_fingerprint = candidate_review_fingerprint(&candidate);
        candidates.push(candidate);
    }
    #[cfg(not(coverage))]
    let exact_duplicates = mark_exact_duplicate_candidates(&mut candidates);
    #[cfg(coverage)]
    let exact_duplicates = ExactDuplicateSummary::default();
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
        exact_duplicates,
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
            account_scope: CloudAccountScope::Organization,
            label: "test".into(),
            path: path.to_string_lossy().into_owned(),
            readable: true,
            access_issue: None,
        }
    }

    #[cfg(all(not(coverage), unix))]
    #[test]
    fn metadata_probe_commands_bound_runtime_output_and_failures() {
        let mut ok = Command::new("sh");
        ok.args(["-c", "printf ok"]);
        assert_eq!(
            run_metadata_command_with_limits(ok, Duration::from_secs(1), 16).unwrap(),
            b"ok"
        );

        let mut oversized = Command::new("sh");
        oversized.args(["-c", "printf 0123456789"]);
        assert_eq!(
            run_metadata_command_with_limits(oversized, Duration::from_secs(1), 4),
            Err(MetadataProbeFailure::OutputTooLarge)
        );

        let mut slow = Command::new("sh");
        slow.args(["-c", "sleep 1"]);
        assert_eq!(
            run_metadata_command_with_limits(slow, Duration::from_millis(10), 16),
            Err(MetadataProbeFailure::Timeout)
        );

        let mut failed = Command::new("sh");
        failed.args(["-c", "exit 2"]);
        assert_eq!(
            run_metadata_command_with_limits(failed, Duration::from_secs(1), 16),
            Err(MetadataProbeFailure::Exit)
        );

        let missing = Command::new("/definitely/missing/disksage-probe");
        assert_eq!(
            run_metadata_command_with_limits(missing, Duration::from_secs(1), 16),
            Err(MetadataProbeFailure::Spawn)
        );
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
        writable_dir(&google.join("Shared drives"));
        writable_dir(&google.join(".Trash"));
        let roots = discover_cloud_roots(home);
        assert_eq!(roots.len(), 4);
        assert!(roots
            .iter()
            .all(|root| root.readable && root.access_issue.is_none()));
        assert!(roots.iter().any(|r| {
            r.provider == CloudProvider::Icloud && r.account_scope == CloudAccountScope::Unknown
        }));
        assert!(roots.iter().any(|r| {
            r.provider == CloudProvider::Onedrive && r.account_scope == CloudAccountScope::Personal
        }));
        assert!(roots.iter().any(|r| {
            r.provider == CloudProvider::GoogleDrive
                && r.account_scope == CloudAccountScope::Organization
                && r.path.ends_with("My Drive")
        }));
        assert!(roots.iter().any(|r| {
            r.provider == CloudProvider::GoogleDrive
                && r.account_scope == CloudAccountScope::Shared
                && r.path.ends_with("Shared drives")
        }));
        assert!(!roots.iter().any(|r| r.path.ends_with(".Trash")));
    }

    #[test]
    fn account_scope_classification_is_explicit_and_fail_closed() {
        assert_eq!(
            account_scope(CloudProvider::Icloud, "", None),
            CloudAccountScope::Unknown
        );
        assert_eq!(
            account_scope(CloudProvider::Onedrive, "개인", None),
            CloudAccountScope::Personal
        );
        assert_eq!(
            account_scope(CloudProvider::Onedrive, "Example Corp", None),
            CloudAccountScope::Organization
        );
        assert_eq!(
            account_scope(CloudProvider::Onedrive, "OneDrive", None),
            CloudAccountScope::Unknown
        );
        assert_eq!(
            account_scope(CloudProvider::GoogleDrive, "me@gmail.com", Some("My Drive")),
            CloudAccountScope::Personal
        );
        assert_eq!(
            account_scope(
                CloudProvider::GoogleDrive,
                "me@example.com",
                Some("My Drive")
            ),
            CloudAccountScope::Organization
        );
        assert_eq!(
            account_scope(
                CloudProvider::GoogleDrive,
                "me@example.com",
                Some("공유 드라이브")
            ),
            CloudAccountScope::Shared
        );
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
        let report = discover_cloud_roots_report(tmp.path());
        assert!(report.roots.is_empty());
        assert_eq!(report.issues.len(), 1);
        assert_eq!(report.issues[0].reason, "read-only");
    }

    #[cfg(all(unix, not(coverage)))]
    #[test]
    fn exposes_unreadable_provider_root_and_rejects_selection() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let one = tmp.path().join("OneDrive");
        writable_dir(&one);
        std::fs::set_permissions(&one, std::fs::Permissions::from_mode(0o300)).unwrap();

        let report = discover_cloud_roots_report(tmp.path());
        assert_eq!(report.roots.len(), 1);
        assert_eq!(report.issues.len(), 1);
        assert_eq!(report.issues[0].provider, Some(CloudProvider::Onedrive));
        assert_eq!(report.issues[0].reason, "permission-denied");
        assert!(!report.roots[0].readable);
        assert_eq!(
            report.roots[0].access_issue.as_deref(),
            Some("permission-denied")
        );
        assert_eq!(
            validate_cloud_root_readable(&report.roots[0]),
            Err(format!(
                "cloud-root-unreadable:{}:permission-denied",
                one.display()
            ))
        );

        std::fs::set_permissions(&one, std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    #[cfg(all(unix, not(coverage)))]
    #[test]
    fn reports_google_account_when_drive_children_cannot_be_enumerated() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().unwrap();
        let account = tmp
            .path()
            .join("Library/CloudStorage/GoogleDrive-me@example.com");
        writable_dir(&account);
        std::fs::set_permissions(&account, std::fs::Permissions::from_mode(0o300)).unwrap();

        let report = discover_cloud_roots_report(tmp.path());
        assert!(report.roots.is_empty());
        assert_eq!(report.issues.len(), 1);
        assert_eq!(
            report.issues[0].provider,
            Some(CloudProvider::GoogleDrive)
        );
        assert_eq!(
            report.issues[0].account_scope,
            CloudAccountScope::Organization
        );
        assert_eq!(report.issues[0].label, "Google Drive account");
        assert_eq!(report.issues[0].reason, "permission-denied");

        std::fs::set_permissions(&account, std::fs::Permissions::from_mode(0o700)).unwrap();
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
    fn source_root_preflight_distinguishes_readable_directory_from_file() {
        let temp = tempfile::tempdir().unwrap();
        assert_eq!(validate_source_root_readable(temp.path()), Ok(()));
        let file = temp.path().join("report.pdf");
        std::fs::write(&file, b"pdf").unwrap();
        assert_eq!(
            validate_source_root_readable(&file),
            Err(format!("source-root-not-directory:{}", file.display()))
        );
    }

    #[test]
    fn local_download_provenance_parsers_keep_hosts_and_acquisition_separate() {
        assert_eq!(
            origin_host("https://GW.Example.com/path?q=secret"),
            Some("gw.example.com".into())
        );
        assert_eq!(origin_host("file:///private/tmp/report.pdf"), None);
        assert_eq!(decode_hex_ascii(b"62 70\n6c69"), Some(b"bpli".to_vec()));
        assert_eq!(decode_hex_ascii(b"xyz"), None);
        assert_eq!(
            quarantine_record("0081;65F00A10;Edge;opaque-id"),
            Some((0x65F00A10, "Edge".into()))
        );
        assert!(!should_probe_general_metadata(Path::new(
            "unknown.crdownload"
        )));
        assert!(!should_probe_general_metadata(Path::new(
            "bundle.zip.part004"
        )));
        assert!(should_probe_general_metadata(Path::new("complete.zip")));
    }

    #[cfg(not(coverage))]
    #[test]
    fn multipart_archive_metadata_reports_internal_gaps_without_reading_payloads() {
        let tmp = tempfile::tempdir().unwrap();
        for part in [0, 1, 3, 4] {
            std::fs::write(
                tmp.path().join(format!("bundle.zip.part{part:03}")),
                b"part",
            )
            .unwrap();
        }
        let metadata = multipart_archive_metadata(&tmp.path().join("bundle.zip.part004"));
        assert!(metadata.evidence.iter().any(|evidence| {
            evidence.field == "multipart-archive-present-parts"
                && evidence.value == "000,001,003,004"
        }));
        assert!(metadata.evidence.iter().any(|evidence| {
            evidence.field == "multipart-archive-missing-parts" && evidence.value == "002"
        }));
    }

    #[test]
    fn incomplete_and_unreadable_download_artifacts_are_non_overridable_planner_blocks() {
        let destination = Path::new("/definitely/missing/disksage-destination");
        assert_eq!(
            planner_blocked_reason(
                Path::new("unknown.crdownload"),
                ArchiveKind::IncompleteDownload,
                &ContentMetadata::default(),
                destination,
            )
            .as_deref(),
            Some("incomplete-download")
        );
        assert_eq!(
            planner_blocked_reason(
                Path::new("bundle.zip.part003"),
                ArchiveKind::Archive,
                &ContentMetadata::default(),
                destination,
            )
            .as_deref(),
            Some("multipart-archive-atomic-copy-required")
        );
        let mut metadata = ContentMetadata::default();
        metadata.evidence.push(MetadataEvidence {
            field: "metadata-probe-warning".into(),
            value: "zipinfo:nonzero-exit".into(),
            source: "local:metadata-probe:zipinfo".into(),
            confidence: "high".into(),
        });
        assert_eq!(
            planner_blocked_reason(
                Path::new("broken.zip"),
                ArchiveKind::Archive,
                &metadata,
                destination,
            )
            .as_deref(),
            Some("archive-index-unreadable")
        );
    }

    #[test]
    fn filename_date_parser_recognizes_low_confidence_review_tokens() {
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
            dataset_profile: None,
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
            dataset_profile: Some(DatasetProfile {
                format: "csv".into(),
                profile_complete: true,
                ..DatasetProfile::default()
            }),
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
        assert_eq!(merged.dataset_profile.unwrap().format, "csv");
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
                    dataset_profile: None,
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
            evidence.field == "filename-date-hint"
                && evidence.source == "filename:path-token"
                && evidence.confidence == "low"
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
        let personnel = review_reasons(Path::new("직원_실적데이터.xlsx"), ArchiveKind::Document);
        assert!(personnel.contains(&"filename-context-may-be-confidential".to_string()));
    }

    #[test]
    fn destination_scope_review_is_transparent_and_fail_closed() {
        let sensitive = vec!["recording-may-contain-sensitive-speech".into()];
        assert_eq!(
            destination_scope_review_reasons(CloudAccountScope::Personal, &sensitive),
            ["personal-cloud-sensitive-context-needs-explicit-approval"]
        );
        assert!(
            destination_scope_review_reasons(CloudAccountScope::Organization, &sensitive)
                .is_empty()
        );
        assert_eq!(
            destination_scope_review_reasons(CloudAccountScope::Shared, &[]),
            ["shared-destination-access-needs-review"]
        );
        assert_eq!(
            destination_scope_review_reasons(CloudAccountScope::Unknown, &[]),
            ["destination-account-scope-unknown"]
        );
    }

    #[test]
    fn embedded_metadata_flags_sensitive_context_and_known_office_template_dates() {
        let python_default = date_epoch_ms(2013, 12, 23).unwrap();
        let metadata = ContentMetadata {
            authors: vec!["python-docx".into()],
            context: vec![
                "description=generated by python-docx".into(),
                "subject=직원 평가 자료".into(),
            ],
            ..ContentMetadata::default()
        };
        let reasons = embedded_metadata_review_reasons(
            Path::new("report.docx"),
            &metadata,
            python_default,
            date_epoch_ms(2026, 6, 1).unwrap(),
        );
        for expected in [
            "embedded-metadata-may-contain-personal-context",
            "embedded-metadata-context-may-be-confidential",
            "embedded-production-date-known-template-default",
        ] {
            assert!(reasons.contains(&expected.to_string()), "{expected}");
        }

        let powerpoint_default = date_epoch_ms(2006, 8, 16).unwrap();
        let metadata = ContentMetadata {
            evidence: vec![MetadataEvidence {
                field: "production-date".into(),
                value: "2006-08-16".into(),
                source: "embedded:ooxml:created".into(),
                confidence: "high".into(),
            }],
            ..ContentMetadata::default()
        };
        assert!(embedded_metadata_review_reasons(
            Path::new("slides.pptx"),
            &metadata,
            powerpoint_default,
            date_epoch_ms(2026, 6, 1).unwrap(),
        )
        .contains(&"embedded-production-date-known-template-default".to_string()));

        assert!(embedded_metadata_review_reasons(
            Path::new("future.pdf"),
            &ContentMetadata::default(),
            date_epoch_ms(2026, 7, 3).unwrap(),
            date_epoch_ms(2026, 7, 1).unwrap(),
        )
        .contains(&"embedded-production-date-after-filesystem-modified".to_string()));

        let incomplete_probe = ContentMetadata {
            evidence: vec![MetadataEvidence {
                field: "metadata-probe-warning".into(),
                value: "pdfinfo:timeout".into(),
                source: "local:metadata-probe:pdfinfo".into(),
                confidence: "high".into(),
            }],
            ..ContentMetadata::default()
        };
        assert!(embedded_metadata_review_reasons(
            Path::new("report.pdf"),
            &incomplete_probe,
            date_epoch_ms(2026, 6, 1).unwrap(),
            date_epoch_ms(2026, 6, 2).unwrap(),
        )
        .contains(&"embedded-metadata-probe-incomplete".to_string()));
    }

    #[cfg(not(coverage))]
    #[test]
    fn planner_profiles_dataset_schema_without_retaining_cell_values() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let cloud = tmp.path().join("cloud");
        writable_dir(&source);
        writable_dir(&cloud);
        let path = source.join("2026-01-01-data.csv");
        let contents = b"customer_email,amount,active\nalice@example.com,42,true\n";
        std::fs::File::create(&path)
            .unwrap()
            .write_all(contents)
            .unwrap();
        let modified_ms = date_epoch_ms(2026, 1, 2).unwrap();
        let report = plan_cloud_archive(
            &[FileFact {
                path,
                bytes: contents.len() as u64,
                created_ms: modified_ms,
                modified_ms,
                content_metadata: ContentMetadata::default(),
            }],
            &source,
            &root(CloudProvider::GoogleDrive, &cloud),
            modified_ms + 200 * DAY_MS,
            CloudPlanOptions {
                min_size_bytes: 0,
                min_age_days: 1,
                limit: 10,
            },
        );

        let candidate = &report.candidates[0];
        let profile = candidate.dataset_profile.as_ref().unwrap();
        assert_eq!(profile.format, "csv");
        assert_eq!(profile.sampled_rows, 1);
        assert_eq!(profile.columns[0].name, "customer_email");
        assert!(candidate
            .review_reasons
            .contains(&"dataset-sensitive-column-name-detected".to_string()));
        let evidence = serde_json::to_string(&candidate.metadata_evidence).unwrap();
        assert!(!evidence.contains("alice@example.com"));
        assert!(!evidence.contains("42"));
    }

    #[cfg(not(coverage))]
    #[test]
    fn planner_requires_canonical_selection_for_exact_duplicate_candidates() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let cloud = tmp.path().join("cloud");
        writable_dir(&source);
        writable_dir(&cloud);
        for (name, contents) in [
            ("a.pdf", &b"same-content"[..]),
            ("b.pdf", &b"same-content"[..]),
            ("c.pdf", &b"uniq-content"[..]),
        ] {
            std::fs::write(source.join(name), contents).unwrap();
        }
        let production_time_ms = date_epoch_ms(2026, 1, 2).unwrap();
        let metadata = ContentMetadata {
            production_time_ms: Some(production_time_ms),
            production_time_source: Some("embedded:test:creation-date".into()),
            production_time_confidence: Some("high".into()),
            ..ContentMetadata::default()
        };
        let files: Vec<_> = ["a.pdf", "b.pdf", "c.pdf"]
            .into_iter()
            .map(|name| {
                let path = source.join(name);
                let file_metadata = std::fs::metadata(&path).unwrap();
                FileFact {
                    path,
                    bytes: file_metadata.len(),
                    created_ms: millis(file_metadata.created()),
                    modified_ms: millis(file_metadata.modified()),
                    content_metadata: metadata.clone(),
                }
            })
            .collect();

        let report = plan_cloud_archive(
            &files,
            &source,
            &root(CloudProvider::GoogleDrive, &cloud),
            system_now_ms() + DAY_MS,
            CloudPlanOptions {
                min_size_bytes: 0,
                min_age_days: 0,
                limit: 10,
            },
        );

        let mut duplicate_hashes = Vec::new();
        for name in ["a.pdf", "b.pdf"] {
            let candidate = report
                .candidates
                .iter()
                .find(|candidate| candidate.relative_path == name)
                .unwrap();
            assert!(candidate.requires_review);
            assert!(candidate
                .review_reasons
                .contains(&"exact-duplicate-content-needs-canonical-selection".to_string()));
            assert!(candidate.metadata_evidence.iter().any(|evidence| {
                evidence.field == "exact-duplicate-candidate-count" && evidence.value == "2"
            }));
            duplicate_hashes.push(
                candidate
                    .metadata_evidence
                    .iter()
                    .find(|evidence| evidence.field == "exact-duplicate-content-sha256")
                    .unwrap()
                    .value
                    .clone(),
            );
        }
        assert_eq!(duplicate_hashes[0], duplicate_hashes[1]);
        assert_eq!(report.exact_duplicates.cluster_count, 1);
        assert_eq!(report.exact_duplicates.candidate_count, 2);
        assert_eq!(report.exact_duplicates.candidate_bytes, 24);
        assert_eq!(report.exact_duplicates.redundant_bytes, 12);
        let unique = report
            .candidates
            .iter()
            .find(|candidate| candidate.relative_path == "c.pdf")
            .unwrap();
        assert!(!unique
            .review_reasons
            .contains(&"exact-duplicate-content-needs-canonical-selection".to_string()));
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
                    content_metadata: ContentMetadata {
                        dataset_profile: Some(DatasetProfile {
                            format: "csv".into(),
                            sampled_rows: 2,
                            profile_complete: true,
                            columns: vec![crate::dataset_metadata::DatasetColumnProfile {
                                name: "customer_email".into(),
                                inferred_type: "text".into(),
                                observed_values: 2,
                                missing_values: 0,
                                sensitive_name: true,
                            }],
                            ..DatasetProfile::default()
                        }),
                        ..ContentMetadata::default()
                    },
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
        assert!(report.candidates[1]
            .review_reasons
            .contains(&"dataset-sensitive-column-name-detected".to_string()));
        assert_eq!(
            report.candidates[1]
                .dataset_profile
                .as_ref()
                .unwrap()
                .columns[0]
                .name,
            "customer_email"
        );
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
    fn planner_uses_filename_date_only_as_review_required_provisional_value() {
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
                    created_ms: date_epoch_ms(2025, 11, 9).unwrap(),
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
        assert_eq!(candidate.production_time_source, "filename:path-token");
        assert_eq!(candidate.production_time_confidence, "low");
        assert_eq!(date_parts(candidate.production_time_ms), (2025, 12, 10));
        assert!(candidate.requires_review);
        assert!(candidate
            .review_reasons
            .contains(&"production-date-not-from-embedded-metadata".to_string()));
        assert!(candidate.metadata_evidence.iter().any(|evidence| {
            evidence.field == "filename-date-hint"
                && evidence.value == "2025-12-10"
                && evidence.source == "filename:path-token"
        }));
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
        assert!(report.candidates[0].requires_review);
        assert!(report.candidates[0]
            .review_reasons
            .contains(&"embedded-production-date-confidence-not-high".to_string()));
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
            ("x.crdownload", ArchiveKind::IncompleteDownload),
            ("x.zip.part004", ArchiveKind::Archive),
        ] {
            assert_eq!(archive_kind(Path::new(ext)), Some(expected));
            assert!(!expected.folder().is_empty());
        }
        assert_eq!(archive_kind(Path::new("x.zip.part04")), None);
        assert_eq!(archive_kind(Path::new("README")), None);
    }
}
