//! Bounded, read-only duplicate-file evidence.
//!
//! Content equality is established with three streaming digests after a size and prefix filter.
//! Paths and filesystem timestamps remain private evidence. No duplicate is selected for deletion,
//! and neither filename dates nor filesystem dates are promoted to production-time evidence.

use crate::content_digest::{digest_file, ContentDigests};
use crate::dupes::hash_prefix;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const DUPLICATE_AUDIT_VERSION: u32 = 1;
pub const DUPLICATE_AUDIT_SCHEMA_KIND: &str = "disksage.duplicate-audit/v1";
pub const DEFAULT_DUPLICATE_MIN_FILE_BYTES: u64 = 1024 * 1024;
pub const DEFAULT_DUPLICATE_PREFIX_BYTES: usize = 64 * 1024;
pub const DEFAULT_DUPLICATE_MAX_ENTRIES: usize = 200_000;
pub const DEFAULT_DUPLICATE_MAX_DURATION_MS: u64 = 120_000;
pub const DEFAULT_DUPLICATE_MAX_FILES_TO_HASH: usize = 20_000;
pub const DEFAULT_DUPLICATE_MAX_SIZE_GROUPS: usize = 10_000;
pub const DEFAULT_DUPLICATE_MAX_HASH_BYTES: u64 = 64 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateAuditOptions {
    pub min_file_bytes: u64,
    pub prefix_bytes: usize,
    pub max_entries: usize,
    pub max_duration_ms: u64,
    pub max_files_to_hash: usize,
    pub max_size_groups: usize,
    pub max_hash_bytes: u64,
}

impl Default for DuplicateAuditOptions {
    fn default() -> Self {
        Self {
            min_file_bytes: DEFAULT_DUPLICATE_MIN_FILE_BYTES,
            prefix_bytes: DEFAULT_DUPLICATE_PREFIX_BYTES,
            max_entries: DEFAULT_DUPLICATE_MAX_ENTRIES,
            max_duration_ms: DEFAULT_DUPLICATE_MAX_DURATION_MS,
            max_files_to_hash: DEFAULT_DUPLICATE_MAX_FILES_TO_HASH,
            max_size_groups: DEFAULT_DUPLICATE_MAX_SIZE_GROUPS,
            max_hash_bytes: DEFAULT_DUPLICATE_MAX_HASH_BYTES,
        }
    }
}

