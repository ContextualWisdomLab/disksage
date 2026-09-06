//! Cloud-offload discovery and dry-run planning.
//!
//! This module is intentionally local and deterministic: it never uploads, moves, deletes,
//! hydrates, or calls a model.  The plan preserves enough source metadata to become the first
//! lineage record for a later verified move.

use crate::content_digest::ContentDigests;
#[cfg(not(coverage))]
use crate::content_digest::ContentHasher;
#[cfg(not(coverage))]
use crate::dataset_metadata::profile_dataset;
use crate::dataset_metadata::DatasetProfile;
#[cfg(not(coverage))]
use std::collections::BTreeMap;
use std::collections::BTreeSet;
#[cfg(not(coverage))]
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
#[cfg(not(coverage))]
use std::process::{Command, Stdio};
#[cfg(not(coverage))]
use std::time::{Duration, Instant};
use unicode_normalization::UnicodeNormalization;

const ARCHIVE_DIR: &str = "DiskSage Archive";
const DAY_MS: u64 = 86_400_000;
// OneDrive's decoded relative path is limited to 400 characters; local sync also needs each
// component to fit the filesystem's 255-character/byte boundary. Keep this provider-specific
// preflight separate from iCloud/Google Drive planning.
const ONEDRIVE_MAX_RELATIVE_PATH_CHARS: usize = 400;
const ONEDRIVE_MAX_PATH_COMPONENT_CHARS: usize = 255;
const ONEDRIVE_MAX_PATH_COMPONENT_BYTES: usize = 255;
#[cfg(not(coverage))]
const METADATA_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(not(coverage))]
const METADATA_PROBE_OUTPUT_LIMIT: usize = 1024 * 1024;
#[cfg(not(coverage))]
const MACOS_METADATA_PROBE_TIMEOUT: Duration = Duration::from_millis(500);
#[cfg(not(coverage))]
const EXIFTOOL_BATCH_SIZE: usize = 32;
#[cfg(not(coverage))]
const EXIFTOOL_BATCH_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(not(coverage))]
const EXIFTOOL_BATCH_OUTPUT_LIMIT: usize = 8 * 1024 * 1024;
#[cfg(not(coverage))]
// ponytail: cap detailed probes per plan; expand only with an asynchronous per-file budget.
const MAX_METADATA_PROBE_FILES: usize = 32;
#[cfg(not(coverage))]
const METADATA_PROBE_TOTAL_TIMEOUT: Duration = Duration::from_secs(15);
#[cfg(not(coverage))]
pub const ARCHIVE_SCAN_MAX_ENTRIES: u64 = 100_000;
#[cfg(not(coverage))]
pub const ARCHIVE_SCAN_MAX_DURATION: Duration = Duration::from_secs(10);
#[cfg(not(coverage))]
const MAX_ZIP_METADATA_ENTRIES: usize = 10_000;
#[cfg(not(coverage))]
const MAX_ZIP_CONTEXT_NAMES: usize = 16;
#[cfg(not(coverage))]
const MAX_ZIP_CENTRAL_DIRECTORY_BYTES: u64 = 16 * 1024 * 1024;
#[cfg(not(coverage))]
const MAX_ZIP_EMAIL_METADATA_ENTRIES: usize = 4_096;
#[cfg(not(coverage))]
const MAX_ZIP_EMAIL_METADATA_BYTES: u64 = 32 * 1024 * 1024;
#[cfg(not(coverage))]
const MAX_ZIP_EMAIL_HEADER_BYTES: usize = 64 * 1024;
#[cfg(not(coverage))]
const INCOMPLETE_DOWNLOAD_SCAN_CHUNK_BYTES: usize = 1024 * 1024;
#[cfg(not(coverage))]
const MAX_INCOMPLETE_DOWNLOAD_SCAN_BYTES: u64 = 4 * 1024 * 1024 * 1024;
#[cfg(not(coverage))]
const MAX_INCOMPLETE_DOWNLOAD_EOCD_OFFSETS: usize = 64;
#[cfg(not(coverage))]
const MAX_EMAIL_HEADER_BYTES: usize = 1024 * 1024;
#[cfg(not(coverage))]
const MAX_AUDACITY_SCHEMA_PROBE_BYTES: usize = 64 * 1024;
#[cfg(all(not(coverage), target_os = "macos"))]
const DIRECTORY_READ_TIMEOUT: Duration = Duration::from_secs(3);
#[cfg(all(not(coverage), target_os = "macos"))]
const DIRECTORY_READ_OUTPUT_LIMIT: u64 = 256 * 1024;

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

pub const ORGANIZATION_TENANT_AUTHORITY_REVIEW_REASON: &str =
    "organization-cloud-sensitive-context-needs-explicit-tenant-approval";

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
    SensitiveConfig,
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
            Self::SensitiveConfig => "sensitive-config",
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
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

/// Immutable, process-local evidence prepared once for one source corpus.
///
/// Content metadata is destination-independent and expensive to collect. A snapshot lets callers
/// derive several destination plans without re-running probes. Destination existence, account
/// scope, review fingerprints, capacity, provider sync state, and content digests are deliberately
/// evaluated for each final destination-specific candidate set.
#[derive(Debug, Clone)]
pub struct CloudSourceSnapshot {
    source_root: PathBuf,
    prepared_at_ms: u64,
    options: CloudPlanOptions,
    files: Vec<FileFact>,
    source_scan_complete: bool,
    source_scan_visited_entries: u64,
    source_scan_stop_reasons: Vec<String>,
    #[cfg(not(coverage))]
    verified_regular_files: BTreeSet<PathBuf>,
}

impl CloudSourceSnapshot {
    pub fn candidate_count(&self) -> usize {
        self.files.len()
    }

    pub fn candidate_bytes(&self) -> u64 {
        self.files.iter().map(|file| file.bytes).sum()
    }
}

/// Bounded result of the source-tree walk used by the cloud planner.
///
/// An incomplete walk is evidence that the candidate set is not exhaustive. The planner keeps
/// the observed files for diagnosis but marks every resulting candidate blocked, so a partial
/// scan can never become a copy or eviction approval.
#[cfg(not(coverage))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveFileCollection {
    pub files: Vec<FileFact>,
    pub visited_entries: u64,
    pub complete: bool,
    pub stop_reasons: Vec<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_selection_policy: Option<CloudPlanOptions>,
    pub candidates: Vec<CloudCandidate>,
    pub candidate_bytes: u64,
    pub potentially_reclaimable_bytes: u64,
    pub exact_duplicates: ExactDuplicateSummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capacity: Option<crate::provider_capacity::CloudCapacityAssessment>,
    /// Native source-volume pressure observed while preparing this plan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_volume: Option<crate::volume_pressure::LocalVolumeSnapshot>,
    /// Path-free freshness/integrity comparison of the observations used before a native copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_copy_evidence: Option<PreCopyEvidenceCohort>,
    pub notices: Vec<String>,
}

pub const PRE_COPY_EVIDENCE_COHORT_SCHEMA_VERSION: u32 = 1;
pub const PRE_COPY_EVIDENCE_MAX_SKEW_MS: u64 = 5 * 60 * 1000;
const PRE_COPY_EVIDENCE_REQUIRED_STREAMS: [&str; 3] = [
    "icloud-sync-health-evidence",
    "provider-client-runtime-evidence",
    "volume-pressure-evidence",
];

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreCopyEvidenceObservation {
    pub stream: String,
    pub observed_at_ms: u64,
    pub evidence_complete: bool,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreCopyEvidenceCohort {
    pub schema_version: u32,
    pub observed_at_ms: u64,
    pub observations: Vec<PreCopyEvidenceObservation>,
    pub complete: bool,
    pub blockers: Vec<String>,
    pub cohort_fingerprint: String,
}

fn valid_evidence_fingerprint(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn cohort_fingerprint(cohort: &PreCopyEvidenceCohort) -> String {
    let mut unsigned = cohort.clone();
    unsigned.cohort_fingerprint.clear();
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.pre-copy-evidence-cohort\0v1\0");
    for observation in &unsigned.observations {
        for value in [
            observation.stream.as_bytes(),
            &observation.observed_at_ms.to_le_bytes(),
            &[observation.evidence_complete as u8],
            observation.fingerprint.as_bytes(),
        ] {
            hasher.update(&(value.len() as u64).to_le_bytes());
            hasher.update(value);
        }
    }
    for blocker in &unsigned.blockers {
        hasher.update(&(blocker.len() as u64).to_le_bytes());
        hasher.update(blocker.as_bytes());
    }
    hasher.update(&unsigned.observed_at_ms.to_le_bytes());
    hasher.update(&[unsigned.complete as u8]);
    hasher.finalize().to_hex().to_string()
}

/// Compare the bounded observations that precede a native provider copy.
///
/// This is deliberately a freshness/integrity cohort, not a cloud-sync assertion. Any missing,
/// incomplete, duplicated, malformed, or materially skewed stream remains blocked.
pub fn compare_pre_copy_evidence(
    mut observations: Vec<PreCopyEvidenceObservation>,
) -> PreCopyEvidenceCohort {
    observations.sort_by(|left, right| left.stream.cmp(&right.stream));
    let mut blockers = Vec::new();
    let mut previous_stream: Option<&str> = None;
    let mut minimum = u64::MAX;
    let mut maximum = 0_u64;
    for observation in &observations {
        if observation.stream.is_empty()
            || !observation
                .stream
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            blockers.push("pre-copy-evidence-stream-name-invalid".into());
        }
        if previous_stream == Some(observation.stream.as_str()) {
            blockers.push("pre-copy-evidence-stream-duplicate".into());
        }
        previous_stream = Some(observation.stream.as_str());
        if observation.observed_at_ms == 0 {
            blockers.push("pre-copy-evidence-observation-time-invalid".into());
        } else {
            minimum = minimum.min(observation.observed_at_ms);
            maximum = maximum.max(observation.observed_at_ms);
        }
        if !valid_evidence_fingerprint(&observation.fingerprint) {
            blockers.push("pre-copy-evidence-fingerprint-invalid".into());
        }
        if !observation.evidence_complete {
            blockers.push(format!(
                "pre-copy-evidence-stream-incomplete-{}",
                observation.stream
            ));
        }
    }
    for required_stream in PRE_COPY_EVIDENCE_REQUIRED_STREAMS {
        if !observations
            .iter()
            .any(|observation| observation.stream == required_stream)
        {
            blockers.push(format!(
                "pre-copy-evidence-stream-missing-{required_stream}"
            ));
        }
    }
    for observation in &observations {
        if !PRE_COPY_EVIDENCE_REQUIRED_STREAMS.contains(&observation.stream.as_str()) {
            blockers.push("pre-copy-evidence-stream-unexpected".into());
        }
    }
    if observations.is_empty() {
        blockers.push("pre-copy-evidence-cohort-empty".into());
    }
    if minimum != u64::MAX && maximum.saturating_sub(minimum) > PRE_COPY_EVIDENCE_MAX_SKEW_MS {
        blockers.push("pre-copy-evidence-observation-time-skew".into());
    }
    blockers.sort();
    blockers.dedup();
    let mut cohort = PreCopyEvidenceCohort {
        schema_version: PRE_COPY_EVIDENCE_COHORT_SCHEMA_VERSION,
        observed_at_ms: maximum,
        observations,
        complete: blockers.is_empty(),
        blockers,
        cohort_fingerprint: String::new(),
    };
    cohort.cohort_fingerprint = cohort_fingerprint(&cohort);
    cohort
}

/// Require the exact cohort produced by the current iCloud plan before a native copy mutates the
/// destination. Recompute the fingerprint so a serialized or caller-provided cohort cannot bypass
/// the fail-closed gate.
pub fn require_pre_copy_evidence_cohort(
    cohort: Option<&PreCopyEvidenceCohort>,
) -> Result<(), String> {
    let cohort = cohort.ok_or_else(|| "pre-copy-evidence-cohort-unavailable".to_string())?;
    if cohort.schema_version != PRE_COPY_EVIDENCE_COHORT_SCHEMA_VERSION {
        return Err("pre-copy-evidence-cohort-schema-unsupported".into());
    }
    let recomputed = compare_pre_copy_evidence(cohort.observations.clone());
    if recomputed.cohort_fingerprint != cohort.cohort_fingerprint
        || recomputed.observed_at_ms != cohort.observed_at_ms
        || recomputed.complete != cohort.complete
        || recomputed.blockers != cohort.blockers
    {
        return Err("pre-copy-evidence-cohort-integrity-invalid".into());
    }
    if !cohort.complete || !cohort.blockers.is_empty() {
        return Err("pre-copy-evidence-cohort-blocked".into());
    }
    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExactDuplicateSummary {
    pub cluster_count: usize,
    pub candidate_count: usize,
    pub candidate_bytes: u64,
    pub redundant_bytes: u64,
    #[serde(default)]
    pub clusters: Vec<ExactDuplicateClusterRecommendation>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ExactDuplicateClusterRecommendation {
    pub cluster_fingerprint: String,
    pub candidate_count: usize,
    pub bytes_per_candidate: u64,
    pub redundant_bytes: u64,
    pub recommended_canonical_metadata_fingerprint: String,
    pub recommendation_confidence: String,
    pub recommendation_reason_codes: Vec<String>,
    pub member_metadata_fingerprints: Vec<String>,
    pub requires_human_confirmation: bool,
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

    #[cfg(all(not(coverage), target_os = "macos"))]
    {
        if let Some(reason) = directory_access_issue(Path::new(&root.path)) {
            return Err(format!("cloud-root-unreadable:{}:{reason}", root.path));
        }
        return Ok(());
    }

    #[cfg(any(coverage, not(target_os = "macos")))]
    std::fs::read_dir(&root.path)
        .map(|_| ())
        .map_err(|error| format!("cloud-root-unreadable:{}:{error}", root.path))
}

/// Match an operator-supplied cloud root to a discovered root without depending on the Unicode
/// normalization form exposed by the shell or macOS File Provider. Filesystem identity wins when
/// both spellings resolve; canonical-equivalent UTF-8 is the bounded fallback. Callers still fail
/// closed when more than one discovered root matches.
pub fn cloud_root_path_matches(discovered: &Path, requested: &Path) -> bool {
    if discovered == requested {
        return true;
    }
    if let (Ok(discovered), Ok(requested)) = (
        std::fs::canonicalize(discovered),
        std::fs::canonicalize(requested),
    ) {
        if discovered == requested {
            return true;
        }
    }
    match (discovered.to_str(), requested.to_str()) {
        (Some(discovered), Some(requested)) => discovered.nfc().eq(requested.nfc()),
        _ => false,
    }
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
    #[cfg(all(not(coverage), target_os = "macos"))]
    {
        return run_bounded_find(
            path,
            &["-mindepth", "1", "-maxdepth", "1", "-print0", "-quit"],
        )
        .err();
    }

    #[cfg(any(coverage, not(target_os = "macos")))]
    std::fs::read_dir(path)
        .err()
        .map(|error| access_issue_for_error(&error))
}

#[cfg(all(not(coverage), target_os = "macos"))]
fn run_bounded_find(path: &Path, action: &[&str]) -> Result<Vec<u8>, String> {
    use std::os::unix::process::CommandExt;

    let metadata = std::fs::metadata(path).map_err(|error| access_issue_for_error(&error))?;
    if !metadata.is_dir() {
        return Err("not-a-directory".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mode = metadata.permissions().mode();
        if mode & 0o444 == 0 || mode & 0o111 == 0 {
            return Err("permission-denied".into());
        }
    }

    let find = Path::new("/usr/bin/find");
    let find_metadata =
        std::fs::symlink_metadata(find).map_err(|_| "read-dir-helper-unavailable".to_string())?;
    if !find_metadata.file_type().is_file() {
        return Err("read-dir-helper-unavailable".into());
    }

    let mut command = Command::new(find);
    command
        .arg(path)
        .args(action)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // File Provider paths can leave helper descendants holding stdout after the leader exits.
    // Keep the helper in its own group so timeout cleanup closes the pipe and joins promptly.
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command
        .spawn()
        .map_err(|_| "read-dir-helper-failed".to_string())?;
    let child_pid = child.id();
    let kill_group = || unsafe {
        let _ = libc::kill(-(child_pid as libc::pid_t), libc::SIGKILL);
    };
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => {
            kill_group();
            let _ = child.kill();
            let _ = child.wait();
            return Err("read-dir-helper-failed".into());
        }
    };
    let reader = std::thread::spawn(move || {
        let mut output = Vec::new();
        stdout
            .take(DIRECTORY_READ_OUTPUT_LIMIT + 1)
            .read_to_end(&mut output)
            .map(|_| output)
    });

    let deadline = Instant::now() + DIRECTORY_READ_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                kill_group();
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err("read-dir-timeout".into());
            }
            Err(_) => {
                kill_group();
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err("read-dir-helper-failed".into());
            }
        }
    };
    // The leader may have exited while a descendant still owns the pipe; close the private group
    // before joining the reader so a successful probe cannot hang on inherited stdout.
    kill_group();
    let output = reader
        .join()
        .map_err(|_| "read-dir-helper-failed".to_string())?
        .map_err(|_| "read-dir-helper-failed".to_string())?;
    if output.len() as u64 > DIRECTORY_READ_OUTPUT_LIMIT {
        return Err("read-dir-output-too-large".into());
    }
    if !status.success() {
        return Err("read-dir-failed".into());
    }
    Ok(output)
}

#[cfg(not(coverage))]
fn read_children_sorted(path: &Path, limit: usize) -> Result<Vec<PathBuf>, String> {
    #[cfg(target_os = "macos")]
    {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let output = run_bounded_find(path, &["-mindepth", "1", "-maxdepth", "1", "-print0"])?;
        let mut children = Vec::new();
        for raw in output
            .split(|byte| *byte == 0)
            .filter(|raw| !raw.is_empty())
        {
            if children.len() >= limit {
                break;
            }
            children.push(PathBuf::from(OsString::from_vec(raw.to_vec())));
        }
        children.sort();
        return Ok(children);
    }

    #[cfg(not(target_os = "macos"))]
    {
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
        push_discovery_issue(report, Some(provider), account_scope, &path, label, reason);
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
    if is_sensitive_config_path(path) {
        return Some(ArchiveKind::SensitiveConfig);
    }
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    match ext.as_str() {
        "pdf" | "doc" | "docx" | "ppt" | "pptx" | "xls" | "xlsx" | "xlsm" | "xlsb" | "odt"
        | "ods" | "odp" | "pages" | "numbers" | "key" | "epub" | "mobi" => {
            Some(ArchiveKind::Document)
        }
        "eml" => Some(ArchiveKind::Document),
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
        "psd" | "ai" | "indd" | "sketch" | "fig" | "blend" | "aup3" => Some(ArchiveKind::Creative),
        "crdownload" => Some(ArchiveKind::IncompleteDownload),
        _ if multipart_archive_part(path).is_some() => Some(ArchiveKind::Archive),
        _ => None,
    }
}

/// Classify credential-bearing names without opening the file. These entries stay visible in a
/// plan for diagnosis, but the shared source blocker prevents metadata probing, cloud copy, and
/// source eviction.
fn is_sensitive_config_path(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let file_name = file_name.to_ascii_lowercase();
    let env_secret = (file_name == ".env" || file_name.starts_with(".env."))
        && !matches!(
            file_name.as_str(),
            ".env.example" | ".env.sample" | ".env.template"
        );
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    env_secret
        || matches!(extension.as_str(), "key" | "pem" | "p12" | "pfx")
        || file_name.contains("credential")
        || file_name.contains("private_key")
        || file_name.contains("private-key")
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

#[cfg(target_os = "macos")]
pub(crate) fn metadata_is_dataless(metadata: &std::fs::Metadata) -> bool {
    use std::os::macos::fs::MetadataExt;

    const SF_DATALESS: u32 = 0x4000_0000;
    metadata.st_flags() & SF_DATALESS != 0
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn metadata_is_dataless(_metadata: &std::fs::Metadata) -> bool {
    false
}

pub(crate) fn source_content_is_dataless(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|metadata| metadata_is_dataless(&metadata))
        .unwrap_or(false)
}

/// Collect only archive-shaped regular files while pruning cloud roots and regenerable trees
/// before descent. Symlinks/reparse points are rejected by the shared scanner guard.
///
/// The walk is deliberately bounded. A partial source tree is useful diagnostic evidence but is
/// never eligible for copy because `plan_cloud_archive_from_snapshot` carries the incomplete-scan
/// blocker into every candidate.
#[cfg(not(coverage))]
pub fn collect_archive_files_bounded(
    root: &Path,
    excluded_roots: &[PathBuf],
    max_entries: u64,
    max_duration: Duration,
) -> ArchiveFileCollection {
    if path_inside_managed_file_provider_storage(root) {
        return ArchiveFileCollection {
            files: Vec::new(),
            visited_entries: 0,
            complete: false,
            stop_reasons: vec!["source-scan-managed-file-provider-root".into()],
        };
    }
    let excluded = excluded_roots.to_vec();
    let mut files = Vec::new();
    let mut visited_entries = 0_u64;
    let mut stop_reasons = Vec::new();
    let started = Instant::now();
    let max_entries = max_entries.max(1);
    let max_duration = max_duration.max(Duration::from_millis(1));
    let mut complete = true;
    let walker = walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(move |entry| {
            if entry.depth() == 0 {
                return true;
            }
            let path = entry.path();
            if crate::safety::is_explicitly_protected(path) {
                return false;
            }
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
        });
    for result in walker {
        if visited_entries >= max_entries {
            complete = false;
            stop_reasons.push("source-scan-entry-limit".into());
            break;
        }
        if started.elapsed() >= max_duration {
            complete = false;
            stop_reasons.push("source-scan-time-limit".into());
            break;
        }
        visited_entries = visited_entries.saturating_add(1);
        let entry = match result {
            Ok(entry) => entry,
            Err(_) => {
                complete = false;
                if !stop_reasons.iter().any(|reason| reason == "source-scan-entry-error") {
                    stop_reasons.push("source-scan-entry-error".into());
                }
                continue;
            }
        };
        if !entry.file_type().is_file() || archive_kind(&entry.path()).is_none() {
            continue;
        }
        let Some(metadata) = entry.metadata().ok() else {
            complete = false;
            if !stop_reasons
                .iter()
                .any(|reason| reason == "source-scan-metadata-error")
            {
                stop_reasons.push("source-scan-metadata-error".into());
            }
            continue;
        };
        files.push(FileFact {
            path: entry.path().to_path_buf(),
            bytes: metadata.len(),
            created_ms: millis(metadata.created()),
            modified_ms: millis(metadata.modified()),
            content_metadata: ContentMetadata::default(),
        });
    }
    stop_reasons.sort();
    stop_reasons.dedup();
    if !stop_reasons.is_empty() {
        complete = false;
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    ArchiveFileCollection {
        files,
        visited_entries,
        complete,
        stop_reasons,
    }
}

/// Collect archive files using the production source-scan bounds.
#[cfg(not(coverage))]
pub fn collect_archive_files(root: &Path, excluded_roots: &[PathBuf]) -> Vec<FileFact> {
    collect_archive_files_bounded(
        root,
        excluded_roots,
        ARCHIVE_SCAN_MAX_ENTRIES,
        ARCHIVE_SCAN_MAX_DURATION,
    )
    .files
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

/// Stable UTC production year/month used by path-free organization summaries.
/// The cloud destination builder uses the same underlying date decomposition.
pub fn production_year_month(epoch_ms: u64) -> (i32, u32) {
    let (year, month, _) = date_parts(epoch_ms);
    (year, month)
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
pub(crate) fn filename_date_ms(path: &Path) -> Option<u64> {
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

fn filename_publication_month(path: &Path) -> Option<(i32, u32)> {
    let normalized: String = path.file_name()?.to_string_lossy().nfc().collect();
    let chars: Vec<char> = normalized.chars().collect();
    for start in 0..chars.len() {
        if start > 0 && chars[start - 1].is_alphanumeric() {
            continue;
        }
        for year_len in [4, 2] {
            let Some(year_digits) = chars.get(start..start + year_len) else {
                continue;
            };
            if !year_digits.iter().all(char::is_ascii_digit) {
                continue;
            }
            let mut index = start + year_len;
            while chars.get(index).is_some_and(|value| value.is_whitespace()) {
                index += 1;
            }
            if chars.get(index) != Some(&'년') {
                continue;
            }
            index += 1;
            while chars.get(index).is_some_and(|value| value.is_whitespace()) {
                index += 1;
            }
            let month_start = index;
            while index < chars.len() && index - month_start < 2 && chars[index].is_ascii_digit() {
                index += 1;
            }
            if index == month_start || chars.get(index).is_some_and(char::is_ascii_digit) {
                continue;
            }
            let month = chars[month_start..index].iter().fold(0u32, |value, digit| {
                value * 10 + digit.to_digit(10).unwrap_or_default()
            });
            while chars.get(index).is_some_and(|value| value.is_whitespace()) {
                index += 1;
            }
            if chars.get(index) != Some(&'월') {
                continue;
            }
            index += 1;
            while chars.get(index).is_some_and(|value| value.is_whitespace()) {
                index += 1;
            }
            if chars.get(index) != Some(&'호') {
                continue;
            }
            index += 1;
            if chars
                .get(index)
                .is_some_and(|value| value.is_alphanumeric())
            {
                continue;
            }
            let year = year_digits.iter().fold(0i32, |value, digit| {
                value * 10 + digit.to_digit(10).unwrap_or_default() as i32
            });
            let year = if year_len == 2 { 2000 + year } else { year };
            if (1970..=2100).contains(&year) && (1..=12).contains(&month) {
                return Some((year, month));
            }
        }
    }
    None
}

fn add_filename_publication_month(metadata: &mut ContentMetadata, year: i32, month: u32) {
    let value = format!("{year:04}-{month:02}");
    let context = format!("filename-publication-month={value}");
    if !metadata.context.contains(&context) {
        metadata.context.push(context);
    }
    add_evidence(
        metadata,
        "filename-publication-month",
        value,
        "filename:publication-month",
        "low",
    );
}

pub(crate) fn date_value(epoch_ms: u64) -> String {
    let (year, month, day) = date_parts(epoch_ms);
    format!("{year:04}-{month:02}-{day:02}")
}

fn timestamp_value(epoch_ms: u64) -> String {
    let (year, month, day) = date_parts(epoch_ms);
    let time_ms = epoch_ms % DAY_MS;
    let hour = time_ms / 3_600_000;
    let minute = (time_ms % 3_600_000) / 60_000;
    let second = (time_ms % 60_000) / 1_000;
    let millisecond = time_ms % 1_000;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millisecond:03}Z")
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
    if epoch_ms % DAY_MS != 0 {
        add_evidence(
            metadata,
            "production-timestamp",
            timestamp_value(epoch_ms),
            source,
            confidence,
        );
    }
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

fn timestamp_from_text(value: &str) -> Option<u64> {
    let bytes = value.trim().as_bytes();
    if bytes.len() < 20
        || !matches!(bytes.get(4), Some(b':' | b'-'))
        || !matches!(bytes.get(7), Some(b':' | b'-'))
        || !matches!(bytes.get(10), Some(b' ' | b'T'))
        || bytes.get(13) != Some(&b':')
        || bytes.get(16) != Some(&b':')
    {
        return None;
    }

    let year = digits(bytes, 0, 4)? as i32;
    let month = digits(bytes, 5, 2)?;
    let day = digits(bytes, 8, 2)?;
    let hour = digits(bytes, 11, 2)?;
    let minute = digits(bytes, 14, 2)?;
    let second = digits(bytes, 17, 2)?;
    if !valid_date(year, month, day) || hour > 23 || minute > 59 || second > 59 {
        return None;
    }

    let mut index = 19;
    let mut millisecond = 0_u64;
    if bytes.get(index) == Some(&b'.') {
        index += 1;
        let fraction_start = index;
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            if index - fraction_start < 3 {
                millisecond = millisecond
                    .saturating_mul(10)
                    .saturating_add(u64::from(bytes[index] - b'0'));
            }
            index += 1;
        }
        let fraction_digits = index - fraction_start;
        if fraction_digits == 0 {
            return None;
        }
        if fraction_digits == 1 {
            millisecond *= 100;
        } else if fraction_digits == 2 {
            millisecond *= 10;
        }
    }

    let offset_seconds = match bytes.get(index) {
        Some(b'Z' | b'z') if index + 1 == bytes.len() => 0_i64,
        Some(sign @ (b'+' | b'-'))
            if index + 6 == bytes.len() && bytes.get(index + 3) == Some(&b':') =>
        {
            let offset_hour = digits(bytes, index + 1, 2)?;
            let offset_minute = digits(bytes, index + 4, 2)?;
            if offset_hour > 14 || offset_minute > 59 || (offset_hour == 14 && offset_minute != 0) {
                return None;
            }
            let magnitude = i64::from(offset_hour * 3_600 + offset_minute * 60);
            if *sign == b'+' {
                magnitude
            } else {
                -magnitude
            }
        }
        _ => return None,
    };

    let local_seconds = days_from_civil(year, month, day)
        .checked_mul(86_400)?
        .checked_add(i64::from(hour * 3_600 + minute * 60 + second))?;
    let utc_seconds = local_seconds.checked_sub(offset_seconds)?;
    u64::try_from(utc_seconds)
        .ok()?
        .checked_mul(1_000)?
        .checked_add(millisecond)
}

fn date_from_text(value: &str) -> Option<u64> {
    timestamp_from_text(value).or_else(|| {
        filename_date_ms(Path::new(value)).or_else(|| {
            let normalized = value.replace(':', "-");
            filename_date_ms(Path::new(&normalized))
        })
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
    FileLimit,
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
            Self::FileLimit => "file-limit-exceeded",
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
    if let Ok(output) = run_metadata_command_with_limits(
        where_froms,
        MACOS_METADATA_PROBE_TIMEOUT,
        METADATA_PROBE_OUTPUT_LIMIT,
    ) {
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
    if let Ok(output) = run_metadata_command_with_limits(
        quarantine,
        MACOS_METADATA_PROBE_TIMEOUT,
        METADATA_PROBE_OUTPUT_LIMIT,
    ) {
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
fn configure_exiftool_command(command: &mut Command) {
    command.args([
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
    ]);
}

#[cfg(not(coverage))]
fn exiftool_values_metadata(
    values: &serde_json::Map<String, serde_json::Value>,
) -> ContentMetadata {
    let mut metadata = ContentMetadata::default();
    if values.get("Error").is_some() {
        add_probe_warning(
            &mut metadata,
            "exiftool",
            MetadataProbeFailure::InvalidOutput,
        );
        return metadata;
    }

    for key in [
        "DateTimeOriginal",
        "MediaCreateDate",
        "TrackCreateDate",
        "CreateDate",
        "CreationDate",
    ] {
        for value in json_strings(values.get(key)) {
            let precise_timestamp = timestamp_from_text(&value);
            if let Some(epoch_ms) = date_from_text(&value) {
                if precise_timestamp.is_some() {
                    add_evidence(
                        &mut metadata,
                        "production-timestamp-raw",
                        &value,
                        &format!("embedded:exiftool:{key}"),
                        "high",
                    );
                }
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
fn exiftool_document_metadata(
    output: &[u8],
) -> Result<BTreeMap<PathBuf, ContentMetadata>, MetadataProbeFailure> {
    let document = serde_json::from_slice::<Vec<serde_json::Value>>(output)
        .map_err(|_| MetadataProbeFailure::InvalidOutput)?;
    let mut by_path = BTreeMap::new();
    for item in document {
        let values = item
            .as_object()
            .ok_or(MetadataProbeFailure::InvalidOutput)?;
        let path = values
            .get("SourceFile")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or(MetadataProbeFailure::InvalidOutput)?;
        if by_path
            .insert(PathBuf::from(path), exiftool_values_metadata(values))
            .is_some()
        {
            return Err(MetadataProbeFailure::InvalidOutput);
        }
    }
    Ok(by_path)
}

#[cfg(not(coverage))]
fn exiftool_metadata(path: &Path) -> ContentMetadata {
    let mut command = local_command("exiftool");
    configure_exiftool_command(&mut command);
    command.arg(path);
    let output = match run_metadata_command(command) {
        Ok(output) => output,
        Err(failure) => {
            let mut metadata = ContentMetadata::default();
            add_probe_warning(&mut metadata, "exiftool", failure);
            return metadata;
        }
    };
    match exiftool_document_metadata(&output) {
        Ok(mut by_path) => by_path.remove(path).unwrap_or_else(|| {
            let mut metadata = ContentMetadata::default();
            add_probe_warning(
                &mut metadata,
                "exiftool",
                MetadataProbeFailure::InvalidOutput,
            );
            metadata
        }),
        Err(failure) => {
            let mut metadata = ContentMetadata::default();
            add_probe_warning(&mut metadata, "exiftool", failure);
            metadata
        }
    }
}

#[cfg(not(coverage))]
fn exiftool_batch_failure_metadata(failure: MetadataProbeFailure) -> ContentMetadata {
    let mut metadata = ContentMetadata::default();
    add_probe_warning(&mut metadata, "exiftool-batch", failure);
    metadata
}

#[cfg(not(coverage))]
fn exiftool_metadata_batch(paths: &[PathBuf]) -> BTreeMap<PathBuf, ContentMetadata> {
    let mut by_path = BTreeMap::new();
    for chunk in paths.chunks(EXIFTOOL_BATCH_SIZE) {
        let mut command = local_command("exiftool");
        configure_exiftool_command(&mut command);
        command.args(chunk);
        let batch = run_metadata_command_with_limits(
            command,
            EXIFTOOL_BATCH_TIMEOUT,
            EXIFTOOL_BATCH_OUTPUT_LIMIT,
        )
        .and_then(|output| exiftool_document_metadata(&output));
        match batch {
            Ok(mut parsed) => {
                for path in chunk {
                    let metadata = parsed.remove(path).unwrap_or_else(|| {
                        // A successful batch that omits a requested source is incomplete.
                        // Do not fall back to one unbounded subprocess per file; retain an
                        // explicit warning so the planner can block or review the candidate.
                        exiftool_batch_failure_metadata(MetadataProbeFailure::InvalidOutput)
                    });
                    by_path.insert(path.clone(), metadata);
                }
            }
            Err(failure) => {
                // Retrying every path individually after a batch timeout turned one bounded
                // probe into minutes of serial work on a real Downloads corpus. Preserve the
                // metadata gap as evidence and let format-specific probes (ffprobe, zip, PDF,
                // and so on) continue without hiding the batch failure.
                for path in chunk {
                    by_path.insert(path.clone(), exiftool_batch_failure_metadata(failure));
                }
            }
        }
    }
    by_path
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
    let year = parts[4].parse().ok()?;
    let day = parts[2].parse().ok()?;
    let offset = match parts.get(5).copied() {
        Some("UTC" | "GMT") => Some("+00:00"),
        Some("KST" | "JST") => Some("+09:00"),
        _ => None,
    };
    offset
        .and_then(|offset| {
            timestamp_from_text(&format!(
                "{year:04}-{month:02}-{day:02}T{}{offset}",
                parts[3]
            ))
        })
        .or_else(|| date_epoch_ms(year, month, day))
}

#[cfg(not(coverage))]
fn pdfinfo_metadata(path: &Path) -> ContentMetadata {
    let mut metadata = ContentMetadata::default();
    let mut command = local_command("pdfinfo");
    command.env("LC_ALL", "C").env("TZ", "UTC").arg(path);
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ZipEocdPreflight {
    entry_count: usize,
    central_directory_bytes: u64,
}

#[cfg(not(coverage))]
fn zip_eocd_preflight(file: &mut std::fs::File) -> Result<ZipEocdPreflight, &'static str> {
    const EOCD_BYTES: usize = 22;
    const MAX_COMMENT_BYTES: usize = u16::MAX as usize;
    const EOCD_SIGNATURE: &[u8; 4] = b"PK\x05\x06";

    let file_bytes = file.seek(SeekFrom::End(0)).map_err(|_| "seek-failed")?;
    if file_bytes < EOCD_BYTES as u64 {
        return Err("eocd-missing");
    }
    let tail_bytes = usize::try_from(file_bytes.min((EOCD_BYTES + MAX_COMMENT_BYTES) as u64))
        .map_err(|_| "size-overflow")?;
    file.seek(SeekFrom::End(-(tail_bytes as i64)))
        .map_err(|_| "seek-failed")?;
    let mut tail = vec![0u8; tail_bytes];
    file.read_exact(&mut tail).map_err(|_| "read-failed")?;

    let eocd = (0..=tail.len() - EOCD_BYTES).rev().find(|offset| {
        if tail.get(*offset..*offset + 4) != Some(EOCD_SIGNATURE.as_slice()) {
            return false;
        }
        let comment_bytes = u16::from_le_bytes([tail[*offset + 20], tail[*offset + 21]]) as usize;
        *offset + EOCD_BYTES + comment_bytes == tail.len()
    });
    let offset = eocd.ok_or("eocd-missing")?;
    let u16_at = |relative: usize| {
        u16::from_le_bytes([tail[offset + relative], tail[offset + relative + 1]])
    };
    let u32_at = |relative: usize| {
        u32::from_le_bytes([
            tail[offset + relative],
            tail[offset + relative + 1],
            tail[offset + relative + 2],
            tail[offset + relative + 3],
        ])
    };
    let disk = u16_at(4);
    let central_disk = u16_at(6);
    let entries_on_disk = u16_at(8);
    let total_entries = u16_at(10);
    let central_directory_bytes = u64::from(u32_at(12));
    let central_directory_offset = u64::from(u32_at(16));
    if disk != 0 || central_disk != 0 || entries_on_disk != total_entries {
        return Err("multi-disk-unsupported");
    }
    if total_entries == u16::MAX
        || central_directory_bytes == u64::from(u32::MAX)
        || central_directory_offset == u64::from(u32::MAX)
    {
        return Err("zip64-unsupported");
    }
    let entry_count = usize::from(total_entries);
    if entry_count > MAX_ZIP_METADATA_ENTRIES {
        return Err("entry-limit-exceeded");
    }
    if central_directory_bytes > MAX_ZIP_CENTRAL_DIRECTORY_BYTES {
        return Err("central-directory-too-large");
    }
    if central_directory_offset
        .checked_add(central_directory_bytes)
        .is_none_or(|end| end > file_bytes)
    {
        return Err("central-directory-out-of-bounds");
    }
    Ok(ZipEocdPreflight {
        entry_count,
        central_directory_bytes,
    })
}

#[cfg(not(coverage))]
fn zip_archive_metadata(path: &Path) -> ContentMetadata {
    let mut metadata = ContentMetadata::default();
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => {
            add_evidence(
                &mut metadata,
                "metadata-probe-warning",
                "zip-central-directory:open-failed",
                "local:metadata-probe:rust-zip",
                "high",
            );
            return metadata;
        }
    };
    let preflight = match zip_eocd_preflight(&mut file) {
        Ok(preflight) => preflight,
        Err(reason) => {
            add_evidence(
                &mut metadata,
                "metadata-probe-warning",
                format!("zip-central-directory:{reason}"),
                "local:metadata-probe:rust-zip",
                "high",
            );
            return metadata;
        }
    };
    if file.seek(SeekFrom::Start(0)).is_err() {
        add_evidence(
            &mut metadata,
            "metadata-probe-warning",
            "zip-central-directory:seek-failed",
            "local:metadata-probe:rust-zip",
            "high",
        );
        return metadata;
    }
    let mut archive = match zip::ZipArchive::new(std::io::BufReader::new(file)) {
        Ok(archive) => archive,
        Err(_) => {
            add_evidence(
                &mut metadata,
                "metadata-probe-warning",
                "zip-central-directory:index-unreadable",
                "local:metadata-probe:rust-zip",
                "high",
            );
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
    let entry_count = archive.len();
    if entry_count != preflight.entry_count {
        add_evidence(
            &mut metadata,
            "metadata-probe-warning",
            format!(
                "zip-central-directory:entry-count-mismatch:{}:{entry_count}",
                preflight.entry_count
            ),
            "local:metadata-probe:rust-zip",
            "high",
        );
        return metadata;
    }
    add_evidence(
        &mut metadata,
        "archive-entry-count",
        entry_count.to_string(),
        "embedded:zip-central-directory",
        "high",
    );
    add_evidence(
        &mut metadata,
        "archive-central-directory-bytes",
        preflight.central_directory_bytes.to_string(),
        "embedded:zip-central-directory",
        "high",
    );

    let mut earliest_modified_ms = None;
    let mut latest_modified_ms = None;
    let mut timestamped_entries = 0u64;
    let mut uncompressed_bytes = 0u64;
    let mut encrypted_entries = 0u64;
    let mut unsafe_path_entries = 0u64;
    let mut top_level_names = BTreeSet::new();
    let mut content_classes = BTreeSet::new();
    let mut email_entry_count = 0usize;
    let mut email_scanned_count = 0usize;
    let mut email_header_bytes = 0u64;
    let mut email_header_parse_failures = 0usize;
    let mut email_scan_bounded = false;
    let mut earliest_email_date_ms = None;
    let mut latest_email_date_ms = None;
    for index in 0..entry_count {
        let mut entry = match archive.by_index(index) {
            Ok(entry) => entry,
            Err(_) => {
                add_evidence(
                    &mut metadata,
                    "metadata-probe-warning",
                    format!("zip-central-directory:entry-unreadable:{index}"),
                    "local:metadata-probe:rust-zip",
                    "high",
                );
                return metadata;
            }
        };
        uncompressed_bytes = uncompressed_bytes.saturating_add(entry.size());
        encrypted_entries += u64::from(entry.encrypted());
        if entry.is_file() {
            if let Some(epoch_ms) = entry.last_modified().and_then(zip_datetime_epoch_ms) {
                timestamped_entries += 1;
                earliest_modified_ms = Some(
                    earliest_modified_ms.map_or(epoch_ms, |current: u64| current.min(epoch_ms)),
                );
                latest_modified_ms =
                    Some(latest_modified_ms.map_or(epoch_ms, |current: u64| current.max(epoch_ms)));
            }
        }

        let Some(enclosed) = entry.enclosed_name() else {
            unsafe_path_entries += 1;
            continue;
        };
        if enclosed.components().any(|component| {
            !matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        }) {
            unsafe_path_entries += 1;
            continue;
        }
        let Some(unicode_path) = enclosed.to_str() else {
            unsafe_path_entries += 1;
            continue;
        };
        let normalized: String = unicode_path.nfc().take(512).collect();
        if normalized.bytes().any(|byte| byte.is_ascii_control()) {
            unsafe_path_entries += 1;
            continue;
        }
        if let Some(name) = enclosed
            .components()
            .find_map(|component| match component {
                std::path::Component::Normal(name) => name.to_str(),
                _ => None,
            })
            .map(|name| name.nfc().take(160).collect::<String>())
            .filter(|name| !name.is_empty())
        {
            if top_level_names.len() < MAX_ZIP_CONTEXT_NAMES {
                top_level_names.insert(name);
            }
        }

        let lower = normalized.to_lowercase();
        let extension = enclosed
            .extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        if extension == "eml" {
            content_classes.insert("email");
            email_entry_count = email_entry_count.saturating_add(1);
            let remaining = MAX_ZIP_EMAIL_METADATA_BYTES.saturating_sub(email_header_bytes);
            if email_scanned_count >= MAX_ZIP_EMAIL_METADATA_ENTRIES || remaining == 0 {
                email_scan_bounded = true;
            } else {
                let read_limit = MAX_ZIP_EMAIL_HEADER_BYTES.min(remaining as usize);
                let mut header = Vec::with_capacity(read_limit.saturating_add(1));
                let read_result = entry
                    .by_ref()
                    .take((read_limit.saturating_add(1)) as u64)
                    .read_to_end(&mut header);
                match read_result {
                    Ok(_) => {
                        let truncated = header.len() > read_limit;
                        header.truncate(read_limit);
                        email_header_bytes = email_header_bytes
                            .saturating_add(header.len() as u64);
                        email_scanned_count = email_scanned_count.saturating_add(1);
                        let email_metadata = email_metadata_from_header(
                            &header,
                            truncated,
                            "embedded:zip-entry:rfc5322-header",
                            "embedded:zip-entry:rfc5322:date",
                            "local:metadata-probe:rust-zip-email",
                        );
                        if email_metadata
                            .evidence
                            .iter()
                            .any(|evidence| evidence.field == "metadata-probe-warning")
                        {
                            email_header_parse_failures =
                                email_header_parse_failures.saturating_add(1);
                        }
                        if let Some(epoch_ms) = email_metadata.production_time_ms {
                            earliest_email_date_ms = Some(
                                earliest_email_date_ms
                                    .map_or(epoch_ms, |current: u64| current.min(epoch_ms)),
                            );
                            latest_email_date_ms = Some(
                                latest_email_date_ms
                                    .map_or(epoch_ms, |current: u64| current.max(epoch_ms)),
                            );
                        }
                    }
                    Err(_) => {
                        email_header_parse_failures =
                            email_header_parse_failures.saturating_add(1);
                        email_scan_bounded = true;
                    }
                }
            }
        }
        if matches!(
            extension.as_str(),
            "csv"
                | "tsv"
                | "parquet"
                | "feather"
                | "arrow"
                | "sav"
                | "sas7bdat"
                | "dta"
                | "rdata"
                | "rds"
                | "sqlite"
                | "sqlite3"
                | "db"
                | "sql"
                | "jsonl"
                | "xls"
                | "xlsx"
                | "xlsm"
                | "xlsb"
                | "ods"
        ) {
            content_classes.insert("structured-data");
        }
        if matches!(
            extension.as_str(),
            "wav" | "mp3" | "m4a" | "flac" | "aiff" | "aac" | "ogg"
        ) {
            content_classes.insert("recording-media");
        }
        if matches!(
            extension.as_str(),
            "doc" | "docx" | "ppt" | "pptx" | "pdf" | "odt" | "odp"
        ) {
            content_classes.insert("document");
        }
        const SOURCE_EXTENSIONS: &[&str] = &[
            "rs", "py", "js", "jsx", "ts", "tsx", "java", "kt", "go", "c", "cc", "cpp", "h", "hpp",
            "cs", "swift", "rb", "php", "scala", "sh", "ps1",
        ];
        if SOURCE_EXTENSIONS.contains(&extension.as_str()) {
            content_classes.insert("source-code");
        }
        let file_name = enclosed
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        let env_secret = (file_name == ".env" || file_name.starts_with(".env."))
            && !matches!(
                file_name.as_str(),
                ".env.example" | ".env.sample" | ".env.template"
            );
        if env_secret
            || matches!(extension.as_str(), "key" | "pem" | "p12" | "pfx")
            || lower.contains("credential")
            || lower.contains("private_key")
            || lower.contains("private-key")
        {
            content_classes.insert("secret-like-path");
        }
    }

    for (field, value) in [
        ("archive-uncompressed-bytes", uncompressed_bytes),
        ("archive-timestamped-entry-count", timestamped_entries),
        ("archive-encrypted-entry-count", encrypted_entries),
        ("archive-unsafe-path-entry-count", unsafe_path_entries),
    ] {
        add_evidence(
            &mut metadata,
            field,
            value.to_string(),
            "embedded:zip-central-directory",
            "high",
        );
    }
    if let Some(epoch_ms) = earliest_modified_ms {
        add_evidence(
            &mut metadata,
            "archive-earliest-entry-modified",
            date_value(epoch_ms),
            "embedded:zip-central-directory",
            "medium",
        );
    }
    if let Some(epoch_ms) = latest_modified_ms {
        set_production_time(
            &mut metadata,
            epoch_ms,
            "embedded:zip-central-directory:latest-entry-modified",
            "medium",
        );
    }
    if email_entry_count > 0 {
        add_evidence(
            &mut metadata,
            "archive-email-entry-count",
            email_entry_count.to_string(),
            "embedded:zip-central-directory",
            "high",
        );
        add_evidence(
            &mut metadata,
            "archive-email-header-scanned-count",
            email_scanned_count.to_string(),
            "embedded:zip-entry:rfc5322-header",
            "high",
        );
        add_evidence(
            &mut metadata,
            "archive-email-header-scanned-bytes",
            email_header_bytes.to_string(),
            "embedded:zip-entry:rfc5322-header",
            "high",
        );
        if let Some(epoch_ms) = earliest_email_date_ms {
            add_evidence(
                &mut metadata,
                "archive-earliest-email-date",
                date_value(epoch_ms),
                "embedded:zip-entry:rfc5322:date",
                "high",
            );
        }
        if let Some(epoch_ms) = latest_email_date_ms {
            set_production_time(
                &mut metadata,
                epoch_ms,
                "embedded:zip-entry:rfc5322:latest-date",
                "high",
            );
        }
        if email_scan_bounded {
            add_evidence(
                &mut metadata,
                "metadata-probe-warning",
                "zip-email-header:bounded-scan-incomplete",
                "local:metadata-probe:rust-zip-email",
                "high",
            );
        }
        if email_header_parse_failures > 0 {
            add_evidence(
                &mut metadata,
                "metadata-probe-warning",
                format!("zip-email-header:parse-failures:{email_header_parse_failures}"),
                "local:metadata-probe:rust-zip-email",
                "high",
            );
        }
    }
    for name in top_level_names {
        push_context(
            &mut metadata,
            "archive-top-level-entry",
            &name,
            "embedded:zip-central-directory",
        );
    }
    for class in content_classes {
        push_context(
            &mut metadata,
            "archive-content-class",
            class,
            "embedded:zip-central-directory",
        );
    }
    metadata
}

#[cfg(not(coverage))]
fn zip_datetime_epoch_ms(value: zip::DateTime) -> Option<u64> {
    if !value.is_valid()
        || (
            value.year(),
            value.month(),
            value.day(),
            value.hour(),
            value.minute(),
            value.second(),
        ) == (1980, 1, 1, 0, 0, 0)
    {
        return None;
    }
    let date = date_epoch_ms(
        i32::from(value.year()),
        u32::from(value.month()),
        u32::from(value.day()),
    )?;
    let seconds = u64::from(value.hour()) * 3_600
        + u64::from(value.minute()) * 60
        + u64::from(value.second());
    Some(date + seconds * 1_000)
}

#[cfg(not(coverage))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct IncompleteDownloadSignatureScan {
    file_bytes: u64,
    zip_eocd_count: u64,
    zip_eocd_offsets: Vec<u64>,
}

#[cfg(not(coverage))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EmbeddedZipStructuralCandidate {
    start: u64,
    end: u64,
    entry_count: usize,
    central_directory_bytes: u64,
}

#[cfg(not(coverage))]
fn scan_incomplete_download_signatures(
    path: &Path,
) -> Result<IncompleteDownloadSignatureScan, &'static str> {
    const ZIP_EOCD_SIGNATURE: &[u8; 4] = b"PK\x05\x06";
    const SIGNATURE_OVERLAP_BYTES: usize = ZIP_EOCD_SIGNATURE.len() - 1;

    let file = std::fs::File::open(path).map_err(|_| "open-failed")?;
    let file_bytes = file.metadata().map_err(|_| "metadata-failed")?.len();
    if file_bytes > MAX_INCOMPLETE_DOWNLOAD_SCAN_BYTES {
        return Err("size-limit-exceeded");
    }

    let mut reader = std::io::BufReader::with_capacity(
        INCOMPLETE_DOWNLOAD_SCAN_CHUNK_BYTES,
        file.take(file_bytes),
    );
    let mut chunk = vec![0u8; INCOMPLETE_DOWNLOAD_SCAN_CHUNK_BYTES];
    let mut overlap = Vec::with_capacity(SIGNATURE_OVERLAP_BYTES);
    let mut bytes_read = 0u64;
    let mut zip_eocd_count = 0u64;
    let mut zip_eocd_offsets = Vec::new();
    loop {
        let read = reader.read(&mut chunk).map_err(|_| "read-failed")?;
        if read == 0 {
            break;
        }
        let window_offset = bytes_read.saturating_sub(overlap.len() as u64);
        let mut window = Vec::with_capacity(overlap.len() + read);
        window.extend_from_slice(&overlap);
        window.extend_from_slice(&chunk[..read]);
        for relative in memchr::memmem::find_iter(&window, ZIP_EOCD_SIGNATURE) {
            zip_eocd_count = zip_eocd_count.saturating_add(1);
            if zip_eocd_offsets.len() < MAX_INCOMPLETE_DOWNLOAD_EOCD_OFFSETS {
                zip_eocd_offsets.push(window_offset + relative as u64);
            }
        }
        overlap.clear();
        let overlap_start = window.len().saturating_sub(SIGNATURE_OVERLAP_BYTES);
        overlap.extend_from_slice(&window[overlap_start..]);
        bytes_read = bytes_read.saturating_add(read as u64);
    }
    if bytes_read != file_bytes {
        return Err("size-changed-during-scan");
    }

    Ok(IncompleteDownloadSignatureScan {
        file_bytes,
        zip_eocd_count,
        zip_eocd_offsets,
    })
}

#[cfg(not(coverage))]
fn embedded_zip_structural_candidate(
    file: &mut std::fs::File,
    file_bytes: u64,
    eocd_offset: u64,
) -> Option<EmbeddedZipStructuralCandidate> {
    const EOCD_BYTES: u64 = 22;
    const CENTRAL_HEADER_BYTES: usize = 46;
    const EOCD_SIGNATURE: &[u8; 4] = b"PK\x05\x06";
    const CENTRAL_SIGNATURE: &[u8; 4] = b"PK\x01\x02";
    const LOCAL_SIGNATURE: &[u8; 4] = b"PK\x03\x04";

    let u16_at = |bytes: &[u8], offset: usize| {
        bytes
            .get(offset..offset + 2)
            .map(|value| u16::from_le_bytes([value[0], value[1]]))
    };
    let u32_at = |bytes: &[u8], offset: usize| {
        bytes
            .get(offset..offset + 4)
            .map(|value| u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
    };

    let mut eocd = [0u8; EOCD_BYTES as usize];
    file.seek(SeekFrom::Start(eocd_offset)).ok()?;
    file.read_exact(&mut eocd).ok()?;
    if eocd.get(..4) != Some(EOCD_SIGNATURE.as_slice()) {
        return None;
    }
    let disk = u16_at(&eocd, 4)?;
    let central_disk = u16_at(&eocd, 6)?;
    let entries_on_disk = u16_at(&eocd, 8)?;
    let total_entries = u16_at(&eocd, 10)?;
    let central_directory_bytes = u64::from(u32_at(&eocd, 12)?);
    let central_directory_offset = u64::from(u32_at(&eocd, 16)?);
    let comment_bytes = u64::from(u16_at(&eocd, 20)?);
    let entry_count = usize::from(total_entries);
    if disk != 0
        || central_disk != 0
        || entries_on_disk != total_entries
        || total_entries == 0
        || total_entries == u16::MAX
        || entry_count > MAX_ZIP_METADATA_ENTRIES
        || central_directory_bytes == u64::from(u32::MAX)
        || central_directory_bytes > MAX_ZIP_CENTRAL_DIRECTORY_BYTES
        || central_directory_offset == u64::from(u32::MAX)
    {
        return None;
    }
    let end = eocd_offset
        .checked_add(EOCD_BYTES)?
        .checked_add(comment_bytes)?;
    if end > file_bytes {
        return None;
    }
    let relative_central_end = central_directory_offset.checked_add(central_directory_bytes)?;
    let start = eocd_offset.checked_sub(relative_central_end)?;
    let central_start = start.checked_add(central_directory_offset)?;
    if central_start.checked_add(central_directory_bytes)? != eocd_offset {
        return None;
    }

    let central_len = usize::try_from(central_directory_bytes).ok()?;
    let mut central = vec![0u8; central_len];
    file.seek(SeekFrom::Start(central_start)).ok()?;
    file.read_exact(&mut central).ok()?;
    let mut cursor = 0usize;
    let mut first_local_header = None;
    let mut last_local_header = None;
    for _ in 0..entry_count {
        let fixed_end = cursor.checked_add(CENTRAL_HEADER_BYTES)?;
        let header = central.get(cursor..fixed_end)?;
        if header.get(..4) != Some(CENTRAL_SIGNATURE.as_slice())
            || u32_at(header, 20)? == u32::MAX
            || u32_at(header, 24)? == u32::MAX
            || u16_at(header, 34)? != 0
            || u32_at(header, 42)? == u32::MAX
        {
            return None;
        }
        let variable_bytes = usize::from(u16_at(header, 28)?)
            .checked_add(usize::from(u16_at(header, 30)?))?
            .checked_add(usize::from(u16_at(header, 32)?))?;
        cursor = fixed_end.checked_add(variable_bytes)?;
        if cursor > central.len() {
            return None;
        }
        let local_header = start.checked_add(u64::from(u32_at(header, 42)?))?;
        if local_header.checked_add(4)? > central_start {
            return None;
        }
        first_local_header.get_or_insert(local_header);
        last_local_header = Some(local_header);
    }
    if cursor != central.len() {
        return None;
    }
    for local_header in [first_local_header?, last_local_header?] {
        let mut signature = [0u8; 4];
        file.seek(SeekFrom::Start(local_header)).ok()?;
        file.read_exact(&mut signature).ok()?;
        if signature != *LOCAL_SIGNATURE {
            return None;
        }
    }
    Some(EmbeddedZipStructuralCandidate {
        start,
        end,
        entry_count,
        central_directory_bytes,
    })
}

#[cfg(not(coverage))]
fn incomplete_download_metadata(path: &Path) -> ContentMetadata {
    let mut metadata = ContentMetadata::default();
    let scan = match scan_incomplete_download_signatures(path) {
        Ok(scan) => scan,
        Err(reason) => {
            add_evidence(
                &mut metadata,
                "metadata-probe-warning",
                format!("incomplete-download-signature-scan:{reason}"),
                "local:metadata-probe:rust-signature-scan",
                "high",
            );
            return metadata;
        }
    };
    add_evidence(
        &mut metadata,
        "incomplete-download-file-bytes",
        scan.file_bytes.to_string(),
        "filesystem:bounded-content-signature-scan",
        "high",
    );
    add_evidence(
        &mut metadata,
        "incomplete-download-embedded-zip-eocd-count",
        scan.zip_eocd_count.to_string(),
        "content-signature:zip-eocd-stream-scan",
        "medium",
    );
    add_evidence(
        &mut metadata,
        "incomplete-download-embedded-zip-eocd-offsets-retained",
        scan.zip_eocd_offsets.len().to_string(),
        "content-signature:zip-eocd-stream-scan",
        "high",
    );
    if scan.zip_eocd_count > scan.zip_eocd_offsets.len() as u64 {
        add_evidence(
            &mut metadata,
            "metadata-probe-warning",
            "incomplete-download-signature-scan:offset-limit-reached",
            "local:metadata-probe:rust-signature-scan",
            "high",
        );
    }
    for offset in &scan.zip_eocd_offsets {
        add_evidence(
            &mut metadata,
            "incomplete-download-embedded-zip-eocd-offset",
            offset.to_string(),
            "content-signature:zip-eocd-stream-scan",
            "medium",
        );
    }
    let structural_candidates = match std::fs::File::open(path) {
        Ok(mut file)
            if file
                .metadata()
                .is_ok_and(|value| value.len() == scan.file_bytes) =>
        {
            scan.zip_eocd_offsets
                .iter()
                .copied()
                .filter_map(|offset| {
                    embedded_zip_structural_candidate(&mut file, scan.file_bytes, offset)
                })
                .collect::<Vec<_>>()
        }
        Ok(_) => {
            add_evidence(
                &mut metadata,
                "metadata-probe-warning",
                "incomplete-download-structural-scan:size-changed",
                "local:metadata-probe:rust-signature-scan",
                "high",
            );
            Vec::new()
        }
        Err(_) => {
            add_evidence(
                &mut metadata,
                "metadata-probe-warning",
                "incomplete-download-structural-scan:reopen-failed",
                "local:metadata-probe:rust-signature-scan",
                "high",
            );
            Vec::new()
        }
    };
    add_evidence(
        &mut metadata,
        "incomplete-download-structural-zip-candidate-count",
        structural_candidates.len().to_string(),
        "content-structure:zip-central-directory",
        "medium",
    );
    for candidate in structural_candidates {
        add_evidence(
            &mut metadata,
            "incomplete-download-structural-zip-candidate",
            format!(
                "start={};end={};entries={};central-directory-bytes={}",
                candidate.start,
                candidate.end,
                candidate.entry_count,
                candidate.central_directory_bytes
            ),
            "content-structure:zip-central-directory",
            "medium",
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

#[cfg(not(coverage))]
fn read_bounded_prefix(path: &Path, limit: usize) -> Result<(Vec<u8>, bool), String> {
    let file = std::fs::File::open(path).map_err(|_| "open-failed".to_string())?;
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024).saturating_add(1));
    file.take(limit.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| "read-failed".to_string())?;
    let truncated = bytes.len() > limit;
    bytes.truncate(limit);
    Ok((bytes, truncated))
}

#[cfg(not(coverage))]
fn email_header_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
        .or_else(|| {
            bytes
                .windows(2)
                .position(|window| window == b"\n\n")
                .map(|index| index + 2)
        })
}

/// Parse only a bounded RFC 5322 header block. Message bodies and attachments are deliberately
/// not parsed because the planner needs lineage metadata, not message contents.
#[cfg(not(coverage))]
fn email_metadata_from_header(
    bytes: &[u8],
    truncated: bool,
    evidence_source: &str,
    date_source: &str,
    warning_source: &str,
) -> ContentMetadata {
    let mut metadata = ContentMetadata::default();
    let Some(header_end) = email_header_end(&bytes) else {
        add_evidence(
            &mut metadata,
            "metadata-probe-warning",
            if truncated {
                "email-header:bounded-header-terminator-not-found"
            } else {
                "email-header:header-terminator-not-found"
            },
            warning_source,
            "high",
        );
        return metadata;
    };
    let Some(message) = mail_parser::MessageParser::default().parse_headers(&bytes[..header_end])
    else {
        add_evidence(
            &mut metadata,
            "metadata-probe-warning",
            "email-header:rfc5322-parse-failed",
            warning_source,
            "high",
        );
        return metadata;
    };

    add_evidence(
        &mut metadata,
        "email-header-bytes-inspected",
        header_end.to_string(),
        evidence_source,
        "high",
    );
    add_evidence(
        &mut metadata,
        "email-body-inspected",
        "false",
        evidence_source,
        "high",
    );
    if let Some(date) = message.date() {
        if let Ok(seconds) = u64::try_from(date.to_timestamp()) {
            if let Some(epoch_ms) = seconds.checked_mul(1_000) {
                set_production_time(&mut metadata, epoch_ms, date_source, "high");
            }
        }
    }
    if let Some(subject) = message
        .subject()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let bounded = subject.chars().take(500).collect::<String>();
        metadata.title = Some(bounded.clone());
        push_context(&mut metadata, "email-subject", &bounded, evidence_source);
    }
    if let Some(thread) = message
        .thread_name()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        push_context(&mut metadata, "email-thread", thread, evidence_source);
    }
    if let Some(author) = message
        .return_address()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let bounded = author.chars().take(320).collect::<String>();
        metadata.authors.push(bounded.clone());
        add_evidence(
            &mut metadata,
            "email-author",
            bounded,
            evidence_source,
            "high",
        );
    }
    for (field, value) in [
        ("email-message-id", message.message_id()),
        ("email-in-reply-to", message.header_raw("In-Reply-To")),
        ("email-references", message.header_raw("References")),
    ] {
        if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
            add_evidence(
                &mut metadata,
                field,
                value.chars().take(500).collect::<String>(),
                evidence_source,
                "high",
            );
        }
    }
    metadata
}

/// Read only the bounded RFC 5322 header block from a standalone message.
#[cfg(not(coverage))]
fn email_metadata(path: &Path) -> ContentMetadata {
    let (bytes, truncated) = match read_bounded_prefix(path, MAX_EMAIL_HEADER_BYTES) {
        Ok(value) => value,
        Err(reason) => {
            let mut metadata = ContentMetadata::default();
            add_evidence(
                &mut metadata,
                "metadata-probe-warning",
                format!("email-header:{reason}"),
                "local:metadata-probe:rust-rfc5322",
                "high",
            );
            return metadata;
        }
    };
    email_metadata_from_header(
        &bytes,
        truncated,
        "local:metadata-probe:bounded-rfc5322-header",
        "embedded:rfc5322:date",
        "local:metadata-probe:rust-rfc5322",
    )
}

#[cfg(not(coverage))]
fn contains_ascii_case_insensitive(bytes: &[u8], needle: &[u8]) -> bool {
    bytes
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

/// Audacity 3 projects are SQLite containers. Their schema proves application context but does
/// not carry a trustworthy creation date, so the production-time fallback remains review-only.
#[cfg(not(coverage))]
fn audacity_project_metadata(path: &Path) -> ContentMetadata {
    let mut metadata = ContentMetadata::default();
    let (bytes, _) = match read_bounded_prefix(path, MAX_AUDACITY_SCHEMA_PROBE_BYTES) {
        Ok(value) => value,
        Err(reason) => {
            add_evidence(
                &mut metadata,
                "metadata-probe-warning",
                format!("audacity-aup3:{reason}"),
                "local:metadata-probe:rust-aup3",
                "high",
            );
            return metadata;
        }
    };
    if !bytes.starts_with(b"SQLite format 3\0") {
        add_evidence(
            &mut metadata,
            "metadata-probe-warning",
            "audacity-aup3:sqlite-header-missing",
            "local:metadata-probe:rust-aup3",
            "high",
        );
        return metadata;
    }
    add_evidence(
        &mut metadata,
        "container-format",
        "audacity-aup3-sqlite3",
        "embedded:audacity:sqlite-header",
        "high",
    );
    push_context(
        &mut metadata,
        "creating-application",
        "Audacity",
        "embedded:audacity:sqlite-header",
    );
    for table in ["project", "autosave", "sampleblocks"] {
        let signature = format!("CREATE TABLE {table}");
        if contains_ascii_case_insensitive(&bytes, signature.as_bytes()) {
            add_evidence(
                &mut metadata,
                "audacity-schema-table",
                table,
                "embedded:audacity:sqlite-schema",
                "high",
            );
        }
    }
    if !metadata
        .evidence
        .iter()
        .any(|evidence| evidence.field == "audacity-schema-table" && evidence.value == "project")
    {
        add_evidence(
            &mut metadata,
            "metadata-probe-warning",
            "audacity-aup3:project-schema-signature-missing",
            "local:metadata-probe:rust-aup3",
            "medium",
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
    add_evidence(
        &mut metadata,
        "dataset-sampled-worksheets",
        profile.sampled_worksheets.to_string(),
        &source,
        "medium",
    );
    for worksheet in &profile.worksheet_names {
        add_evidence(
            &mut metadata,
            "dataset-worksheet",
            worksheet,
            &source,
            "high",
        );
    }
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
fn probe_content_metadata_with_general(
    path: &Path,
    prefetched_general: Option<ContentMetadata>,
) -> ContentMetadata {
    probe_content_metadata_with_general_inner(path, prefetched_general, true)
}

#[cfg(not(coverage))]
fn probe_content_metadata_for_planner(
    path: &Path,
    prefetched_general: Option<ContentMetadata>,
) -> ContentMetadata {
    // Download origin and quarantine are useful audit context but are not production metadata.
    // Keep them out of the planner's per-file subprocess budget so embedded/format metadata can
    // be collected for more candidates without weakening the lineage precedence rules.
    probe_content_metadata_with_general_inner(path, prefetched_general, false)
}

#[cfg(not(coverage))]
fn probe_content_metadata_with_general_inner(
    path: &Path,
    prefetched_general: Option<ContentMetadata>,
    include_macos_provenance: bool,
) -> ContentMetadata {
    if source_content_is_dataless(path) {
        return ContentMetadata::default();
    }
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    // Transient downloads and raw multipart members do not represent standalone payloads.
    // ExifTool can spend the full timeout trying to infer their format, so retain only the
    // lightweight acquisition/sibling-set evidence for these fail-closed candidates.
    let general = if should_probe_general_metadata(path) {
        prefetched_general.unwrap_or_else(|| exiftool_metadata(path))
    } else {
        ContentMetadata::default()
    };
    let format_specific = match extension.as_str() {
        "m4a" | "mp4" | "m4v" | "mov" | "mkv" | "avi" | "wav" | "mp3" | "flac" | "aiff" => {
            ffprobe_metadata(path)
        }
        "pdf" => pdfinfo_metadata(path),
        "zip" => zip_archive_metadata(path),
        "eml" => email_metadata(path),
        "aup3" => audacity_project_metadata(path),
        "crdownload" => incomplete_download_metadata(path),
        "xlsx" | "xlsm" => merge_metadata(
            zipped_document_metadata(path, "docProps/core.xml"),
            dataset_content_metadata(path),
        ),
        "ods" => merge_metadata(
            zipped_document_metadata(path, "meta.xml"),
            dataset_content_metadata(path),
        ),
        "xls" | "xlsb" => dataset_content_metadata(path),
        "docx" | "pptx" => zipped_document_metadata(path, "docProps/core.xml"),
        "odt" | "odp" => zipped_document_metadata(path, "meta.xml"),
        "csv" | "tsv" | "parquet" | "feather" | "arrow" | "sav" | "sas7bdat" | "dta" | "rdata"
        | "rds" | "sqlite" | "sqlite3" | "db" | "db3" | "sql" | "jsonl" => {
            dataset_content_metadata(path)
        }
        _ if multipart_archive_part(path).is_some() => multipart_archive_metadata(path),
        _ => ContentMetadata::default(),
    };
    let metadata = merge_metadata(general, format_specific);
    if include_macos_provenance {
        merge_metadata(metadata, macos_file_provenance_metadata(path))
    } else {
        metadata
    }
}

/// Reuse the cloud planner's bounded embedded/acquisition metadata probes in read-only audit
/// commands. The returned acquisition date remains context evidence and is never promoted to a
/// production date.
#[cfg(not(coverage))]
pub(crate) fn probe_content_metadata_for_audit(path: &Path) -> ContentMetadata {
    probe_content_metadata_with_general(path, None)
}

fn should_probe_general_metadata(path: &Path) -> bool {
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    archive_kind(path) != Some(ArchiveKind::IncompleteDownload)
        && archive_kind(path) != Some(ArchiveKind::SensitiveConfig)
        && multipart_archive_part(path).is_none()
        && !matches!(
            extension.as_str(),
            "eml" | "aup3" | "bak" | "db" | "db3" | "sqlite" | "sqlite3"
        )
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
    let archive_class = |expected: &str| {
        metadata
            .evidence
            .iter()
            .any(|evidence| evidence.field == "archive-content-class" && evidence.value == expected)
    };
    if archive_class("structured-data") {
        reasons.push("archive-contains-structured-data".into());
    }
    if archive_class("recording-media") {
        reasons.push("archive-contains-recording-media".into());
    }
    if archive_class("secret-like-path") {
        reasons.push("archive-contains-secret-like-path".into());
    }
    if metadata.evidence.iter().any(|evidence| {
        evidence.field == "incomplete-download-embedded-zip-eocd-count"
            && evidence.value.parse::<u64>().is_ok_and(|count| count > 0)
    }) {
        reasons.push("incomplete-download-contains-zip-fragment".into());
    }
    if metadata.evidence.iter().any(|evidence| {
        evidence.field == "incomplete-download-structural-zip-candidate-count"
            && evidence.value.parse::<u64>().is_ok_and(|count| count > 0)
    }) {
        reasons.push("incomplete-download-has-structural-zip-candidate".into());
    }
    if metadata.evidence.iter().any(|evidence| {
        evidence.field == "archive-encrypted-entry-count"
            && evidence.value.parse::<u64>().is_ok_and(|count| count > 0)
    }) {
        reasons.push("archive-contains-encrypted-entries".into());
    }
    if metadata.evidence.iter().any(|evidence| {
        evidence.field == "archive-unsafe-path-entry-count"
            && evidence.value.parse::<u64>().is_ok_and(|count| count > 0)
    }) {
        reasons.push("archive-contains-unsafe-entry-path".into());
    }
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
            "기업 분석",
            "기업분석",
            "분석 보고서",
            "분석보고서",
            "실사 보고서",
            "실사보고서",
            "company analysis",
            "business analysis",
            "due diligence",
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
    // Spreadsheet containers can carry personal or confidential cell data even when their
    // document-level metadata is complete and innocuous. Until DiskSage profiles worksheet
    // contents, never let a spreadsheet become an automatic cloud-copy candidate.
    if matches!(
        extension.as_str(),
        "xls" | "xlsx" | "xlsm" | "xlsb" | "ods" | "numbers"
    ) {
        reasons.push("spreadsheet-content-needs-review".into());
    }
    if matches!(extension.as_str(), "wav" | "mp3" | "m4a" | "flac" | "aiff") {
        reasons.push("recording-may-contain-sensitive-speech".into());
    }
    if extension == "eml" {
        reasons.push("email-content-needs-review".into());
    }
    if extension == "aup3" {
        reasons.push("audacity-project-recording-content-needs-review".into());
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
        "기업 분석",
        "기업분석",
        "분석 보고서",
        "분석보고서",
        "실사 보고서",
        "실사보고서",
        "company analysis",
        "business analysis",
        "due diligence",
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
                | "archive-contains-structured-data"
                | "archive-contains-recording-media"
                | "archive-contains-secret-like-path"
                | "archive-contains-encrypted-entries"
                | "archive-contains-unsafe-entry-path"
                | "incomplete-download-contains-zip-fragment"
                | "incomplete-download-has-structural-zip-candidate"
                | "structured-data-may-contain-personal-data"
                | "spreadsheet-content-needs-review"
                | "recording-may-contain-sensitive-speech"
                | "email-content-needs-review"
                | "audacity-project-recording-content-needs-review"
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
        CloudAccountScope::Organization if sensitive_context => {
            vec![ORGANIZATION_TENANT_AUTHORITY_REVIEW_REASON.into()]
        }
        CloudAccountScope::Personal | CloudAccountScope::Organization => Vec::new(),
    }
}

fn source_blocked_reason(
    path: &Path,
    kind: ArchiveKind,
    metadata: &ContentMetadata,
) -> Option<String> {
    if kind == ArchiveKind::SensitiveConfig || is_sensitive_config_path(path) {
        return Some("sensitive-config-file".into());
    }
    if path_inside_managed_file_provider_storage(path) {
        return Some("system-managed-file-provider-storage".into());
    }
    if path_inside_managed_photo_library(path) {
        return Some("system-managed-photos-library-data".into());
    }
    if kind == ArchiveKind::IncompleteDownload {
        return Some("incomplete-download".into());
    }
    if multipart_archive_part(path).is_some() {
        return Some("multipart-archive-atomic-copy-required".into());
    }
    if source_content_is_dataless(path) {
        return Some("source-content-not-local".into());
    }
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if extension == "zip"
        && metadata.evidence.iter().any(|evidence| {
            evidence.field == "metadata-probe-warning"
                && matches!(
                    evidence.source.as_str(),
                    "local:metadata-probe:zipinfo" | "local:metadata-probe:rust-zip"
                )
        })
    {
        return Some("archive-index-unreadable".into());
    }
    None
}

/// File Provider's private storage and download staging trees are owned by macOS. Their files
/// are implementation state, not user payloads; only a provider-aware operation may reclaim them.
fn path_inside_managed_file_provider_storage(path: &Path) -> bool {
    let mut previous = String::new();
    path.components().any(|component| {
        let name = normalized_account_text(&component.as_os_str().to_string_lossy());
        let managed = name == "file provider storage"
            || (previous == "library"
                && matches!(name.as_str(), "mobile documents" | "cloudstorage"))
            || (previous == "application support" && name == "fileprovider");
        previous = name;
        managed
    })
}

/// Photos databases are individually archive-shaped but are owned by the Photos package.
/// Moving one member would corrupt the library; only a future package-aware operation may handle
/// the bundle as a whole.
fn path_inside_managed_photo_library(path: &Path) -> bool {
    path.components().any(|component| {
        let name = normalized_account_text(&component.as_os_str().to_string_lossy());
        name.ends_with(".photoslibrary") || name.ends_with(".photolibrary")
    })
}

fn planner_blocked_reason(
    path: &Path,
    kind: ArchiveKind,
    metadata: &ContentMetadata,
    destination: &Path,
    provider: CloudProvider,
    expected_bytes: u64,
) -> Option<String> {
    if destination.exists() {
        return Some(
            crate::provider_sync::existing_destination_sync_blocker(
                provider,
                destination,
                expected_bytes,
            )
            .unwrap_or("destination-exists")
            .into(),
        );
    }
    source_blocked_reason(path, kind, metadata)
}

fn provider_destination_path_blocked_reason(
    cloud_root: &CloudRoot,
    destination: &Path,
) -> Option<String> {
    if cloud_root.provider != CloudProvider::Onedrive {
        return None;
    }
    let Ok(relative) = destination.strip_prefix(Path::new(&cloud_root.path)) else {
        return Some("destination-outside-cloud-root".into());
    };
    let mut relative_chars = 0;
    for component in relative.components() {
        let std::path::Component::Normal(component) = component else {
            return Some("onedrive-path-invalid".into());
        };
        let Some(component) = component.to_str() else {
            return Some("onedrive-path-not-unicode".into());
        };
        let normalized = component.nfc().collect::<String>();
        if normalized.is_empty() {
            return Some("onedrive-path-invalid".into());
        }
        if normalized.chars().count() > ONEDRIVE_MAX_PATH_COMPONENT_CHARS
            || normalized.as_bytes().len() > ONEDRIVE_MAX_PATH_COMPONENT_BYTES
        {
            return Some("onedrive-path-component-too-long".into());
        }
        relative_chars += normalized.chars().count();
    }
    if relative.components().count().saturating_sub(1) + relative_chars
        > ONEDRIVE_MAX_RELATIVE_PATH_CHARS
    {
        return Some("onedrive-path-too-long".into());
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
    if metadata_is_dataless(&before) {
        return Err("duplicate-content-source-not-local".into());
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
    if metadata_is_dataless(&after)
        || after.len() != expected_bytes
        || millis(after.modified()) != before_modified_ms
    {
        return Err("duplicate-content-source-changed".into());
    }
    Ok(hasher.finalize())
}

#[cfg(not(coverage))]
fn source_snapshot_file_unchanged(file: &FileFact) -> bool {
    let Ok(metadata) = std::fs::metadata(&file.path) else {
        return false;
    };
    metadata.is_file()
        && metadata.len() == file.bytes
        && millis(metadata.modified()) == file.modified_ms
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

fn duplicate_confidence_rank(value: &str) -> u8 {
    match value {
        "high" => 3,
        "medium" => 2,
        "low" => 1,
        _ => 0,
    }
}

fn embedded_metadata_richness(candidate: &CloudCandidate) -> usize {
    usize::from(candidate.content_title.is_some())
        + candidate.content_authors.len()
        + usize::from(candidate.duration_ms.is_some())
        + usize::from(candidate.dataset_profile.is_some())
        + candidate
            .metadata_evidence
            .iter()
            .filter(|evidence| evidence.source.starts_with("embedded:"))
            .count()
}

fn source_lineage_context_richness(candidate: &CloudCandidate) -> usize {
    candidate.content_context.len()
}

fn filename_looks_like_copy(path: &str) -> bool {
    let path = Path::new(path);
    let stem = path
        .file_stem()
        .map(|value| value.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    if [" copy", "-copy", "_copy", "복사본"]
        .iter()
        .any(|marker| stem.contains(marker))
    {
        return true;
    }
    let Some(open) = stem.rfind('(') else {
        return false;
    };
    let suffix = stem[open + 1..].trim_end_matches(')').trim();
    stem.ends_with(')') && !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
}

fn path_looks_regenerable_or_quarantined(path: &str) -> bool {
    Path::new(path).components().any(|component| {
        let value = component.as_os_str().to_string_lossy().to_lowercase();
        matches!(
            value.as_str(),
            "quarantine"
                | "exact-duplicates"
                | "duplicates"
                | ".trash"
                | "trash"
                | "tmp"
                | "temp"
                | "cache"
        )
    })
}

fn retain_max_by_key<T: Ord, F: Fn(usize) -> T>(indices: &mut Vec<usize>, key: F) -> bool {
    let before = indices.len();
    let Some(best) = indices.iter().copied().map(|index| key(index)).max() else {
        return false;
    };
    indices.retain(|index| key(*index) == best);
    indices.len() < before
}

fn retain_min_by_key<T: Ord, F: Fn(usize) -> T>(indices: &mut Vec<usize>, key: F) -> bool {
    let before = indices.len();
    let Some(best) = indices.iter().copied().map(|index| key(index)).min() else {
        return false;
    };
    indices.retain(|index| key(*index) == best);
    indices.len() < before
}

fn record_recommendation_stage(
    reduced: bool,
    remaining_len: usize,
    reason: &str,
    stage_confidence: &str,
    reasons: &mut Vec<String>,
    confidence: &mut Option<String>,
) {
    if reduced {
        reasons.push(reason.to_string());
        if remaining_len == 1 && confidence.is_none() {
            *confidence = Some(stage_confidence.to_string());
        }
    }
}

fn recommend_exact_duplicate_canonical(
    candidates: &[CloudCandidate],
    exact_matches: &[usize],
) -> (usize, String, Vec<String>) {
    let mut remaining = exact_matches.to_vec();
    let mut reasons = Vec::new();
    let mut confidence = None;

    let reduced = retain_max_by_key(&mut remaining, |index| {
        candidates[index]
            .production_time_source
            .starts_with("embedded:")
    });
    record_recommendation_stage(
        reduced,
        remaining.len(),
        "embedded-production-time-preferred",
        "high",
        &mut reasons,
        &mut confidence,
    );

    let reduced = retain_max_by_key(&mut remaining, |index| {
        duplicate_confidence_rank(&candidates[index].production_time_confidence)
    });
    record_recommendation_stage(
        reduced,
        remaining.len(),
        "higher-production-time-confidence",
        "high",
        &mut reasons,
        &mut confidence,
    );

    let reduced = retain_max_by_key(&mut remaining, |index| {
        embedded_metadata_richness(&candidates[index])
    });
    record_recommendation_stage(
        reduced,
        remaining.len(),
        "richer-embedded-metadata-preferred",
        "high",
        &mut reasons,
        &mut confidence,
    );

    let reduced = retain_max_by_key(&mut remaining, |index| {
        source_lineage_context_richness(&candidates[index])
    });
    record_recommendation_stage(
        reduced,
        remaining.len(),
        "richer-source-lineage-context-preferred",
        "high",
        &mut reasons,
        &mut confidence,
    );

    let reduced = retain_min_by_key(&mut remaining, |index| {
        path_looks_regenerable_or_quarantined(&candidates[index].relative_path)
    });
    record_recommendation_stage(
        reduced,
        remaining.len(),
        "non-quarantine-path-preferred",
        "medium",
        &mut reasons,
        &mut confidence,
    );

    let reduced = retain_min_by_key(&mut remaining, |index| {
        filename_looks_like_copy(&candidates[index].relative_path)
    });
    record_recommendation_stage(
        reduced,
        remaining.len(),
        "non-copy-marked-filename-preferred",
        "medium",
        &mut reasons,
        &mut confidence,
    );

    let reduced = retain_min_by_key(&mut remaining, |index| {
        let created_ms = candidates[index].created_ms;
        if created_ms == 0 {
            u64::MAX
        } else {
            created_ms
        }
    });
    record_recommendation_stage(
        reduced,
        remaining.len(),
        "filesystem-created-time-tiebreaker",
        "low",
        &mut reasons,
        &mut confidence,
    );

    remaining.sort_by(|left, right| {
        candidates[*left]
            .relative_path
            .cmp(&candidates[*right].relative_path)
            .then_with(|| {
                candidates[*left]
                    .metadata_fingerprint
                    .cmp(&candidates[*right].metadata_fingerprint)
            })
    });
    if remaining.len() > 1 {
        reasons.push("stable-path-tiebreaker".into());
    }
    let recommended = remaining
        .first()
        .copied()
        .unwrap_or_else(|| exact_matches[0]);
    (
        recommended,
        confidence.unwrap_or_else(|| "low".into()),
        reasons,
    )
}

fn exact_duplicate_cluster_fingerprint(
    sha256: &str,
    blake3: &str,
    bytes_per_candidate: u64,
    member_metadata_fingerprints: &[String],
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-exact-duplicate-cluster-v1\0");
    for value in [sha256.as_bytes(), blake3.as_bytes()] {
        hash_review_value(&mut hasher, value);
    }
    hash_review_value(&mut hasher, &bytes_per_candidate.to_le_bytes());
    for fingerprint in member_metadata_fingerprints {
        hash_review_value(&mut hasher, fingerprint.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

/// Hash only non-blocked candidates that share a byte length. Exact duplicates remain movable,
/// but require an operator to select the canonical lineage instead of silently copying every path.
#[cfg(not(coverage))]
fn mark_exact_duplicate_candidates(
    candidates: &mut [CloudCandidate],
    cached_digests: Option<&BTreeMap<PathBuf, Result<ContentDigests, String>>>,
) -> ExactDuplicateSummary {
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
            let cached = cached_digests
                .and_then(|digests| digests.get(Path::new(&candidate.src)))
                .cloned();
            match cached.unwrap_or_else(|| {
                hash_duplicate_candidate(Path::new(&candidate.src), candidate.bytes)
            }) {
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
            let (recommended_index, recommendation_confidence, recommendation_reason_codes) =
                recommend_exact_duplicate_canonical(candidates, &exact_matches);
            let recommended_canonical_metadata_fingerprint =
                candidates[recommended_index].metadata_fingerprint.clone();
            let mut member_metadata_fingerprints = exact_matches
                .iter()
                .map(|index| candidates[*index].metadata_fingerprint.clone())
                .collect::<Vec<_>>();
            member_metadata_fingerprints.sort();
            let cluster_fingerprint = exact_duplicate_cluster_fingerprint(
                &sha256,
                &blake3,
                bytes_per_candidate,
                &member_metadata_fingerprints,
            );
            let recommendation_reasons = recommendation_reason_codes.join(",");
            summary.cluster_count += 1;
            summary.candidate_count += exact_matches.len();
            summary.candidate_bytes = summary
                .candidate_bytes
                .saturating_add(bytes_per_candidate.saturating_mul(exact_matches.len() as u64));
            summary.redundant_bytes = summary.redundant_bytes.saturating_add(
                bytes_per_candidate.saturating_mul((exact_matches.len() - 1) as u64),
            );
            summary.clusters.push(ExactDuplicateClusterRecommendation {
                cluster_fingerprint: cluster_fingerprint.clone(),
                candidate_count: exact_matches.len(),
                bytes_per_candidate,
                redundant_bytes: bytes_per_candidate
                    .saturating_mul((exact_matches.len() - 1) as u64),
                recommended_canonical_metadata_fingerprint:
                    recommended_canonical_metadata_fingerprint.clone(),
                recommendation_confidence: recommendation_confidence.clone(),
                recommendation_reason_codes: recommendation_reason_codes.clone(),
                member_metadata_fingerprints,
                requires_human_confirmation: true,
            });
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
                push_candidate_evidence(
                    candidate,
                    "exact-duplicate-cluster-fingerprint",
                    cluster_fingerprint.clone(),
                    "planner:exact-content-cluster",
                    "high",
                );
                push_candidate_evidence(
                    candidate,
                    "exact-duplicate-canonical-recommendation",
                    if index == recommended_index {
                        "preferred"
                    } else {
                        "redundant-copy-candidate"
                    },
                    "planner:metadata-lineage-ranking",
                    &recommendation_confidence,
                );
                push_candidate_evidence(
                    candidate,
                    "exact-duplicate-canonical-recommendation-reasons",
                    recommendation_reasons.clone(),
                    "planner:metadata-lineage-ranking",
                    &recommendation_confidence,
                );
                push_candidate_evidence(
                    candidate,
                    "exact-duplicate-canonical-human-confirmation-required",
                    "true",
                    "planner:metadata-lineage-ranking",
                    "high",
                );
            }
        }
    }
    summary
        .clusters
        .sort_by(|left, right| left.cluster_fingerprint.cmp(&right.cluster_fingerprint));

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

/// Bind a human-visible decision batch without including volatile scan time or capacity state.
///
/// Candidate review fingerprints already bind paths, destination, embedded metadata evidence,
/// and review context. This batch fingerprint additionally binds the source selection policy,
/// complete candidate set, planner blockers, destination scope, totals, and exact-duplicate
/// summary. Sorting makes the result independent of report presentation order while fresh capacity
/// is still required at copy time.
pub const CLOUD_DECISION_BATCH_FINGERPRINT_VERSION: u32 = 2;

pub fn cloud_decision_batch_fingerprint(report: &CloudPlanReport) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-cloud-decision-batch-v2\0");
    for value in [
        report.cloud_root.provider.as_str().as_bytes(),
        report.cloud_root.account_scope.as_str().as_bytes(),
    ] {
        hash_review_value(&mut hasher, value);
    }
    for value in [
        report.candidates.len() as u64,
        report.candidate_bytes,
        report.potentially_reclaimable_bytes,
        report.exact_duplicates.cluster_count as u64,
        report.exact_duplicates.candidate_count as u64,
        report.exact_duplicates.candidate_bytes,
        report.exact_duplicates.redundant_bytes,
    ] {
        hash_review_value(&mut hasher, &value.to_le_bytes());
    }
    match report.source_selection_policy {
        Some(policy) => {
            hash_review_value(&mut hasher, b"1");
            for value in [
                policy.min_size_bytes,
                policy.min_age_days,
                u64::try_from(policy.limit).unwrap_or(u64::MAX),
            ] {
                hash_review_value(&mut hasher, &value.to_le_bytes());
            }
        }
        None => hash_review_value(&mut hasher, b"0"),
    }

    let mut candidates = report.candidates.iter().collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        left.metadata_fingerprint
            .cmp(&right.metadata_fingerprint)
            .then_with(|| left.review_fingerprint.cmp(&right.review_fingerprint))
            .then_with(|| left.blocked_reason.cmp(&right.blocked_reason))
    });
    for candidate in candidates {
        for value in [
            candidate.metadata_fingerprint.as_bytes(),
            candidate.review_fingerprint.as_bytes(),
        ] {
            hash_review_value(&mut hasher, value);
        }
        match &candidate.blocked_reason {
            Some(reason) => {
                hash_review_value(&mut hasher, b"1");
                hash_review_value(&mut hasher, reason.as_bytes());
            }
            None => hash_review_value(&mut hasher, b"0"),
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
    let snapshot = prepare_cloud_archive_source(files, source_root, now_ms, options);
    plan_cloud_archive_from_snapshot(&snapshot, cloud_root)
}

/// Probe destination-independent source evidence once for reuse across cloud-root plans.
pub fn prepare_cloud_archive_source(
    files: &[FileFact],
    source_root: &Path,
    now_ms: u64,
    options: CloudPlanOptions,
) -> CloudSourceSnapshot {
    prepare_cloud_archive_source_with_scan(
        files,
        source_root,
        now_ms,
        options,
        true,
        files.len() as u64,
        Vec::new(),
    )
}

/// Prepare source metadata while retaining whether the bounded filesystem walk was exhaustive.
#[cfg(not(coverage))]
pub fn prepare_cloud_archive_source_from_collection(
    collection: &ArchiveFileCollection,
    source_root: &Path,
    now_ms: u64,
    options: CloudPlanOptions,
) -> CloudSourceSnapshot {
    prepare_cloud_archive_source_with_scan(
        &collection.files,
        source_root,
        now_ms,
        options,
        collection.complete,
        collection.visited_entries,
        collection.stop_reasons.clone(),
    )
}

fn prepare_cloud_archive_source_with_scan(
    files: &[FileFact],
    source_root: &Path,
    now_ms: u64,
    options: CloudPlanOptions,
    source_scan_complete: bool,
    source_scan_visited_entries: u64,
    source_scan_stop_reasons: Vec<String>,
) -> CloudSourceSnapshot {
    #[cfg(not(coverage))]
    let (batched_exiftool, probe_candidate_paths, selected_probe_paths, metadata_probe_started) = {
        let mut probe_candidates = files
            .iter()
            .filter(|file| {
                file.bytes >= options.min_size_bytes
                    && file.modified_ms > 0
                    && now_ms.saturating_sub(file.modified_ms) / DAY_MS >= options.min_age_days
                    && archive_kind(&file.path).is_some()
                    && archive_kind(&file.path) != Some(ArchiveKind::SensitiveConfig)
                    && file
                        .path
                        .strip_prefix(source_root)
                        .is_ok_and(|relative| !relative.as_os_str().is_empty())
                    && file.content_metadata == ContentMetadata::default()
                    && file.path.is_file()
                    && !source_content_is_dataless(&file.path)
            })
            .collect::<Vec<_>>();
        probe_candidates.sort_by(|left, right| {
            right
                .bytes
                .cmp(&left.bytes)
                .then_with(|| left.path.cmp(&right.path))
        });
        let probe_candidate_paths = probe_candidates
            .iter()
            .map(|file| file.path.clone())
            .collect::<BTreeSet<_>>();
        probe_candidates.truncate(MAX_METADATA_PROBE_FILES);
        let selected_probe_paths = probe_candidates
            .iter()
            .map(|file| file.path.clone())
            .collect::<BTreeSet<_>>();
        let paths = probe_candidates
            .iter()
            .filter(|file| should_probe_general_metadata(&file.path))
            .map(|file| file.path.clone())
            .collect::<Vec<_>>();
        let metadata_probe_started = Instant::now();
        (
            exiftool_metadata_batch(&paths),
            probe_candidate_paths,
            selected_probe_paths,
            metadata_probe_started,
        )
    };

    let mut prepared_files = Vec::new();
    #[cfg(not(coverage))]
    let mut verified_regular_files = BTreeSet::new();
    for file in files {
        if file.bytes < options.min_size_bytes || file.modified_ms == 0 {
            continue;
        }
        let age_days = now_ms.saturating_sub(file.modified_ms) / DAY_MS;
        if age_days < options.min_age_days {
            continue;
        }
        if archive_kind(&file.path).is_none() {
            continue;
        }
        let Ok(relative) = file.path.strip_prefix(source_root) else {
            continue;
        };
        if relative.as_os_str().is_empty() {
            continue;
        }
        let mut prepared = file.clone();
        // Coverage builds exercise the deterministic planning core. Content probing is an
        // external-process adapter (ExifTool/ffprobe/pdfinfo/unzip) covered by normal tests and
        // integration smoke runs, so it is kept outside the in-process line-coverage boundary.
        #[cfg(not(coverage))]
        if file.path.is_file() {
            verified_regular_files.insert(file.path.clone());
            if prepared.content_metadata == ContentMetadata::default()
                && selected_probe_paths.contains(&file.path)
                && metadata_probe_started.elapsed() < METADATA_PROBE_TOTAL_TIMEOUT
            {
                prepared.content_metadata = probe_content_metadata_for_planner(
                    &file.path,
                    batched_exiftool.get(&file.path).cloned(),
                );
            } else if prepared.content_metadata == ContentMetadata::default()
                && !source_content_is_dataless(&file.path)
            {
                let failure = if probe_candidate_paths.contains(&file.path)
                    && !selected_probe_paths.contains(&file.path)
                {
                    MetadataProbeFailure::FileLimit
                } else {
                    MetadataProbeFailure::Timeout
                };
                add_probe_warning(
                    &mut prepared.content_metadata,
                    "planner",
                    failure,
                );
            }
        }
        prepared_files.push(prepared);
    }

    CloudSourceSnapshot {
        source_root: source_root.to_path_buf(),
        prepared_at_ms: now_ms,
        options,
        files: prepared_files,
        source_scan_complete,
        source_scan_visited_entries,
        source_scan_stop_reasons,
        #[cfg(not(coverage))]
        verified_regular_files,
    }
}

/// Derive one destination-specific dry-run from an immutable source snapshot.
///
/// Cheap source stat checks and all destination-dependent checks are repeated for every plan.
/// Metadata probes and content hashing are not.
pub fn plan_cloud_archive_from_snapshot(
    snapshot: &CloudSourceSnapshot,
    cloud_root: &CloudRoot,
) -> CloudPlanReport {
    let files = &snapshot.files;
    let source_root = &snapshot.source_root;
    let now_ms = snapshot.prepared_at_ms;
    let options = snapshot.options;
    let source_scan_blocker = (!snapshot.source_scan_complete)
        .then(|| "source-scan-incomplete".to_string());
    let mut candidates = Vec::new();
    for file in files {
        let age_days = now_ms.saturating_sub(file.modified_ms) / DAY_MS;
        let Some(kind) = archive_kind(&file.path) else {
            continue;
        };
        let Ok(relative) = file.path.strip_prefix(source_root) else {
            continue;
        };
        let filename_ms = filename_date_ms(&file.path);
        let filename_publication_month = filename_publication_month(&file.path);
        let mut lineage_metadata = file.content_metadata.clone();
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
        if let Some((year, month)) = filename_publication_month {
            add_filename_publication_month(&mut lineage_metadata, year, month);
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
        #[cfg(not(coverage))]
        let source_snapshot_stale = snapshot.verified_regular_files.contains(&file.path)
            && !source_snapshot_file_unchanged(file);
        #[cfg(coverage)]
        let source_snapshot_stale = false;
        let blocked_reason = if source_snapshot_stale {
            Some("source-snapshot-stale".into())
        } else {
            source_scan_blocker
                .clone()
                .or_else(|| {
                    planner_blocked_reason(
                        &file.path,
                        kind,
                        &lineage_metadata,
                        &dst,
                        cloud_root.provider,
                        file.bytes,
                    )
                })
                .or_else(|| provider_destination_path_blocked_reason(cloud_root, &dst))
        };
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
        let extension = file
            .path
            .extension()
            .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        if matches!(extension.as_str(), "xls" | "xlsx" | "xlsm" | "xlsb" | "ods") {
            match lineage_metadata.dataset_profile.as_ref() {
                None => review_reasons.push("spreadsheet-schema-profile-missing".into()),
                Some(profile) => {
                    if !profile.profile_complete {
                        review_reasons.push("spreadsheet-schema-profile-incomplete".into());
                    }
                    if profile.columns.iter().any(|column| column.sensitive_name) {
                        review_reasons.push("spreadsheet-sensitive-column-name-detected".into());
                    }
                    if !profile.quality_warnings.is_empty() {
                        review_reasons.push("spreadsheet-quality-warning-present".into());
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
        if let (Some(embedded_ms), Some((publication_year, publication_month))) =
            (embedded_production_time_ms, filename_publication_month)
        {
            let (embedded_year, embedded_month, _) = date_parts(embedded_ms);
            if (embedded_year, embedded_month) != (publication_year, publication_month) {
                review_reasons.push("embedded-date-differs-from-filename-publication-month".into());
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
    let exact_duplicates = {
        candidates.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.src.cmp(&b.src)));
        candidates.truncate(options.limit);
        mark_exact_duplicate_candidates(&mut candidates, None)
    };
    #[cfg(coverage)]
    let exact_duplicates = {
        candidates.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.src.cmp(&b.src)));
        candidates.truncate(options.limit);
        ExactDuplicateSummary::default()
    };
    let candidate_bytes = candidates.iter().map(|c| c.bytes).sum();
    let potentially_reclaimable_bytes = candidates
        .iter()
        .filter(|c| c.blocked_reason.is_none())
        .map(|c| c.bytes)
        .sum();
    let local_volume = crate::volume_pressure::snapshot_volume(source_root, now_ms).ok();
    let mut notices = vec![
        "dry-run-only".into(),
        "cloud-quota-unverified".into(),
        "provider-client-runtime-unverified".into(),
        "cloud-sync-unverified".into(),
        "full-transfer-content-hash-pending".into(),
    ];
    if local_volume.as_ref().is_some_and(|volume| {
        candidates.iter().any(|candidate| {
            !crate::volume_pressure::has_copy_headroom(volume.available_bytes, candidate.bytes)
        })
    }) {
        notices.push("local-volume-headroom-insufficient".into());
    }
    if !snapshot.source_scan_complete {
        notices.push("source-scan-incomplete".into());
        notices.push(format!(
            "source-scan-visited-entries:{}",
            snapshot.source_scan_visited_entries
        ));
        notices.extend(
            snapshot
                .source_scan_stop_reasons
                .iter()
                .map(|reason| format!("source-scan-stopped:{reason}")),
        );
    }
    CloudPlanReport {
        cloud_root: cloud_root.clone(),
        generated_at_ms: now_ms,
        source_selection_policy: Some(options),
        candidates,
        candidate_bytes,
        potentially_reclaimable_bytes,
        exact_duplicates,
        capacity: None,
        local_volume,
        pre_copy_evidence: None,
        notices,
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

    #[cfg(not(coverage))]
    #[test]
    fn folded_received_header_at_end_of_block_is_safe() {
        let temp = tempfile::tempdir().unwrap();
        let message_path = temp.path().join("folded-received.eml");
        std::fs::write(
            &message_path,
            concat!(
                "Date: Mon, 17 Aug 2026 12:00:00 +0000\r\n",
                "Subject: Folded Received regression\r\n",
                "Received: from relay.example\r\n",
                "\tby mx.example with ESMTP\r\n",
                "\r\n",
                "body is deliberately outside the bounded metadata parser\r\n",
            ),
        )
        .unwrap();

        let metadata = probe_content_metadata_with_general(&message_path, None);
        assert_eq!(
            metadata.title.as_deref(),
            Some("Folded Received regression")
        );
        assert!(metadata.evidence.iter().any(|evidence| {
            evidence.field == "email-header-bytes-inspected"
                && evidence.source == "local:metadata-probe:bounded-rfc5322-header"
        }));
        assert!(metadata.evidence.iter().any(|evidence| {
            evidence.field == "email-body-inspected"
                && evidence.value == "false"
                && evidence.source == "local:metadata-probe:bounded-rfc5322-header"
        }));
    }

    #[cfg(not(coverage))]
    #[test]
    fn exiftool_batch_documents_bind_each_source_file_and_reject_duplicates() {
        let parsed = exiftool_document_metadata(
            br#"[
                {"SourceFile":"/tmp/a.jpg","CreateDate":"2026:07:01 02:03:04"},
                {"SourceFile":"/tmp/b.jpg","Title":"Field note"}
            ]"#,
        )
        .unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(
            parsed[Path::new("/tmp/a.jpg")]
                .production_time_source
                .as_deref(),
            Some("embedded:exiftool:CreateDate")
        );
        assert_eq!(
            parsed[Path::new("/tmp/b.jpg")].title.as_deref(),
            Some("Field note")
        );

        assert_eq!(
            exiftool_document_metadata(
                br#"[
                    {"SourceFile":"/tmp/a.jpg"},
                    {"SourceFile":"/tmp/a.jpg"}
                ]"#,
            ),
            Err(MetadataProbeFailure::InvalidOutput)
        );
        let failed = exiftool_document_metadata(
            br#"[{"SourceFile":"/tmp/error.jpg","Error":"unsupported"}]"#,
        )
        .unwrap();
        assert!(failed[Path::new("/tmp/error.jpg")]
            .evidence
            .iter()
            .any(|evidence| evidence.value == "exiftool:invalid-output"));
    }

    #[cfg(not(coverage))]
    #[test]
    fn exiftool_batch_failure_is_retained_as_metadata_evidence() {
        let metadata = exiftool_batch_failure_metadata(MetadataProbeFailure::Timeout);
        assert!(metadata.production_time_ms.is_none());
        assert!(metadata.evidence.iter().any(|evidence| {
            evidence.field == "metadata-probe-warning"
                && evidence.value == "exiftool-batch:timeout"
                && evidence.confidence == "high"
        }));
        assert_eq!(MetadataProbeFailure::FileLimit.code(), "file-limit-exceeded");
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
        assert_eq!(report.issues[0].provider, Some(CloudProvider::GoogleDrive));
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

    #[cfg(not(coverage))]
    #[test]
    fn bounded_source_scan_blocks_partial_plans() {
        let tmp = tempfile::tempdir().unwrap();
        let source_root = tmp.path().join("source");
        writable_dir(&source_root);
        for name in ["one.pdf", "two.pdf", "three.pdf"] {
            std::fs::write(source_root.join(name), b"pdf").unwrap();
        }
        let collection = collect_archive_files_bounded(
            &source_root,
            &[],
            2,
            Duration::from_secs(30),
        );
        assert!(!collection.complete);
        assert!(collection
            .stop_reasons
            .contains(&"source-scan-entry-limit".to_string()));
        assert!(!collection.files.is_empty());

        let snapshot = prepare_cloud_archive_source_from_collection(
            &collection,
            &source_root,
            system_now_ms(),
            CloudPlanOptions {
                min_size_bytes: 1,
                min_age_days: 0,
                limit: 10,
            },
        );
        let destination = source_root.join("cloud");
        writable_dir(&destination);
        let report = plan_cloud_archive_from_snapshot(
            &snapshot,
            &root(CloudProvider::Icloud, &destination),
        );
        assert!(report.notices.contains(&"source-scan-incomplete".to_string()));
        assert!(report
            .candidates
            .iter()
            .all(|candidate| candidate.blocked_reason.as_deref() == Some("source-scan-incomplete")));
        assert_eq!(report.potentially_reclaimable_bytes, 0);
        assert_eq!(
            report
                .local_volume
                .as_ref()
                .map(|snapshot| snapshot.schema_version),
            Some(crate::volume_pressure::LOCAL_VOLUME_SNAPSHOT_SCHEMA_VERSION)
        );
    }

    #[cfg(not(coverage))]
    #[test]
    fn low_local_headroom_blocks_plan_before_copy_review() {
        let tmp = tempfile::tempdir().unwrap();
        let source_root = tmp.path().join("source");
        let cloud_root = tmp.path().join("cloud");
        writable_dir(&source_root);
        writable_dir(&cloud_root);
        let report = plan_cloud_archive(
            &[FileFact {
                path: source_root.join("large.zip"),
                bytes: u64::MAX,
                created_ms: 1,
                modified_ms: 1,
                content_metadata: ContentMetadata::default(),
            }],
            &source_root,
            &root(CloudProvider::GoogleDrive, &cloud_root),
            system_now_ms(),
            CloudPlanOptions {
                min_size_bytes: 1,
                min_age_days: 0,
                limit: 10,
            },
        );

        assert_eq!(report.candidates[0].blocked_reason, None);
        assert!(report
            .notices
            .contains(&"local-volume-headroom-insufficient".to_string()));
        assert_eq!(report.potentially_reclaimable_bytes, u64::MAX);
    }

    #[cfg(not(coverage))]
    #[test]
    fn bounded_source_scan_rejects_managed_file_provider_root() {
        let tmp = tempfile::tempdir().unwrap();
        let source_root = tmp.path().join("Library/Mobile Documents");
        std::fs::create_dir_all(&source_root).unwrap();
        std::fs::write(source_root.join("report.pdf"), b"pdf").unwrap();

        let collection = collect_archive_files_bounded(
            &source_root,
            &[],
            100,
            Duration::from_secs(30),
        );

        assert!(!collection.complete);
        assert!(collection.files.is_empty());
        assert_eq!(
            collection.stop_reasons,
            vec!["source-scan-managed-file-provider-root".to_string()]
        );
    }

    #[cfg(all(target_os = "macos", not(coverage)))]
    #[test]
    fn dataless_files_are_not_misreported_as_metadata_probe_timeouts() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let cloud = tmp.path().join("cloud");
        writable_dir(&source);
        writable_dir(&cloud);
        let path = source.join("mail-backup.zip");
        std::fs::write(&path, b"placeholder").unwrap();
        let mark = std::process::Command::new("chflags")
            .args(["dataless", path.to_str().unwrap()])
            .status()
            .unwrap();
        if !mark.success() || !source_content_is_dataless(&path) {
            let _ = std::process::Command::new("chflags")
                .args(["nodataless", path.to_str().unwrap()])
                .status();
            return;
        }

        let file_metadata = std::fs::metadata(&path).unwrap();
        let snapshot = prepare_cloud_archive_source(
            &[FileFact {
                path: path.clone(),
                bytes: file_metadata.len(),
                created_ms: millis(file_metadata.created()),
                modified_ms: millis(file_metadata.modified()),
                content_metadata: ContentMetadata::default(),
            }],
            &source,
            system_now_ms(),
            CloudPlanOptions {
                min_size_bytes: 1,
                min_age_days: 0,
                limit: 10,
            },
        );
        let report = plan_cloud_archive_from_snapshot(
            &snapshot,
            &root(CloudProvider::GoogleDrive, &cloud),
        );
        assert!(report.candidates[0]
            .metadata_evidence
            .iter()
            .all(|evidence| evidence.value != "planner:timeout"));
        assert_eq!(
            report.candidates[0].blocked_reason.as_deref(),
            Some("source-content-not-local")
        );

        let _ = std::process::Command::new("chflags")
            .args(["nodataless", path.to_str().unwrap()])
            .status();
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
    fn embedded_timestamp_parser_preserves_explicit_offset_and_fraction() {
        let day = date_epoch_ms(2026, 4, 17).unwrap();
        let utc_063547 = day + (6 * 3_600 + 35 * 60 + 47) * 1_000;
        assert_eq!(
            date_from_text("2026:04:17 15:35:47+09:00"),
            Some(utc_063547)
        );
        assert_eq!(
            date_from_text("2026-04-17T06:35:47.123Z"),
            Some(utc_063547 + 123)
        );
        assert_eq!(
            date_from_text("2026-04-17T01:02:03-05:30"),
            Some(day + (6 * 3_600 + 32 * 60 + 3) * 1_000)
        );
        assert_eq!(
            date_from_text("2026:04:17 15:35:47"),
            Some(day),
            "offset-free metadata remains date-only instead of assuming a timezone"
        );
        assert_eq!(timestamp_value(utc_063547), "2026-04-17T06:35:47.000Z");
        assert_eq!(timestamp_from_text("2026-04-17T24:00:00Z"), None);
        assert_eq!(timestamp_from_text("2026-04-17T00:00:00+14:01"), None);
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

    #[cfg(not(coverage))]
    #[test]
    fn incomplete_download_signature_scan_is_streaming_bounded_and_non_overridable() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("unknown.crdownload");
        let crossing_offset = INCOMPLETE_DOWNLOAD_SCAN_CHUNK_BYTES - 2;
        let mut payload = vec![0u8; INCOMPLETE_DOWNLOAD_SCAN_CHUNK_BYTES + 8];
        payload[crossing_offset..crossing_offset + 4].copy_from_slice(b"PK\x05\x06");
        for _ in 0..70 {
            payload.extend_from_slice(b"PK\x05\x06x");
        }
        std::fs::write(&path, &payload).unwrap();

        let scan = scan_incomplete_download_signatures(&path).unwrap();
        assert_eq!(scan.file_bytes, payload.len() as u64);
        assert_eq!(scan.zip_eocd_count, 71);
        assert_eq!(scan.zip_eocd_offsets.len(), 64);
        assert_eq!(scan.zip_eocd_offsets[0], crossing_offset as u64);

        let metadata = incomplete_download_metadata(&path);
        assert!(metadata.evidence.iter().any(|evidence| {
            evidence.field == "incomplete-download-embedded-zip-eocd-count"
                && evidence.value == "71"
        }));
        assert!(embedded_metadata_review_reasons(&path, &metadata, 0, 0)
            .contains(&"incomplete-download-contains-zip-fragment".to_string()));
        assert!(metadata.evidence.iter().any(|evidence| {
            evidence.field == "incomplete-download-structural-zip-candidate-count"
                && evidence.value == "0"
        }));
        assert_eq!(
            planner_blocked_reason(
                &path,
                ArchiveKind::IncompleteDownload,
                &metadata,
                Path::new("/definitely/missing/disksage-destination"),
                CloudProvider::Icloud,
                0,
            )
            .as_deref(),
            Some("incomplete-download")
        );
    }

    #[cfg(not(coverage))]
    #[test]
    fn incomplete_download_structural_zip_candidate_ignores_prefix_and_trailing_bytes() {
        use std::io::Cursor;
        use zip::write::SimpleFileOptions;
        use zip::{CompressionMethod, ZipWriter};

        let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file(
                "evidence/manifest.csv",
                SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(b"sha256,path\n").unwrap();
        let archive = writer.finish().unwrap().into_inner();
        let prefix = b"incomplete-prefix";
        let suffix = b"incomplete-trailing-data";
        let mut payload = prefix.to_vec();
        payload.extend_from_slice(&archive);
        payload.extend_from_slice(suffix);

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("unknown.crdownload");
        std::fs::write(&path, &payload).unwrap();
        let metadata = incomplete_download_metadata(&path);
        let expected = format!(
            "start={};end={};entries=1;",
            prefix.len(),
            prefix.len() + archive.len()
        );
        assert!(metadata.evidence.iter().any(|evidence| {
            evidence.field == "incomplete-download-structural-zip-candidate-count"
                && evidence.value == "1"
        }));
        assert!(metadata.evidence.iter().any(|evidence| {
            evidence.field == "incomplete-download-structural-zip-candidate"
                && evidence.value.starts_with(&expected)
        }));
        let reasons = embedded_metadata_review_reasons(&path, &metadata, 0, 0);
        assert!(reasons.contains(&"incomplete-download-contains-zip-fragment".to_string()));
        assert!(reasons.contains(&"incomplete-download-has-structural-zip-candidate".to_string()));
    }

    #[cfg(not(coverage))]
    #[test]
    fn email_metadata_reads_header_lineage_without_opening_the_body() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("message.eml");
        std::fs::write(
            &path,
            concat!(
                "Date: Mon, 3 Aug 2026 12:34:56 +0900\r\n",
                "From: Example Author <author@example.com>\r\n",
                "To: recipient@example.com\r\n",
                "Subject: Re: Metadata lineage\r\n",
                "Message-ID: <message-1@example.com>\r\n",
                "In-Reply-To: <message-0@example.com>\r\n",
                "References: <message-0@example.com>\r\n",
                "\r\n",
                "BODY_MARKER_MUST_NOT_BE_METADATA\r\n",
            ),
        )
        .unwrap();

        let metadata = email_metadata(&path);
        assert_eq!(
            metadata.production_time_source.as_deref(),
            Some("embedded:rfc5322:date")
        );
        assert_eq!(metadata.production_time_confidence.as_deref(), Some("high"));
        assert_eq!(
            date_parts(metadata.production_time_ms.unwrap()),
            (2026, 8, 3)
        );
        assert_eq!(metadata.title.as_deref(), Some("Re: Metadata lineage"));
        assert_eq!(metadata.authors, ["author@example.com"]);
        assert!(metadata
            .context
            .iter()
            .any(|value| value == "email-thread=Metadata lineage"));
        assert!(metadata.evidence.iter().any(|evidence| {
            evidence.field == "email-message-id" && evidence.value == "message-1@example.com"
        }));
        assert!(metadata.evidence.iter().any(|evidence| {
            evidence.field == "email-body-inspected" && evidence.value == "false"
        }));
        assert!(!metadata
            .evidence
            .iter()
            .any(|evidence| evidence.value.contains("BODY_MARKER_MUST_NOT_BE_METADATA")));
        let reasons = review_reasons(&path, ArchiveKind::Document);
        assert!(reasons.contains(&"email-content-needs-review".to_string()));
        assert!(
            destination_scope_review_reasons(CloudAccountScope::Personal, &reasons)
                .contains(&"personal-cloud-sensitive-context-needs-explicit-approval".to_string())
        );
    }

    #[cfg(not(coverage))]
    #[test]
    fn email_metadata_fails_closed_when_bounded_header_has_no_terminator() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("oversized.eml");
        std::fs::write(&path, vec![b'x'; MAX_EMAIL_HEADER_BYTES + 1]).unwrap();
        let metadata = email_metadata(&path);
        assert!(metadata.evidence.iter().any(|evidence| {
            evidence.field == "metadata-probe-warning"
                && evidence.value == "email-header:bounded-header-terminator-not-found"
        }));
        assert!(metadata.production_time_ms.is_none());
    }

    #[cfg(not(coverage))]
    #[test]
    fn audacity_project_metadata_records_schema_context_without_fabricating_a_date() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("recording.aup3");
        let mut bytes = b"SQLite format 3\0".to_vec();
        bytes.extend_from_slice(
            b" CREATE TABLE project(id INTEGER PRIMARY KEY, dict BLOB, doc BLOB); \
               CREATE TABLE sampleblocks(blockid INTEGER PRIMARY KEY, samples BLOB); ",
        );
        std::fs::write(&path, bytes).unwrap();

        let metadata = audacity_project_metadata(&path);
        assert!(metadata.production_time_ms.is_none());
        assert!(metadata.evidence.iter().any(|evidence| {
            evidence.field == "container-format" && evidence.value == "audacity-aup3-sqlite3"
        }));
        assert!(metadata.evidence.iter().any(|evidence| {
            evidence.field == "audacity-schema-table" && evidence.value == "project"
        }));
        assert!(metadata
            .context
            .contains(&"creating-application=Audacity".to_string()));
        let reasons = review_reasons(&path, ArchiveKind::Creative);
        assert!(reasons.contains(&"audacity-project-recording-content-needs-review".to_string()));
        assert!(!should_probe_general_metadata(&path));
        assert!(!should_probe_general_metadata(Path::new("message.eml")));
    }

    #[cfg(not(coverage))]
    #[test]
    fn zip_archive_metadata_reads_lineage_and_content_classes_without_extracting() {
        use zip::write::SimpleFileOptions;
        use zip::{CompressionMethod, DateTime, ZipWriter};

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("bundle.zip");
        let file = std::fs::File::create(&path).unwrap();
        let mut writer = ZipWriter::new(file);
        for (name, contents, timestamp) in [
            (
                "dataset/customers.csv",
                b"email,amount\n".as_slice(),
                DateTime::from_date_and_time(2026, 6, 4, 10, 30, 0).unwrap(),
            ),
            (
                "audio/meeting.m4a",
                b"audio".as_slice(),
                DateTime::from_date_and_time(2026, 6, 5, 11, 45, 0).unwrap(),
            ),
            (
                "config/.env",
                b"SECRET=redacted\n".as_slice(),
                DateTime::from_date_and_time(2026, 6, 3, 9, 0, 0).unwrap(),
            ),
            (
                "src/main.rs",
                b"fn main() {}\n".as_slice(),
                DateTime::from_date_and_time(2026, 6, 4, 12, 0, 0).unwrap(),
            ),
        ] {
            let options = SimpleFileOptions::default()
                .compression_method(CompressionMethod::Stored)
                .last_modified_time(timestamp);
            writer.start_file(name, options).unwrap();
            writer.write_all(contents).unwrap();
        }
        writer.finish().unwrap();

        let metadata = zip_archive_metadata(&path);
        assert_eq!(
            metadata.production_time_source.as_deref(),
            Some("embedded:zip-central-directory:latest-entry-modified")
        );
        assert_eq!(
            metadata.production_time_confidence.as_deref(),
            Some("medium")
        );
        assert_eq!(
            date_parts(metadata.production_time_ms.unwrap()),
            (2026, 6, 5)
        );
        for expected in [
            "structured-data",
            "recording-media",
            "secret-like-path",
            "source-code",
        ] {
            assert!(metadata.evidence.iter().any(|evidence| {
                evidence.field == "archive-content-class" && evidence.value == expected
            }));
        }
        for expected in [
            "archive-contains-structured-data",
            "archive-contains-recording-media",
            "archive-contains-secret-like-path",
        ] {
            assert!(embedded_metadata_review_reasons(
                &path,
                &metadata,
                metadata.production_time_ms.unwrap(),
                date_epoch_ms(2026, 6, 6).unwrap(),
            )
            .contains(&expected.to_string()));
        }
        assert!(!tmp.path().join("dataset").exists());
        assert!(!tmp.path().join("audio").exists());
    }

    #[cfg(not(coverage))]
    #[test]
    fn zip_archive_metadata_prefers_bounded_embedded_email_dates() {
        use zip::write::SimpleFileOptions;
        use zip::{CompressionMethod, DateTime, ZipWriter};

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("mail-backup.zip");
        let file = std::fs::File::create(&path).unwrap();
        let mut writer = ZipWriter::new(file);
        for (name, date, timestamp) in [
            (
                "mail/00000.eml",
                "Date: Mon, 3 Aug 2026 12:34:56 +0900",
                DateTime::from_date_and_time(2026, 6, 1, 10, 0, 0).unwrap(),
            ),
            (
                "mail/00001.eml",
                "Date: Mon, 10 Aug 2026 12:34:56 +0900",
                DateTime::from_date_and_time(2026, 6, 1, 10, 0, 0).unwrap(),
            ),
        ] {
            writer
                .start_file(
                    name,
                    SimpleFileOptions::default()
                        .compression_method(CompressionMethod::Deflated)
                        .last_modified_time(timestamp),
                )
                .unwrap();
            writer
                .write_all(format!("{date}\r\nFrom: sender@example.com\r\n\r\nbody\n").as_bytes())
                .unwrap();
        }
        writer.finish().unwrap();

        let metadata = zip_archive_metadata(&path);
        assert_eq!(
            metadata.production_time_source.as_deref(),
            Some("embedded:zip-entry:rfc5322:latest-date")
        );
        assert_eq!(metadata.production_time_confidence.as_deref(), Some("high"));
        assert_eq!(date_parts(metadata.production_time_ms.unwrap()), (2026, 8, 10));
        assert!(metadata.context.iter().any(|value| value == "archive-content-class=email"));
        assert!(metadata.evidence.iter().any(|evidence| {
            evidence.field == "archive-email-entry-count" && evidence.value == "2"
        }));
        assert!(metadata.evidence.iter().any(|evidence| {
            evidence.field == "archive-email-header-scanned-count" && evidence.value == "2"
        }));
        assert!(!metadata
            .evidence
            .iter()
            .any(|evidence| evidence.field == "metadata-probe-warning"));
        assert!(!tmp.path().join("mail").exists());
    }

    #[cfg(not(coverage))]
    #[test]
    fn zip_archive_metadata_rejects_unreadable_index_and_default_timestamp() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("broken.zip");
        std::fs::write(&path, b"not-a-zip").unwrap();
        let metadata = zip_archive_metadata(&path);
        assert!(metadata.evidence.iter().any(|evidence| {
            evidence.field == "metadata-probe-warning"
                && evidence.source == "local:metadata-probe:rust-zip"
        }));
        assert_eq!(
            planner_blocked_reason(
                &path,
                ArchiveKind::Archive,
                &metadata,
                Path::new("/definitely/missing/disksage-destination"),
                CloudProvider::Icloud,
                0,
            )
            .as_deref(),
            Some("archive-index-unreadable")
        );
        assert_eq!(zip_datetime_epoch_ms(zip::DateTime::default()), None);
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
                CloudProvider::Icloud,
                0,
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
                CloudProvider::Icloud,
                0,
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
                CloudProvider::Icloud,
                0,
            )
            .as_deref(),
            Some("archive-index-unreadable")
        );
    }

    #[test]
    fn photos_library_members_are_non_overridable_planner_blocks() {
        let destination = Path::new("/definitely/missing/disksage-destination");
        assert_eq!(
            planner_blocked_reason(
                Path::new("Pictures/Photos Library.photoslibrary/database/Photos.sqlite"),
                ArchiveKind::Dataset,
                &ContentMetadata::default(),
                destination,
                CloudProvider::Icloud,
                0,
            )
            .as_deref(),
            Some("system-managed-photos-library-data")
        );
        assert_eq!(
            planner_blocked_reason(
                Path::new("Pictures/export/Photos.sqlite"),
                ArchiveKind::Dataset,
                &ContentMetadata::default(),
                destination,
                CloudProvider::Icloud,
                0,
            ),
            None
        );
    }

    #[test]
    fn file_provider_private_storage_is_non_overridable_planner_block() {
        let destination = Path::new("/definitely/missing/disksage-destination");
        assert_eq!(
            planner_blocked_reason(
                &Path::new(
                    "/Users/test/Library/Group Containers/group.com.apple.iCloudDrive/"
                )
                .join("File Provider Storage/DownloadStage/content.wav"),
                ArchiveKind::Media,
                &ContentMetadata::default(),
                destination,
                CloudProvider::Icloud,
                0,
            )
            .as_deref(),
            Some("system-managed-file-provider-storage")
        );
        assert_eq!(
            planner_blocked_reason(
                Path::new("/Users/test/iCloud Drive/DownloadStage/content.wav"),
                ArchiveKind::Media,
                &ContentMetadata::default(),
                destination,
                CloudProvider::Icloud,
                0,
            ),
            None
        );
    }

    #[test]
    fn onedrive_destination_path_limits_fail_closed_without_affecting_other_providers() {
        let cloud_root = root(CloudProvider::Onedrive, Path::new("/cloud"));
        let short = Path::new("/cloud/DiskSage Archive/2026/documents/report.pdf");
        assert_eq!(
            provider_destination_path_blocked_reason(&cloud_root, short),
            None
        );

        let long_component = PathBuf::from("/cloud")
            .join(ARCHIVE_DIR)
            .join("2026")
            .join("documents")
            .join("x".repeat(ONEDRIVE_MAX_PATH_COMPONENT_BYTES + 1));
        assert_eq!(
            provider_destination_path_blocked_reason(&cloud_root, &long_component).as_deref(),
            Some("onedrive-path-component-too-long")
        );

        let long_relative = PathBuf::from("/cloud")
            .join(ARCHIVE_DIR)
            .join("2026")
            .join("documents")
            .join("a".repeat(100))
            .join("b".repeat(100))
            .join("c".repeat(100))
            .join("d".repeat(100));
        assert_eq!(
            provider_destination_path_blocked_reason(&cloud_root, &long_relative).as_deref(),
            Some("onedrive-path-too-long")
        );

        let google_root = root(CloudProvider::GoogleDrive, Path::new("/cloud"));
        assert_eq!(
            provider_destination_path_blocked_reason(&google_root, &long_relative),
            None
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
    fn filename_publication_month_parser_preserves_issue_context_without_fabricating_a_day() {
        assert_eq!(
            filename_publication_month(Path::new("(견본) 플라스틱스_'26년 1월호.pdf")),
            Some((2026, 1))
        );
        assert_eq!(
            filename_publication_month(Path::new("정기간행물 2026년 12 월 호 최종.pdf")),
            Some((2026, 12))
        );
        assert_eq!(
            filename_publication_month(Path::new("archive_2026년 1월.pdf")),
            None
        );
        assert_eq!(
            filename_publication_month(Path::new("archive_2026년 13월호.pdf")),
            None
        );
        assert_eq!(
            filename_publication_month(Path::new("reportA2026년 1월호.pdf")),
            None
        );
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
        assert_eq!(
            pdf_date("Fri Apr 17 15:35:47 2026 KST"),
            timestamp_from_text("2026-04-17T15:35:47+09:00")
        );
        assert_eq!(
            pdf_date("Fri Apr 17 06:35:47 2026 UTC"),
            timestamp_from_text("2026-04-17T06:35:47+00:00")
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
    fn planner_keeps_filename_publication_month_as_secondary_lineage_context() {
        let source = PathBuf::from("/source");
        let cloud = root(CloudProvider::GoogleDrive, Path::new("/cloud"));
        let embedded_ms = date_epoch_ms(2026, 4, 17).unwrap();
        let modified_ms = date_epoch_ms(2026, 4, 30).unwrap();
        let report = plan_cloud_archive(
            &[FileFact {
                path: source.join("(견본) 플라스틱스_'26년 1월호.pdf"),
                bytes: 103_544_637,
                created_ms: modified_ms,
                modified_ms,
                content_metadata: ContentMetadata {
                    production_time_ms: Some(embedded_ms),
                    production_time_source: Some("embedded:exiftool:CreateDate".into()),
                    production_time_confidence: Some("high".into()),
                    evidence: vec![MetadataEvidence {
                        field: "production-date".into(),
                        value: "2026-04-17".into(),
                        source: "embedded:exiftool:CreateDate".into(),
                        confidence: "high".into(),
                    }],
                    ..ContentMetadata::default()
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
            "embedded:exiftool:CreateDate"
        );
        assert!(candidate
            .dst
            .contains("DiskSage Archive/2026/04/documents/(견본) 플라스틱스_'26년 1월호.pdf"));
        assert!(candidate
            .content_context
            .contains(&"filename-publication-month=2026-01".to_string()));
        assert!(candidate.metadata_evidence.iter().any(|evidence| {
            evidence.field == "filename-publication-month"
                && evidence.value == "2026-01"
                && evidence.source == "filename:publication-month"
                && evidence.confidence == "low"
        }));
        assert!(candidate
            .review_reasons
            .contains(&"embedded-date-differs-from-filename-publication-month".to_string()));

        let mut evidence_removed = candidate.clone();
        evidence_removed
            .metadata_evidence
            .retain(|evidence| evidence.field != "filename-publication-month");
        assert_ne!(
            candidate.review_fingerprint,
            candidate_review_fingerprint(&evidence_removed)
        );
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
        assert!(personnel.contains(&"spreadsheet-content-needs-review".to_string()));
        let neutral_spreadsheet =
            review_reasons(Path::new("quarterly-report.xlsx"), ArchiveKind::Document);
        assert!(neutral_spreadsheet.contains(&"spreadsheet-content-needs-review".to_string()));
        for confidential in [
            "(주)엠파시 기업 분석 보고서(국문 상세)_20260510.pdf",
            "target-company due diligence.pdf",
        ] {
            assert!(
                review_reasons(Path::new(confidential), ArchiveKind::Document)
                    .contains(&"filename-context-may-be-confidential".to_string())
            );
        }
        assert!(
            review_reasons(Path::new("public-annual-report.pdf"), ArchiveKind::Document).is_empty()
        );
    }

    #[test]
    fn destination_scope_review_is_transparent_and_fail_closed() {
        let sensitive = vec!["recording-may-contain-sensitive-speech".into()];
        assert_eq!(
            destination_scope_review_reasons(CloudAccountScope::Personal, &sensitive),
            ["personal-cloud-sensitive-context-needs-explicit-approval"]
        );
        let spreadsheet = vec!["spreadsheet-content-needs-review".into()];
        assert_eq!(
            destination_scope_review_reasons(CloudAccountScope::Personal, &spreadsheet),
            ["personal-cloud-sensitive-context-needs-explicit-approval"]
        );
        assert_eq!(
            destination_scope_review_reasons(CloudAccountScope::Organization, &sensitive),
            ["organization-cloud-sensitive-context-needs-explicit-tenant-approval"]
        );
        assert!(destination_scope_review_reasons(CloudAccountScope::Organization, &[]).is_empty());
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
        let business_analysis = ContentMetadata {
            title: Some("Target company due diligence".into()),
            context: vec!["subject=기업 분석 보고서".into()],
            ..ContentMetadata::default()
        };
        assert!(embedded_metadata_review_reasons(
            Path::new("neutral-name.pdf"),
            &business_analysis,
            date_epoch_ms(2026, 5, 10).unwrap(),
            date_epoch_ms(2026, 5, 11).unwrap(),
        )
        .contains(&"embedded-metadata-context-may-be-confidential".to_string()));

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

        assert!(!should_probe_general_metadata(Path::new(
            "zotero.sqlite.1.bak",
        )));
        assert!(!should_probe_general_metadata(Path::new("zotero.db")));
        assert!(!should_probe_general_metadata(Path::new("zotero.db3")));
        assert!(!should_probe_general_metadata(Path::new("zotero.sqlite")));
        assert!(!should_probe_general_metadata(Path::new("zotero.sqlite3")));
        assert!(should_probe_general_metadata(Path::new("video.mov")));
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
            ("report.pdf", &b"same-content"[..]),
            ("report (1).pdf", &b"same-content"[..]),
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
        let files: Vec<_> = ["report.pdf", "report (1).pdf", "c.pdf"]
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
        for name in ["report.pdf", "report (1).pdf"] {
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
        let cluster = &report.exact_duplicates.clusters[0];
        let canonical = report
            .candidates
            .iter()
            .find(|candidate| candidate.relative_path == "report.pdf")
            .unwrap();
        let marked_copy = report
            .candidates
            .iter()
            .find(|candidate| candidate.relative_path == "report (1).pdf")
            .unwrap();
        assert_eq!(
            cluster.recommended_canonical_metadata_fingerprint,
            canonical.metadata_fingerprint
        );
        assert_eq!(cluster.recommendation_confidence, "medium");
        assert!(cluster
            .recommendation_reason_codes
            .contains(&"non-copy-marked-filename-preferred".to_string()));
        assert!(cluster.requires_human_confirmation);
        assert!(canonical.metadata_evidence.iter().any(|evidence| {
            evidence.field == "exact-duplicate-canonical-recommendation"
                && evidence.value == "preferred"
        }));
        assert!(marked_copy.metadata_evidence.iter().any(|evidence| {
            evidence.field == "exact-duplicate-canonical-recommendation"
                && evidence.value == "redundant-copy-candidate"
        }));
        let unique = report
            .candidates
            .iter()
            .find(|candidate| candidate.relative_path == "c.pdf")
            .unwrap();
        assert!(!unique
            .review_reasons
            .contains(&"exact-duplicate-content-needs-canonical-selection".to_string()));
    }

    #[cfg(not(coverage))]
    #[test]
    fn canonical_recommendation_keeps_embedded_lineage_ahead_of_copy_name_heuristics() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let cloud = tmp.path().join("cloud");
        writable_dir(&source);
        writable_dir(&cloud);
        for name in ["analysis.pdf", "analysis (1).pdf"] {
            std::fs::write(source.join(name), b"same-analysis").unwrap();
        }
        let embedded = ContentMetadata {
            production_time_ms: Some(date_epoch_ms(2026, 2, 3).unwrap()),
            production_time_source: Some("embedded:test:creation-date".into()),
            production_time_confidence: Some("high".into()),
            title: Some("Analysis".into()),
            evidence: vec![MetadataEvidence {
                field: "production-date".into(),
                value: "2026-02-03".into(),
                source: "embedded:test:creation-date".into(),
                confidence: "high".into(),
            }],
            ..ContentMetadata::default()
        };
        let filesystem_only = ContentMetadata::default();
        let files = [
            ("analysis.pdf", filesystem_only),
            ("analysis (1).pdf", embedded),
        ]
        .into_iter()
        .map(|(name, content_metadata)| {
            let path = source.join(name);
            let file_metadata = std::fs::metadata(&path).unwrap();
            FileFact {
                path,
                bytes: file_metadata.len(),
                created_ms: millis(file_metadata.created()),
                modified_ms: millis(file_metadata.modified()),
                content_metadata,
            }
        })
        .collect::<Vec<_>>();

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

        let cluster = &report.exact_duplicates.clusters[0];
        let embedded_copy = report
            .candidates
            .iter()
            .find(|candidate| candidate.relative_path == "analysis (1).pdf")
            .unwrap();
        assert_eq!(
            cluster.recommended_canonical_metadata_fingerprint,
            embedded_copy.metadata_fingerprint
        );
        assert_eq!(cluster.recommendation_confidence, "high");
        assert!(cluster
            .recommendation_reason_codes
            .contains(&"embedded-production-time-preferred".to_string()));
        assert!(embedded_copy.metadata_evidence.iter().any(|evidence| {
            evidence.field == "exact-duplicate-canonical-recommendation"
                && evidence.value == "preferred"
        }));
    }

    #[cfg(not(coverage))]
    #[test]
    fn canonical_recommendation_labels_source_lineage_separately_from_embedded_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let cloud = tmp.path().join("cloud");
        writable_dir(&source);
        writable_dir(&cloud);
        for name in ["recording.m4a", "recording (1).m4a"] {
            std::fs::write(source.join(name), b"same-recording").unwrap();
        }
        let production_time_ms = date_epoch_ms(2026, 2, 3).unwrap();
        let common = ContentMetadata {
            production_time_ms: Some(production_time_ms),
            production_time_source: Some("embedded:test:creation-date".into()),
            production_time_confidence: Some("high".into()),
            title: Some("Recording".into()),
            ..ContentMetadata::default()
        };
        let mut with_source_lineage = common.clone();
        with_source_lineage.context = vec![
            "download-origin-host=recorder.example".into(),
            "download-agent=browser".into(),
        ];
        let files = [
            ("recording.m4a", common),
            ("recording (1).m4a", with_source_lineage),
        ]
        .into_iter()
        .map(|(name, content_metadata)| {
            let path = source.join(name);
            let file_metadata = std::fs::metadata(&path).unwrap();
            FileFact {
                path,
                bytes: file_metadata.len(),
                created_ms: millis(file_metadata.created()),
                modified_ms: millis(file_metadata.modified()),
                content_metadata,
            }
        })
        .collect::<Vec<_>>();

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

        let cluster = &report.exact_duplicates.clusters[0];
        let lineage_copy = report
            .candidates
            .iter()
            .find(|candidate| candidate.relative_path == "recording (1).m4a")
            .unwrap();
        assert_eq!(
            cluster.recommended_canonical_metadata_fingerprint,
            lineage_copy.metadata_fingerprint
        );
        assert_eq!(cluster.recommendation_confidence, "high");
        assert!(cluster
            .recommendation_reason_codes
            .contains(&"richer-source-lineage-context-preferred".to_string()));
        assert!(!cluster
            .recommendation_reason_codes
            .contains(&"richer-embedded-metadata-preferred".to_string()));
        assert!(lineage_copy.metadata_evidence.iter().any(|evidence| {
            evidence.field == "exact-duplicate-canonical-recommendation"
                && evidence.value == "preferred"
        }));
    }

    #[cfg(not(coverage))]
    #[test]
    fn source_snapshot_reuses_probes_and_rehashes_final_destination_candidates() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let google = tmp.path().join("google");
        let onedrive = tmp.path().join("onedrive");
        writable_dir(&source);
        writable_dir(&google);
        writable_dir(&onedrive);
        for name in ["a.pdf", "b.pdf"] {
            std::fs::write(source.join(name), b"same-content").unwrap();
        }
        let production_time_ms = date_epoch_ms(2026, 1, 2).unwrap();
        let metadata = ContentMetadata {
            production_time_ms: Some(production_time_ms),
            production_time_source: Some("embedded:test:creation-date".into()),
            production_time_confidence: Some("high".into()),
            ..ContentMetadata::default()
        };
        let files = ["a.pdf", "b.pdf"]
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
            .collect::<Vec<_>>();
        let snapshot = prepare_cloud_archive_source(
            &files,
            &source,
            system_now_ms() + DAY_MS,
            CloudPlanOptions {
                min_size_bytes: 0,
                min_age_days: 0,
                limit: 10,
            },
        );

        assert_eq!(snapshot.candidate_count(), 2);
        let google_report =
            plan_cloud_archive_from_snapshot(&snapshot, &root(CloudProvider::GoogleDrive, &google));
        let onedrive_report =
            plan_cloud_archive_from_snapshot(&snapshot, &root(CloudProvider::Onedrive, &onedrive));
        assert_eq!(google_report.exact_duplicates.cluster_count, 1);
        assert_eq!(
            google_report.exact_duplicates,
            onedrive_report.exact_duplicates
        );

        let destination = PathBuf::from(&google_report.candidates[0].dst);
        writable_dir(destination.parent().unwrap());
        std::fs::write(&destination, b"existing-destination").unwrap();
        let refreshed =
            plan_cloud_archive_from_snapshot(&snapshot, &root(CloudProvider::GoogleDrive, &google));
        assert!(refreshed.candidates.iter().any(|candidate| {
            candidate.dst == destination.to_string_lossy()
                && candidate.blocked_reason.as_deref() == Some("destination-exists")
        }));
    }

    #[cfg(not(coverage))]
    #[test]
    fn source_snapshot_fails_closed_when_source_stat_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let cloud = tmp.path().join("cloud");
        writable_dir(&source);
        writable_dir(&cloud);
        let path = source.join("report.pdf");
        std::fs::write(&path, b"before").unwrap();
        let file_metadata = std::fs::metadata(&path).unwrap();
        let snapshot = prepare_cloud_archive_source(
            &[FileFact {
                path: path.clone(),
                bytes: file_metadata.len(),
                created_ms: millis(file_metadata.created()),
                modified_ms: millis(file_metadata.modified()),
                content_metadata: ContentMetadata {
                    production_time_ms: Some(date_epoch_ms(2026, 1, 2).unwrap()),
                    production_time_source: Some("embedded:test:creation-date".into()),
                    production_time_confidence: Some("high".into()),
                    ..ContentMetadata::default()
                },
            }],
            &source,
            system_now_ms() + DAY_MS,
            CloudPlanOptions {
                min_size_bytes: 0,
                min_age_days: 0,
                limit: 10,
            },
        );

        std::fs::write(path, b"changed-and-longer").unwrap();
        let report =
            plan_cloud_archive_from_snapshot(&snapshot, &root(CloudProvider::GoogleDrive, &cloud));
        assert_eq!(
            report.candidates[0].blocked_reason.as_deref(),
            Some("source-snapshot-stale")
        );
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
        assert_eq!(report.notices.len(), 5);
        assert!(report
            .notices
            .contains(&"provider-client-runtime-unverified".to_string()));
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
            ("x.eml", ArchiveKind::Document),
            ("x.mp4", ArchiveKind::Media),
            ("x.zip", ArchiveKind::Archive),
            ("x.csv", ArchiveKind::Dataset),
            ("x.bak", ArchiveKind::Backup),
            ("x.psd", ArchiveKind::Creative),
            ("x.aup3", ArchiveKind::Creative),
            ("x.crdownload", ArchiveKind::IncompleteDownload),
            (".env.api", ArchiveKind::SensitiveConfig),
            ("x.zip.part004", ArchiveKind::Archive),
        ] {
            assert_eq!(archive_kind(Path::new(ext)), Some(expected));
            assert!(!expected.folder().is_empty());
        }
        assert_eq!(archive_kind(Path::new("x.zip.part04")), None);
        assert_eq!(archive_kind(Path::new("README")), None);
    }

    #[cfg(not(coverage))]
    #[test]
    fn sensitive_config_files_are_visible_but_never_reclaimable() {
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let cloud = tmp.path().join("cloud");
        writable_dir(&source);
        writable_dir(&cloud);
        for name in [".env.api", "credentials.json"] {
            std::fs::write(source.join(name), b"redacted-test-fixture").unwrap();
        }

        assert_eq!(
            archive_kind(&source.join(".env.api")),
            Some(ArchiveKind::SensitiveConfig)
        );
        assert!(!should_probe_general_metadata(&source.join(".env.api")));
        let report = plan_cloud_archive(
            &collect_archive_files(&source, &[]),
            &source,
            &root(CloudProvider::Icloud, &cloud),
            system_now_ms(),
            CloudPlanOptions {
                min_size_bytes: 1,
                min_age_days: 0,
                limit: 10,
            },
        );
        assert_eq!(report.candidates.len(), 2);
        assert!(report
            .candidates
            .iter()
            .all(|candidate| candidate.blocked_reason.as_deref() == Some("sensitive-config-file")));
        assert_eq!(report.potentially_reclaimable_bytes, 0);
    }

    #[test]
    fn sensitive_config_name_detection_keeps_examples_out_and_covers_key_markers() {
        for (name, expected) in [
            (".env", true),
            (".env.collector", true),
            (".env.example", false),
            (".env.sample", false),
            (".env.template", false),
            ("credentials.json", true),
            ("private-key.pem", true),
            ("server.p12", true),
            ("signing.key", true),
            ("README", false),
        ] {
            assert_eq!(
                is_sensitive_config_path(Path::new(name)),
                expected,
                "unexpected sensitive-config classification for {name}"
            );
        }
    }

    #[test]
    fn pre_copy_evidence_cohort_is_sorted_and_fingerprinted() {
        let cohort = compare_pre_copy_evidence(vec![
            PreCopyEvidenceObservation {
                stream: "icloud-sync-health-evidence".into(),
                observed_at_ms: 1_000,
                evidence_complete: true,
                fingerprint: "c".repeat(64),
            },
            PreCopyEvidenceObservation {
                stream: "volume-pressure-evidence".into(),
                observed_at_ms: 900,
                evidence_complete: true,
                fingerprint: "a".repeat(64),
            },
            PreCopyEvidenceObservation {
                stream: "provider-client-runtime-evidence".into(),
                observed_at_ms: 950,
                evidence_complete: true,
                fingerprint: "b".repeat(64),
            },
        ]);
        assert!(cohort.complete);
        assert_eq!(cohort.observed_at_ms, 1_000);
        assert_eq!(
            cohort
                .observations
                .iter()
                .map(|observation| observation.stream.as_str())
                .collect::<Vec<_>>(),
            vec![
                "icloud-sync-health-evidence",
                "provider-client-runtime-evidence",
                "volume-pressure-evidence"
            ]
        );
        assert!(valid_evidence_fingerprint(&cohort.cohort_fingerprint));
    }

    #[test]
    fn pre_copy_evidence_cohort_blocks_incomplete_and_skewed_observations() {
        let cohort = compare_pre_copy_evidence(vec![
            PreCopyEvidenceObservation {
                stream: "volume-pressure-evidence".into(),
                observed_at_ms: 1,
                evidence_complete: true,
                fingerprint: "a".repeat(64),
            },
            PreCopyEvidenceObservation {
                stream: "icloud-sync-health-evidence".into(),
                observed_at_ms: PRE_COPY_EVIDENCE_MAX_SKEW_MS + 2,
                evidence_complete: false,
                fingerprint: "b".repeat(64),
            },
        ]);
        assert!(!cohort.complete);
        assert!(cohort
            .blockers
            .contains(&"pre-copy-evidence-observation-time-skew".into()));
        assert!(cohort
            .blockers
            .contains(&"pre-copy-evidence-stream-incomplete-icloud-sync-health-evidence".into()));
        assert!(cohort
            .blockers
            .contains(&"pre-copy-evidence-stream-missing-provider-client-runtime-evidence".into()));
    }

    #[test]
    fn pre_copy_evidence_cohort_is_required_and_integrity_bound() {
        assert_eq!(
            require_pre_copy_evidence_cohort(None).unwrap_err(),
            "pre-copy-evidence-cohort-unavailable"
        );
        let valid = compare_pre_copy_evidence(
            PRE_COPY_EVIDENCE_REQUIRED_STREAMS
                .iter()
                .enumerate()
                .map(|(index, stream)| PreCopyEvidenceObservation {
                    stream: (*stream).into(),
                    observed_at_ms: 100 + index as u64,
                    evidence_complete: true,
                    fingerprint: format!("{index:x}").repeat(64),
                })
                .collect(),
        );
        assert!(require_pre_copy_evidence_cohort(Some(&valid)).is_ok());
        let mut tampered = valid.clone();
        tampered.observed_at_ms += 1;
        assert_eq!(
            require_pre_copy_evidence_cohort(Some(&tampered)).unwrap_err(),
            "pre-copy-evidence-cohort-integrity-invalid"
        );
    }
}