impl DuplicateAuditOptions {
    fn validate(&self) -> Result<(), String> {
        if self.min_file_bytes == 0
            || self.prefix_bytes == 0
            || self.prefix_bytes > 16 * 1024 * 1024
            || self.max_entries == 0
            || self.max_entries > 2_000_000
            || self.max_duration_ms == 0
            || self.max_duration_ms > 15 * 60_000
            || self.max_files_to_hash < 2
            || self.max_files_to_hash > 200_000
            || self.max_size_groups == 0
            || self.max_size_groups > 100_000
            || self.max_hash_bytes == 0
        {
            return Err("duplicate-audit-options-unbounded-or-invalid".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DuplicateAuditFile {
    pub path: String,
    pub relative_path: String,
    pub path_fingerprint: String,
    pub logical_bytes: u64,
    pub allocated_bytes: u64,
    pub filesystem_created_ms: Option<u64>,
    pub filesystem_modified_ms: Option<u64>,
    pub device: u64,
    pub inode: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DuplicateAuditGroup {
    pub group_fingerprint: String,
    pub logical_bytes_per_file: u64,
    pub path_count: usize,
    pub unique_file_count: usize,
    pub hardlink_alias_count: usize,
    pub reclaimable_logical_bytes: u64,
    pub reclaimable_allocated_upper_bound_bytes: u64,
    pub content_digests: ContentDigests,
    pub files: Vec<DuplicateAuditFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DuplicateAuditReport {
    pub version: u32,
    pub schema_kind: String,
    pub observed_at_ms: u64,
    pub source_root: String,
    pub source_scope_fingerprint: String,
    pub report_fingerprint: String,
    pub options: DuplicateAuditOptionsSnapshot,
    pub entries_seen: usize,
    pub eligible_file_count: usize,
    pub equal_size_candidate_file_count: usize,
    pub equal_size_candidate_group_count: usize,
    pub hashed_file_count: usize,
    pub hashed_bytes: u64,
    pub duplicate_group_count: usize,
    pub duplicate_path_count: usize,
    pub duplicate_unique_file_count: usize,
    pub hardlink_alias_count: usize,
    pub reclaimable_logical_bytes: u64,
    pub reclaimable_allocated_upper_bound_bytes: u64,
    pub evidence_complete: bool,
    pub context_metadata_complete: bool,
    pub evidence_gap_count: usize,
    pub issue_counts: BTreeMap<String, u64>,
    pub groups: Vec<DuplicateAuditGroup>,
    pub automatic_discard_allowed: bool,
    pub human_context_review_required: bool,
    pub mutation_performed: bool,
    pub filename_date_used_as_production_time: bool,
    pub filesystem_times_used_as_production_time: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DuplicateAuditOptionsSnapshot {
    pub min_file_bytes: u64,
    pub prefix_bytes: usize,
    pub max_entries: usize,
    pub max_duration_ms: u64,
    pub max_files_to_hash: usize,
    pub max_size_groups: usize,
    pub max_hash_bytes: u64,
}

impl From<&DuplicateAuditOptions> for DuplicateAuditOptionsSnapshot {
    fn from(value: &DuplicateAuditOptions) -> Self {
        Self {
            min_file_bytes: value.min_file_bytes,
            prefix_bytes: value.prefix_bytes,
            max_entries: value.max_entries,
            max_duration_ms: value.max_duration_ms,
            max_files_to_hash: value.max_files_to_hash,
            max_size_groups: value.max_size_groups,
            max_hash_bytes: value.max_hash_bytes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DuplicateAuditSummary {
    pub version: u32,
    pub schema_kind: String,
    pub observed_at_ms: u64,
    pub source_scope_fingerprint: String,
    pub report_fingerprint: String,
    pub entries_seen: usize,
    pub eligible_file_count: usize,
    pub equal_size_candidate_file_count: usize,
    pub equal_size_candidate_group_count: usize,
    pub hashed_file_count: usize,
    pub hashed_bytes: u64,
    pub duplicate_group_count: usize,
    pub duplicate_path_count: usize,
    pub duplicate_unique_file_count: usize,
    pub hardlink_alias_count: usize,
    pub reclaimable_logical_bytes: u64,
    pub reclaimable_allocated_upper_bound_bytes: u64,
    pub evidence_complete: bool,
    pub context_metadata_complete: bool,
    pub evidence_gap_count: usize,
    pub issue_counts: BTreeMap<String, u64>,
    pub automatic_discard_allowed: bool,
    pub human_context_review_required: bool,
    pub mutation_performed: bool,
    pub filename_date_used_as_production_time: bool,
    pub filesystem_times_used_as_production_time: bool,
    pub content_digest_algorithms: Vec<String>,
    pub local_paths_redacted: bool,
    pub content_digests_redacted: bool,
    pub metadata_semantics: Vec<String>,
    pub notices: Vec<String>,
}

#[derive(Debug, Clone)]
struct ObservedFile {
    path: PathBuf,
    relative_path: String,
    size: u64,
    allocated_bytes: u64,
    created_ms: Option<u64>,
    modified_ms: Option<u64>,
    device: u64,
    inode: u64,
}

fn system_time_ms(value: SystemTime) -> Option<u64> {
    value
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| u64::try_from(duration.as_millis()).ok())
}

#[cfg(unix)]
fn file_identity(metadata: &std::fs::Metadata) -> (u64, u64, u64) {
    use std::os::unix::fs::MetadataExt;
    (
        metadata.dev(),
        metadata.ino(),
        metadata.blocks().saturating_mul(512),
    )
}

#[cfg(not(unix))]
fn file_identity(metadata: &std::fs::Metadata) -> (u64, u64, u64) {
    (0, 0, metadata.len())
}

fn hash_field(hasher: &mut blake3::Hasher, value: &str) {
    hasher.update(&(value.len() as u64).to_le_bytes());
    hasher.update(value.as_bytes());
}

fn source_scope_fingerprint(root: &Path) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.duplicate-audit-source\0v1\0");
    hash_field(&mut hasher, &root.to_string_lossy());
    hasher.finalize().to_hex().to_string()
}

fn path_fingerprint(scope: &str, relative_path: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.duplicate-audit-path\0v1\0");
    hash_field(&mut hasher, scope);
    hash_field(&mut hasher, relative_path);
    hasher.finalize().to_hex().to_string()
}

fn group_fingerprint(
    scope: &str,
    digests: &ContentDigests,
    files: &[DuplicateAuditFile],
) -> String {
    let mut paths: Vec<_> = files
        .iter()
        .map(|file| file.path_fingerprint.as_str())
        .collect();
    paths.sort_unstable();
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.duplicate-audit-group\0v1\0");
    for value in [
        scope,
        digests.blake3.as_str(),
        digests.sha256.as_str(),
        digests.quick_xor_base64.as_str(),
    ] {
        hash_field(&mut hasher, value);
    }
    for path in paths {
        hash_field(&mut hasher, path);
    }
    hasher.finalize().to_hex().to_string()
}

fn report_fingerprint(
    scope: &str,
    options: &DuplicateAuditOptions,
    evidence_complete: bool,
    groups: &[DuplicateAuditGroup],
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.duplicate-audit-report\0v1\0");
    hash_field(&mut hasher, scope);
    for value in [
        options.min_file_bytes,
        options.prefix_bytes as u64,
        options.max_entries as u64,
        options.max_duration_ms,
        options.max_files_to_hash as u64,
        options.max_size_groups as u64,
        options.max_hash_bytes,
    ] {
        hasher.update(&value.to_le_bytes());
    }
    hasher.update(&[u8::from(evidence_complete)]);
    for group in groups {
        hash_field(&mut hasher, &group.group_fingerprint);
        hasher.update(&group.reclaimable_logical_bytes.to_le_bytes());
        hasher.update(&group.reclaimable_allocated_upper_bound_bytes.to_le_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn canonical_real_directory(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("duplicate-audit-root-must-be-absolute-without-parent-traversal".into());
    }
    let symlink_metadata =
        std::fs::symlink_metadata(path).map_err(|_| "duplicate-audit-root-unavailable")?;
    if !symlink_metadata.is_dir() || symlink_metadata.file_type().is_symlink() {
        return Err("duplicate-audit-root-unsafe".into());
    }
    std::fs::canonicalize(path).map_err(|_| "duplicate-audit-root-unavailable".into())
}

fn increment_issue(issues: &mut BTreeMap<String, u64>, issue: &str) {
    *issues.entry(issue.to_string()).or_default() += 1;
}

fn elapsed_exceeded(started: Instant, max_duration_ms: u64) -> bool {
    started.elapsed() > Duration::from_millis(max_duration_ms)
}

fn metadata_matches(observed: &ObservedFile, metadata: &std::fs::Metadata) -> bool {
    let (device, inode, allocated_bytes) = file_identity(metadata);
    metadata.is_file()
        && !metadata.file_type().is_symlink()
        && metadata.len() == observed.size
        && device == observed.device
        && inode == observed.inode
        && allocated_bytes == observed.allocated_bytes
        && metadata.modified().ok().and_then(system_time_ms) == observed.modified_ms
}

fn private_file(scope: &str, observed: &ObservedFile) -> DuplicateAuditFile {
    DuplicateAuditFile {
        path: observed.path.to_string_lossy().into_owned(),
        relative_path: observed.relative_path.clone(),
        path_fingerprint: path_fingerprint(scope, &observed.relative_path),
        logical_bytes: observed.size,
        allocated_bytes: observed.allocated_bytes,
        filesystem_created_ms: observed.created_ms,
        filesystem_modified_ms: observed.modified_ms,
        device: observed.device,
        inode: observed.inode,
    }
}

fn empty_report(
    root: &Path,
    options: &DuplicateAuditOptions,
    observed_at_ms: u64,
    entries_seen: usize,
    eligible_file_count: usize,
    candidate_file_count: usize,
    candidate_group_count: usize,
    issues: BTreeMap<String, u64>,
    context_metadata_complete: bool,
) -> DuplicateAuditReport {
    let scope = source_scope_fingerprint(root);
    let evidence_gap_count = issues.values().copied().sum::<u64>() as usize;
    let groups = Vec::new();
    let fingerprint = report_fingerprint(&scope, options, false, &groups);
    DuplicateAuditReport {
        version: DUPLICATE_AUDIT_VERSION,
        schema_kind: DUPLICATE_AUDIT_SCHEMA_KIND.into(),
        observed_at_ms,
        source_root: root.to_string_lossy().into_owned(),
        source_scope_fingerprint: scope,
        report_fingerprint: fingerprint,
        options: options.into(),
        entries_seen,
        eligible_file_count,
        equal_size_candidate_file_count: candidate_file_count,
        equal_size_candidate_group_count: candidate_group_count,
        hashed_file_count: 0,
        hashed_bytes: 0,
        duplicate_group_count: 0,
        duplicate_path_count: 0,
        duplicate_unique_file_count: 0,
        hardlink_alias_count: 0,
        reclaimable_logical_bytes: 0,
        reclaimable_allocated_upper_bound_bytes: 0,
        evidence_complete: false,
        context_metadata_complete,
        evidence_gap_count,
        issue_counts: issues,
        groups,
        automatic_discard_allowed: false,
        human_context_review_required: true,
        mutation_performed: false,
        filename_date_used_as_production_time: false,
        filesystem_times_used_as_production_time: false,
    }
}

pub fn audit_duplicates(
    source_root: &Path,
    options: &DuplicateAuditOptions,
    observed_at_ms: u64,
) -> Result<DuplicateAuditReport, String> {
    options.validate()?;
    if observed_at_ms == 0 {
        return Err("duplicate-audit-observed-at-invalid".into());
    }
    let root = canonical_real_directory(source_root)?;
    let scope = source_scope_fingerprint(&root);
    let started = Instant::now();
    let mut entries_seen = 0usize;
    let mut eligible_files = Vec::new();
    let mut issues = BTreeMap::new();
    let mut context_metadata_complete = true;

    let walk = jwalk::WalkDir::new(&root)
        .follow_links(false)
        .skip_hidden(false)
        .process_read_dir(|_depth, _path, _state, children| {
            children.retain(|entry| {
                entry
                    .as_ref()
                    .map(crate::scanner::keep_entry)
                    .unwrap_or(true)
            });
        });

    for entry_result in walk {
        if elapsed_exceeded(started, options.max_duration_ms) {
            increment_issue(&mut issues, "scan-duration-limit-exceeded");
            break;
        }
        entries_seen = entries_seen.saturating_add(1);
        if entries_seen > options.max_entries {
            increment_issue(&mut issues, "scan-entry-limit-exceeded");
            break;
        }
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(_) => {
                increment_issue(&mut issues, "scan-entry-unavailable");
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => metadata,
            _ => {
                increment_issue(&mut issues, "file-metadata-unavailable");
                continue;
            }
        };
        if metadata.len() < options.min_file_bytes {
            continue;
        }
        let relative_path = match path.strip_prefix(&root) {
            Ok(relative) if !relative.as_os_str().is_empty() => {
                relative.to_string_lossy().into_owned()
            }
            _ => {
                increment_issue(&mut issues, "file-path-outside-root");
                continue;
            }
        };
        let created_ms = metadata.created().ok().and_then(system_time_ms);
        let modified_ms = metadata.modified().ok().and_then(system_time_ms);
        if created_ms.is_none() || modified_ms.is_none() {
            context_metadata_complete = false;
            increment_issue(&mut issues, "filesystem-context-time-unavailable");
        }
        let (device, inode, allocated_bytes) = file_identity(&metadata);
        eligible_files.push(ObservedFile {
            path,
            relative_path,
            size: metadata.len(),
            allocated_bytes,
            created_ms,
            modified_ms,
            device,
            inode,
        });
    }

    let mut by_size: BTreeMap<u64, Vec<ObservedFile>> = BTreeMap::new();
    for file in eligible_files.iter().cloned() {
        by_size.entry(file.size).or_default().push(file);
    }
    by_size.retain(|_, files| files.len() >= 2);
    let candidate_file_count = by_size.values().map(Vec::len).sum::<usize>();
    let candidate_group_count = by_size.len();
    if !issues.is_empty()
        || candidate_file_count > options.max_files_to_hash
        || candidate_group_count > options.max_size_groups
    {
        if candidate_file_count > options.max_files_to_hash {
            increment_issue(&mut issues, "candidate-file-limit-exceeded");
        }
        if candidate_group_count > options.max_size_groups {
            increment_issue(&mut issues, "candidate-size-group-limit-exceeded");
        }
        return Ok(empty_report(
            &root,
            options,
            observed_at_ms,
            entries_seen,
            eligible_files.len(),
            candidate_file_count,
            candidate_group_count,
            issues,
            context_metadata_complete,
        ));
    }

    let mut hashed_bytes = 0u64;
    let mut hashed_file_count = 0usize;
    let mut groups = Vec::new();
    let mut size_groups: Vec<_> = by_size.into_iter().collect();
    size_groups.sort_by(|left, right| right.0.cmp(&left.0));

    'size_groups: for (_size, files) in size_groups {
        let mut by_prefix: BTreeMap<String, Vec<ObservedFile>> = BTreeMap::new();
        for file in files {
            if elapsed_exceeded(started, options.max_duration_ms) {
                increment_issue(&mut issues, "hash-duration-limit-exceeded");
                break 'size_groups;
            }
            let prefix_bytes = file.size.min(options.prefix_bytes as u64);
            if hashed_bytes.saturating_add(prefix_bytes) > options.max_hash_bytes {
                increment_issue(&mut issues, "hash-byte-limit-exceeded");
                break 'size_groups;
            }
            match hash_prefix(&file.path, options.prefix_bytes) {
                Ok(prefix) => {
                    hashed_bytes = hashed_bytes.saturating_add(prefix_bytes);
                    by_prefix.entry(prefix).or_default().push(file);
                }
                Err(_) => increment_issue(&mut issues, "prefix-hash-failed"),
            }
        }
        for prefix_files in by_prefix.into_values().filter(|files| files.len() >= 2) {
            let mut by_digest: BTreeMap<String, (ContentDigests, Vec<ObservedFile>)> =
                BTreeMap::new();
            for file in prefix_files {
                if elapsed_exceeded(started, options.max_duration_ms) {
                    increment_issue(&mut issues, "hash-duration-limit-exceeded");
                    break 'size_groups;
                }
                if hashed_bytes.saturating_add(file.size) > options.max_hash_bytes {
                    increment_issue(&mut issues, "hash-byte-limit-exceeded");
                    break 'size_groups;
                }
                let before = match std::fs::symlink_metadata(&file.path) {
                    Ok(metadata) if metadata_matches(&file, &metadata) => metadata,
                    _ => {
                        increment_issue(&mut issues, "file-changed-before-hash");
                        continue;
                    }
                };
                let digests = match digest_file(&file.path) {
                    Ok(digests) => digests,
                    Err(_) => {
                        increment_issue(&mut issues, "full-hash-failed");
                        continue;
                    }
                };
                hashed_bytes = hashed_bytes.saturating_add(file.size);
                hashed_file_count = hashed_file_count.saturating_add(1);
                let after = match std::fs::symlink_metadata(&file.path) {
                    Ok(metadata)
                        if metadata_matches(&file, &metadata)
                            && metadata.modified().ok() == before.modified().ok() =>
                    {
                        metadata
                    }
                    _ => {
                        increment_issue(&mut issues, "file-changed-during-hash");
                        continue;
                    }
                };
                drop(after);
                let key = format!(
                    "{}:{}:{}",
                    digests.blake3, digests.sha256, digests.quick_xor_base64
                );
                by_digest
                    .entry(key)
                    .or_insert_with(|| (digests.clone(), Vec::new()))
                    .1
                    .push(file);
            }
            for (_key, (digests, mut digest_files)) in by_digest {
                if digest_files.len() < 2 {
                    continue;
                }
                digest_files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
                let unique_identities: BTreeSet<_> = digest_files
                    .iter()
                    .map(|file| (file.device, file.inode))
                    .collect();
                if unique_identities.len() < 2 {
                    continue;
                }
                let unique_file_count = unique_identities.len();
                let hardlink_alias_count = digest_files.len().saturating_sub(unique_file_count);
                let logical_bytes_per_file = digest_files[0].size;
                let reclaimable_logical_bytes = logical_bytes_per_file
                    .saturating_mul((unique_file_count as u64).saturating_sub(1));
                let mut allocated_by_identity = BTreeMap::new();
                for file in &digest_files {
                    allocated_by_identity
                        .entry((file.device, file.inode))
                        .or_insert(file.allocated_bytes);
                }
                let allocated_total = allocated_by_identity
                    .values()
                    .fold(0u64, |total, bytes| total.saturating_add(*bytes));
                let allocated_preserved =
                    allocated_by_identity.values().copied().min().unwrap_or(0);
                let reclaimable_allocated_upper_bound_bytes =
                    allocated_total.saturating_sub(allocated_preserved);
                let files: Vec<_> = digest_files
                    .iter()
                    .map(|file| private_file(&scope, file))
                    .collect();
                let fingerprint = group_fingerprint(&scope, &digests, &files);
                groups.push(DuplicateAuditGroup {
                    group_fingerprint: fingerprint,
                    logical_bytes_per_file,
                    path_count: files.len(),
                    unique_file_count,
                    hardlink_alias_count,
                    reclaimable_logical_bytes,
                    reclaimable_allocated_upper_bound_bytes,
                    content_digests: digests,
                    files,
                });
            }
        }
    }

    groups.sort_by(|left, right| {
        right
            .reclaimable_logical_bytes
            .cmp(&left.reclaimable_logical_bytes)
            .then_with(|| left.group_fingerprint.cmp(&right.group_fingerprint))
    });
    let evidence_complete = issues.is_empty();
    if !evidence_complete {
        groups.clear();
    }
    let duplicate_path_count = groups.iter().map(|group| group.path_count).sum();
    let duplicate_unique_file_count = groups.iter().map(|group| group.unique_file_count).sum();
    let hardlink_alias_count = groups.iter().map(|group| group.hardlink_alias_count).sum();
    let reclaimable_logical_bytes = groups.iter().fold(0u64, |total, group| {
        total.saturating_add(group.reclaimable_logical_bytes)
    });
    let reclaimable_allocated_upper_bound_bytes = groups.iter().fold(0u64, |total, group| {
        total.saturating_add(group.reclaimable_allocated_upper_bound_bytes)
    });
    let fingerprint = report_fingerprint(&scope, options, evidence_complete, &groups);
    let evidence_gap_count = issues.values().copied().sum::<u64>() as usize;

    Ok(DuplicateAuditReport {
        version: DUPLICATE_AUDIT_VERSION,
        schema_kind: DUPLICATE_AUDIT_SCHEMA_KIND.into(),
        observed_at_ms,
        source_root: root.to_string_lossy().into_owned(),
        source_scope_fingerprint: scope,
        report_fingerprint: fingerprint,
        options: options.into(),
        entries_seen,
        eligible_file_count: eligible_files.len(),
        equal_size_candidate_file_count: candidate_file_count,
        equal_size_candidate_group_count: candidate_group_count,
        hashed_file_count,
        hashed_bytes,
        duplicate_group_count: groups.len(),
        duplicate_path_count,
        duplicate_unique_file_count,
        hardlink_alias_count,
        reclaimable_logical_bytes,
        reclaimable_allocated_upper_bound_bytes,
        evidence_complete,
        context_metadata_complete,
        evidence_gap_count,
        issue_counts: issues,
        groups,
        automatic_discard_allowed: false,
        human_context_review_required: true,
        mutation_performed: false,
        filename_date_used_as_production_time: false,
        filesystem_times_used_as_production_time: false,
    })
}

pub fn summarize_duplicate_audit(report: &DuplicateAuditReport) -> DuplicateAuditSummary {
    DuplicateAuditSummary {
        version: report.version,
        schema_kind: report.schema_kind.clone(),
        observed_at_ms: report.observed_at_ms,
        source_scope_fingerprint: report.source_scope_fingerprint.clone(),
        report_fingerprint: report.report_fingerprint.clone(),
        entries_seen: report.entries_seen,
        eligible_file_count: report.eligible_file_count,
        equal_size_candidate_file_count: report.equal_size_candidate_file_count,
        equal_size_candidate_group_count: report.equal_size_candidate_group_count,
        hashed_file_count: report.hashed_file_count,
        hashed_bytes: report.hashed_bytes,
        duplicate_group_count: report.duplicate_group_count,
        duplicate_path_count: report.duplicate_path_count,
        duplicate_unique_file_count: report.duplicate_unique_file_count,
        hardlink_alias_count: report.hardlink_alias_count,
        reclaimable_logical_bytes: report.reclaimable_logical_bytes,
        reclaimable_allocated_upper_bound_bytes: report
            .reclaimable_allocated_upper_bound_bytes,
        evidence_complete: report.evidence_complete,
        context_metadata_complete: report.context_metadata_complete,
        evidence_gap_count: report.evidence_gap_count,
        issue_counts: report.issue_counts.clone(),
        automatic_discard_allowed: report.automatic_discard_allowed,
        human_context_review_required: report.human_context_review_required,
        mutation_performed: report.mutation_performed,
        filename_date_used_as_production_time: report.filename_date_used_as_production_time,
        filesystem_times_used_as_production_time: report
            .filesystem_times_used_as_production_time,
        content_digest_algorithms: vec!["blake3".into(), "sha256".into(), "quickxor".into()],
        local_paths_redacted: true,
        content_digests_redacted: true,
        metadata_semantics: vec![
            "content equality requires matching size, prefix BLAKE3, full BLAKE3, SHA-256, and QuickXor"
                .into(),
            "hard-linked paths sharing one device and inode are not counted as reclaimable copies"
                .into(),
            "filesystem created and modified times bind source stability but are not production time"
                .into(),
            "filename dates are not inspected or used as production evidence".into(),
            "one copy must be retained and production context reviewed before any discard".into(),
        ],
        notices: vec![
            "read-only dry-run; no file was renamed, moved, trashed, or deleted".into(),
            "allocated reclaim is an upper bound because APFS clones may share physical blocks".into(),
            "private output contains local paths, timestamps, file identities, and content digests"
                .into(),
            "this report is evidence, not an approval or execution plan".into(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn options() -> DuplicateAuditOptions {
        DuplicateAuditOptions {
            min_file_bytes: 1,
            prefix_bytes: 4,
            max_entries: 100,
            max_duration_ms: 10_000,
            max_files_to_hash: 100,
            max_size_groups: 100,
            max_hash_bytes: 10_000_000,
        }
    }

    #[test]
    fn audit_proves_triple_digest_duplicates_without_mutation() {
        let temp = tempfile::tempdir().unwrap();
        let payload = b"same duplicate payload";
        std::fs::write(temp.path().join("one.bin"), payload).unwrap();
        std::fs::write(temp.path().join("two.bin"), payload).unwrap();
        std::fs::write(temp.path().join("different.bin"), b"different payload!!!").unwrap();

        let report = audit_duplicates(temp.path(), &options(), 10).unwrap();
        assert!(report.evidence_complete);
        assert_eq!(report.duplicate_group_count, 1);
        assert_eq!(report.duplicate_path_count, 2);
        assert_eq!(report.duplicate_unique_file_count, 2);
        assert_eq!(report.reclaimable_logical_bytes, payload.len() as u64);
        assert!(!report.automatic_discard_allowed);
        assert!(report.human_context_review_required);
        assert!(!report.mutation_performed);
        assert!(!report.filename_date_used_as_production_time);
        assert!(!report.filesystem_times_used_as_production_time);
        assert_eq!(
            report.groups[0].content_digests,
            crate::content_digest::digest_bytes(payload)
        );
        let summary = summarize_duplicate_audit(&report);
        assert!(summary.local_paths_redacted);
        assert!(summary.content_digests_redacted);
    }

    #[cfg(unix)]
    #[test]
    fn hardlink_aliases_are_not_reclaimable_duplicates() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("one.bin");
        let alias = temp.path().join("alias.bin");
        std::fs::write(&first, b"same inode").unwrap();
        std::fs::hard_link(&first, &alias).unwrap();
        let report = audit_duplicates(temp.path(), &options(), 10).unwrap();
        assert!(report.evidence_complete);
        assert_eq!(report.duplicate_group_count, 0);
        assert_eq!(report.reclaimable_logical_bytes, 0);
    }

    #[test]
    fn hash_bound_fails_closed_and_discards_partial_groups() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("one.bin"), vec![7_u8; 128]).unwrap();
        std::fs::write(temp.path().join("two.bin"), vec![7_u8; 128]).unwrap();
        let mut bounded = options();
        bounded.max_hash_bytes = 16;
        let report = audit_duplicates(temp.path(), &bounded, 10).unwrap();
        assert!(!report.evidence_complete);
        assert_eq!(report.duplicate_group_count, 0);
        assert_eq!(report.reclaimable_logical_bytes, 0);
        assert_eq!(
            report.issue_counts.get("hash-byte-limit-exceeded"),
            Some(&1)
        );
    }

    #[cfg(unix)]
    #[test]
    fn scan_does_not_follow_file_symlinks() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        outside.as_file().write_all(b"outside duplicate").unwrap();
        std::fs::write(temp.path().join("inside.bin"), b"outside duplicate").unwrap();
        symlink(outside.path(), temp.path().join("linked.bin")).unwrap();
        let report = audit_duplicates(temp.path(), &options(), 10).unwrap();
        assert!(report.evidence_complete);
        assert_eq!(report.duplicate_group_count, 0);
    }
}
