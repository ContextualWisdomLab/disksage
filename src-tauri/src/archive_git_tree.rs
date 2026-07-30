use std::cmp::Ordering;
use std::collections::{btree_map::Entry, BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha1::{Digest as Sha1Digest, Sha1};
use sha2::{Digest as Sha2Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

const REPORT_VERSION: u32 = 1;
const COMPARISON_REPORT_VERSION: u32 = 1;
const COMPARISON_SCHEMA_KIND: &str = "disksage.archive-content-inclusion";
const BATCH_REPORT_VERSION: u32 = 1;
const BATCH_SCHEMA_KIND: &str = "disksage.archive-content-inclusion-batch";
const MAX_ZIP_ENTRIES: usize = 100_000;
const MAX_PATH_BYTES: usize = 4_096;
const MAX_UNCOMPRESSED_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MAX_COMPRESSED_ARCHIVE_BYTES: u64 = 32 * 1024 * 1024 * 1024;
const MAX_CASE_COLLISION_GROUPS: usize = 1_000;
const MAX_COMPARISON_PATH_SAMPLES: usize = 1_000;
const MAX_BATCH_ARCHIVES: usize = 512;
const MAX_BATCH_PAIRS: usize = 131_072;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ArchiveGitTreeReport {
    pub version: u32,
    pub archive: String,
    pub root_prefix: String,
    pub zip_entry_count: usize,
    pub file_count: usize,
    pub directory_count: usize,
    pub uncompressed_bytes: u64,
    pub git_tree_sha1: String,
    pub expected_git_tree_sha1: Option<String>,
    pub matches_expected: Option<bool>,
    pub case_collision_groups: Vec<Vec<String>>,
}

/// Content-addressed proof that every logical file in one ZIP is present in another ZIP.
///
/// Counts cover the complete manifests. Path arrays are bounded evidence samples; when any sample
/// is truncated, `paths_truncated` is true. Equality requires the same validated logical path,
/// normalized Git mode, uncompressed byte length, and SHA-256 of the uncompressed bytes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ArchiveContentInclusionReport {
    pub version: u32,
    pub schema_kind: &'static str,
    pub subset_archive: String,
    pub superset_archive: String,
    pub root_mode: String,
    pub subset_root_prefix: String,
    pub superset_root_prefix: String,
    pub subset_file_count: usize,
    pub superset_file_count: usize,
    pub subset_uncompressed_bytes: u64,
    pub superset_uncompressed_bytes: u64,
    pub matching_file_count: usize,
    pub missing_file_count: usize,
    pub changed_file_count: usize,
    pub additional_file_count: usize,
    pub subset_content_included: bool,
    pub archives_identical: bool,
    pub missing_paths: Vec<String>,
    pub changed_paths: Vec<String>,
    pub additional_paths: Vec<String>,
    pub paths_truncated: bool,
    pub subset_manifest_sha256: String,
    pub superset_manifest_sha256: String,
    pub comparison_fingerprint_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchiveContentBatchOptions {
    pub max_archives: usize,
    pub max_pairs: usize,
}

impl Default for ArchiveContentBatchOptions {
    fn default() -> Self {
        Self {
            max_archives: 128,
            max_pairs: 8_192,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ArchiveContentBatchArchive {
    pub archive_id: String,
    pub archive: String,
    pub compressed_bytes: u64,
    pub raw_archive_sha256: String,
    pub file_count: usize,
    pub uncompressed_bytes: u64,
    pub root_prefix: String,
    pub manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ArchiveContentBatchIssue {
    pub archive_id: String,
    pub archive: String,
    pub reason: String,
}

/// Bounded, read-only comparison of ZIP manifests loaded exactly once per archive.
///
/// `reclaim_review_*` fields are review evidence only. They do not assert that an archive's
/// container metadata, acquisition context, signatures, or production lineage are redundant.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ArchiveContentInclusionBatchReport {
    pub version: u32,
    pub schema_kind: &'static str,
    pub generated_at_ms: u64,
    pub root_mode: String,
    pub archive_input_count: usize,
    pub archive_valid_count: usize,
    pub issue_count: usize,
    pub pair_comparison_count: usize,
    pub inclusion_relation_count: usize,
    pub strict_inclusion_relation_count: usize,
    pub identical_relation_count: usize,
    pub reclaim_review_candidate_count: usize,
    pub reclaim_review_compressed_bytes: u64,
    pub evidence_complete: bool,
    pub batch_fingerprint_sha256: String,
    pub archives: Vec<ArchiveContentBatchArchive>,
    pub issues: Vec<ArchiveContentBatchIssue>,
    pub relations: Vec<ArchiveContentInclusionReport>,
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct NaruonArchiveContentIdentity {
    pub archive_id: String,
    pub raw_archive_sha256: String,
    pub manifest_sha256: String,
    pub compressed_bytes: u64,
    pub file_count: usize,
    pub uncompressed_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct NaruonArchiveInclusionRelation {
    pub subset: NaruonArchiveContentIdentity,
    pub superset: NaruonArchiveContentIdentity,
    pub comparison_fingerprint_sha256: String,
    pub archives_identical: bool,
    pub additional_file_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct NaruonArchiveInclusionIssue {
    pub archive_id: String,
    pub reason: String,
}

/// Path-free archive inclusion lineage contract suitable for a Naruon receiver.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct NaruonArchiveInclusionLineageEnvelope {
    pub schema_version: u32,
    pub schema_kind: &'static str,
    pub generated_at_ms: u64,
    pub batch_fingerprint_sha256: String,
    pub lineage_fingerprint_sha256: String,
    pub root_mode: String,
    pub evidence_complete: bool,
    pub relation_count: usize,
    pub reclaim_review_candidate_count: usize,
    pub reclaim_review_compressed_bytes: u64,
    pub relations: Vec<NaruonArchiveInclusionRelation>,
    pub issues: Vec<NaruonArchiveInclusionIssue>,
    pub content_evidence: Vec<String>,
    pub local_paths_included: bool,
    pub filename_dates_used: bool,
    pub filesystem_timestamps_used: bool,
    pub deletion_authorized: bool,
    pub cloud_write_executed: bool,
}

/// Choose whether the archive's first path component is a transport wrapper or logical content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveTreeRootMode {
    /// Require one shared top-level directory and omit it from the computed tree.
    StripSharedRoot,
    /// Preserve every validated path component in the computed tree.
    KeepTopLevel,
}

impl ArchiveTreeRootMode {
    fn label(self) -> &'static str {
        match self {
            Self::StripSharedRoot => "strip-shared-root",
            Self::KeepTopLevel => "keep-top-level",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ArchiveFileEvidence {
    mode: &'static [u8],
    bytes: u64,
    sha256: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ArchiveManifest {
    report: ArchiveGitTreeReport,
    root_mode: ArchiveTreeRootMode,
    entries: BTreeMap<String, ArchiveFileEvidence>,
}

#[derive(Debug)]
struct LoadedBatchArchive {
    manifest: ArchiveManifest,
    archive: ArchiveContentBatchArchive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BlobEntry {
    mode: &'static [u8],
    oid: [u8; 20],
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TreeEntry {
    Blob(BlobEntry),
    Tree(TreeNode),
}

impl TreeEntry {
    fn is_tree(&self) -> bool {
        matches!(self, Self::Tree(_))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct TreeNode {
    entries: BTreeMap<Vec<u8>, TreeEntry>,
}

impl TreeNode {
    fn insert_blob(&mut self, components: &[Vec<u8>], blob: BlobEntry) -> Result<(), String> {
        let (name, rest) = components
            .split_first()
            .ok_or_else(|| "archive-entry-empty-relative-path".to_string())?;
        if rest.is_empty() {
            return match self.entries.entry(name.clone()) {
                Entry::Vacant(slot) => {
                    slot.insert(TreeEntry::Blob(blob));
                    Ok(())
                }
                Entry::Occupied(_) => Err("archive-entry-duplicate-or-type-conflict".into()),
            };
        }
        let child = match self.entries.entry(name.clone()) {
            Entry::Vacant(slot) => slot.insert(TreeEntry::Tree(TreeNode::default())),
            Entry::Occupied(slot) => slot.into_mut(),
        };
        match child {
            TreeEntry::Tree(tree) => tree.insert_blob(rest, blob),
            TreeEntry::Blob(_) => Err("archive-entry-file-directory-conflict".into()),
        }
    }
}

fn validate_expected_tree(value: Option<&str>) -> Result<Option<String>, String> {
    value
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            if normalized.len() != 40 || !normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                return Err("expected-git-tree-sha1-invalid".into());
            }
            Ok(normalized)
        })
        .transpose()
}

fn zip_path_components(raw_name: &[u8], directory: bool) -> Result<Vec<Vec<u8>>, String> {
    if raw_name.is_empty() || raw_name.len() > MAX_PATH_BYTES || raw_name.contains(&0) {
        return Err("archive-entry-path-invalid".into());
    }
    if raw_name[0] == b'/' || raw_name.contains(&b'\\') {
        return Err("archive-entry-path-unsafe".into());
    }
    let mut components: Vec<Vec<u8>> = raw_name
        .split(|byte| *byte == b'/')
        .map(<[u8]>::to_vec)
        .collect();
    if directory && components.last().is_some_and(Vec::is_empty) {
        components.pop();
    }
    if components.is_empty()
        || components
            .iter()
            .any(|component| component.is_empty() || component == b"." || component == b"..")
    {
        return Err("archive-entry-path-unsafe".into());
    }
    for component in &components {
        std::str::from_utf8(component).map_err(|_| "archive-entry-path-not-utf8".to_string())?;
    }
    Ok(components)
}

fn git_blob_mode(unix_mode: Option<u32>) -> Result<&'static [u8], String> {
    let mode = unix_mode.unwrap_or(0o100644);
    match mode & 0o170000 {
        0 | 0o100000 => {
            if mode & 0o111 == 0 {
                Ok(b"100644")
            } else {
                Ok(b"100755")
            }
        }
        0o120000 => Ok(b"120000"),
        _ => Err("archive-entry-unrepresentable-git-mode".into()),
    }
}

fn blob_digests(reader: &mut impl Read, size: u64) -> Result<([u8; 20], [u8; 32]), String> {
    let mut git_hasher = Sha1::new();
    git_hasher.update(format!("blob {size}\0").as_bytes());
    let mut content_hasher = Sha256::new();
    let mut observed = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|_| "archive-entry-read-failed".to_string())?;
        if read == 0 {
            break;
        }
        observed = observed
            .checked_add(read as u64)
            .ok_or_else(|| "archive-uncompressed-size-overflow".to_string())?;
        if observed > size {
            return Err("archive-entry-size-mismatch".into());
        }
        git_hasher.update(&buffer[..read]);
        content_hasher.update(&buffer[..read]);
    }
    if observed != size {
        return Err("archive-entry-size-mismatch".into());
    }
    Ok((
        git_hasher.finalize().into(),
        content_hasher.finalize().into(),
    ))
}

fn git_name_compare(left: &[u8], left_tree: bool, right: &[u8], right_tree: bool) -> Ordering {
    let shared = left.len().min(right.len());
    match left[..shared].cmp(&right[..shared]) {
        Ordering::Equal => {
            let left_next = left
                .get(shared)
                .copied()
                .unwrap_or(if left_tree { b'/' } else { 0 });
            let right_next =
                right
                    .get(shared)
                    .copied()
                    .unwrap_or(if right_tree { b'/' } else { 0 });
            left_next.cmp(&right_next)
        }
        ordering => ordering,
    }
}

fn git_tree_oid(tree: &TreeNode) -> ([u8; 20], usize) {
    let mut entries: Vec<_> = tree.entries.iter().collect();
    entries.sort_by(|(left_name, left), (right_name, right)| {
        git_name_compare(left_name, left.is_tree(), right_name, right.is_tree())
    });
    let mut content = Vec::new();
    let mut directory_count = 0usize;
    for (name, entry) in entries {
        let (mode, oid) = match entry {
            TreeEntry::Blob(blob) => (blob.mode, blob.oid),
            TreeEntry::Tree(child) => {
                let (oid, nested_count) = git_tree_oid(child);
                directory_count += nested_count + 1;
                (&b"40000"[..], oid)
            }
        };
        content.extend_from_slice(mode);
        content.push(b' ');
        content.extend_from_slice(name);
        content.push(0);
        content.extend_from_slice(&oid);
    }
    let mut hasher = Sha1::new();
    hasher.update(format!("tree {}\0", content.len()).as_bytes());
    hasher.update(&content);
    (hasher.finalize().into(), directory_count)
}

fn hex_sha1(value: &[u8; 20]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hex_sha256(value: &[u8; 32]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn raw_file_sha256(path: &Path) -> Result<[u8; 32], String> {
    let mut file = File::open(path).map_err(|_| "archive-raw-open-failed".to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| "archive-raw-read-failed".to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().into())
}

fn update_len_prefixed(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_le_bytes());
    hasher.update(value);
}

fn manifest_sha256(manifest: &ArchiveManifest) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"disksage.archive-content-manifest\0v1\0");
    update_len_prefixed(&mut hasher, manifest.root_mode.label().as_bytes());
    hasher.update((manifest.entries.len() as u64).to_le_bytes());
    for (path, evidence) in &manifest.entries {
        update_len_prefixed(&mut hasher, path.as_bytes());
        update_len_prefixed(&mut hasher, evidence.mode);
        hasher.update(evidence.bytes.to_le_bytes());
        hasher.update(evidence.sha256);
    }
    hasher.finalize().into()
}

fn comparison_fingerprint(
    root_mode: ArchiveTreeRootMode,
    subset_manifest_sha256: &[u8; 32],
    superset_manifest_sha256: &[u8; 32],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"disksage.archive-content-inclusion\0v1\0");
    update_len_prefixed(&mut hasher, root_mode.label().as_bytes());
    hasher.update(b"subset\0");
    hasher.update(subset_manifest_sha256);
    hasher.update(b"superset\0");
    hasher.update(superset_manifest_sha256);
    hasher.finalize().into()
}

fn archive_id(
    path: &str,
    raw_archive_digest: &[u8; 32],
    manifest_digest: Option<&[u8; 32]>,
    compressed_bytes: u64,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"disksage.archive-content-batch-archive\0v1\0");
    update_len_prefixed(&mut hasher, path.as_bytes());
    hasher.update(raw_archive_digest);
    hasher.update(compressed_bytes.to_le_bytes());
    match manifest_digest {
        Some(digest) => {
            hasher.update(b"manifest\0");
            hasher.update(digest);
        }
        None => hasher.update(b"manifest-unavailable\0"),
    }
    hex_sha256(&hasher.finalize().into())
}

fn unavailable_archive_id(path: &str, compressed_bytes: u64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"disksage.archive-content-batch-unavailable\0v1\0");
    update_len_prefixed(&mut hasher, path.as_bytes());
    hasher.update(compressed_bytes.to_le_bytes());
    hex_sha256(&hasher.finalize().into())
}

fn batch_fingerprint(
    root_mode: ArchiveTreeRootMode,
    archives: &[ArchiveContentBatchArchive],
    issues: &[ArchiveContentBatchIssue],
    relations: &[ArchiveContentInclusionReport],
) -> String {
    let ids: BTreeMap<&str, &str> = archives
        .iter()
        .map(|archive| (archive.archive.as_str(), archive.archive_id.as_str()))
        .collect();
    let mut hasher = Sha256::new();
    hasher.update(b"disksage.archive-content-inclusion-batch\0v1\0");
    update_len_prefixed(&mut hasher, root_mode.label().as_bytes());
    hasher.update((archives.len() as u64).to_le_bytes());
    for archive in archives {
        update_len_prefixed(&mut hasher, archive.archive_id.as_bytes());
        update_len_prefixed(&mut hasher, archive.raw_archive_sha256.as_bytes());
        update_len_prefixed(&mut hasher, archive.manifest_sha256.as_bytes());
        hasher.update(archive.compressed_bytes.to_le_bytes());
        hasher.update((archive.file_count as u64).to_le_bytes());
        hasher.update(archive.uncompressed_bytes.to_le_bytes());
    }
    hasher.update((issues.len() as u64).to_le_bytes());
    for issue in issues {
        update_len_prefixed(&mut hasher, issue.archive_id.as_bytes());
        update_len_prefixed(&mut hasher, issue.reason.as_bytes());
    }
    hasher.update((relations.len() as u64).to_le_bytes());
    for relation in relations {
        update_len_prefixed(
            &mut hasher,
            ids.get(relation.subset_archive.as_str())
                .copied()
                .unwrap_or_default()
                .as_bytes(),
        );
        update_len_prefixed(
            &mut hasher,
            ids.get(relation.superset_archive.as_str())
                .copied()
                .unwrap_or_default()
                .as_bytes(),
        );
        update_len_prefixed(
            &mut hasher,
            relation.comparison_fingerprint_sha256.as_bytes(),
        );
    }
    hex_sha256(&hasher.finalize().into())
}

fn push_bounded(paths: &mut Vec<String>, path: &str) {
    if paths.len() < MAX_COMPARISON_PATH_SAMPLES {
        paths.push(path.to_string());
    }
}

/// Compute the Git tree object represented by a wrapped source ZIP without extracting it.
///
/// Every entry must share one top-level directory, as GitHub source archives do. File bytes are
/// streamed into Git blob hashes; paths that collide only by case remain distinct in the Git tree
/// and are surfaced separately because they cannot be safely extracted on common macOS volumes.
pub fn inspect_zip_git_tree(
    archive_path: &Path,
    expected_tree: Option<&str>,
) -> Result<ArchiveGitTreeReport, String> {
    inspect_zip_git_tree_with_mode(
        archive_path,
        expected_tree,
        ArchiveTreeRootMode::StripSharedRoot,
    )
}

/// Compute a Git-compatible logical tree for a ZIP without extracting it.
///
/// The default wrapper-stripping behavior remains available through [`inspect_zip_git_tree`].
/// Use [`ArchiveTreeRootMode::KeepTopLevel`] only when top-level entries are part of the logical
/// archive content rather than a transport wrapper.
pub fn inspect_zip_git_tree_with_mode(
    archive_path: &Path,
    expected_tree: Option<&str>,
    root_mode: ArchiveTreeRootMode,
) -> Result<ArchiveGitTreeReport, String> {
    Ok(inspect_zip_manifest_with_mode(archive_path, expected_tree, root_mode)?.report)
}

fn inspect_zip_manifest_with_mode(
    archive_path: &Path,
    expected_tree: Option<&str>,
    root_mode: ArchiveTreeRootMode,
) -> Result<ArchiveManifest, String> {
    let expected_git_tree_sha1 = validate_expected_tree(expected_tree)?;
    let file = File::open(archive_path).map_err(|_| "archive-open-failed".to_string())?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|_| "archive-central-directory-invalid".to_string())?;
    if archive.is_empty() || archive.len() > MAX_ZIP_ENTRIES {
        return Err("archive-entry-count-out-of-bounds".into());
    }

    let mut root_prefix: Option<Vec<u8>> = None;
    let mut tree = TreeNode::default();
    let mut file_count = 0usize;
    let mut uncompressed_bytes = 0u64;
    let mut case_paths: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut entries = BTreeMap::new();

    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|_| "archive-entry-open-failed".to_string())?;
        if entry.encrypted() {
            return Err("archive-entry-encrypted".into());
        }
        let directory = entry.is_dir() || entry.name_raw().ends_with(b"/");
        let components = zip_path_components(entry.name_raw(), directory)?;
        let relative = match root_mode {
            ArchiveTreeRootMode::StripSharedRoot => {
                let (root, relative) = components
                    .split_first()
                    .ok_or_else(|| "archive-shared-root-missing".to_string())?;
                match root_prefix.as_ref() {
                    None => root_prefix = Some(root.clone()),
                    Some(expected) if expected == root => {}
                    Some(_) => return Err("archive-shared-root-mismatch".into()),
                }
                if relative.is_empty() {
                    if directory {
                        continue;
                    }
                    return Err("archive-entry-empty-relative-path".into());
                }
                relative
            }
            ArchiveTreeRootMode::KeepTopLevel => components.as_slice(),
        };
        if directory {
            if entry.size() != 0 {
                return Err("archive-directory-entry-has-payload".into());
            }
            continue;
        }

        uncompressed_bytes = uncompressed_bytes
            .checked_add(entry.size())
            .ok_or_else(|| "archive-uncompressed-size-overflow".to_string())?;
        if uncompressed_bytes > MAX_UNCOMPRESSED_BYTES {
            return Err("archive-uncompressed-size-out-of-bounds".into());
        }
        let relative_bytes =
            relative
                .iter()
                .enumerate()
                .fold(Vec::new(), |mut path, (position, component)| {
                    if position > 0 {
                        path.push(b'/');
                    }
                    path.extend_from_slice(component);
                    path
                });
        let display_path = String::from_utf8(relative_bytes.clone())
            .map_err(|_| "archive-entry-path-not-utf8".to_string())?;
        let case_key: String = display_path.nfc().flat_map(char::to_lowercase).collect();
        case_paths
            .entry(case_key)
            .or_default()
            .push(display_path.clone());

        let mode = git_blob_mode(entry.unix_mode())?;
        let size = entry.size();
        let (oid, sha256) = blob_digests(&mut entry, size)?;
        tree.insert_blob(relative, BlobEntry { mode, oid })?;
        if entries
            .insert(
                display_path,
                ArchiveFileEvidence {
                    mode,
                    bytes: size,
                    sha256,
                },
            )
            .is_some()
        {
            return Err("archive-entry-duplicate-or-type-conflict".into());
        }
        file_count += 1;
    }

    if file_count == 0 {
        return Err("archive-no-git-files".into());
    }
    let root_prefix = match root_mode {
        ArchiveTreeRootMode::StripSharedRoot => {
            String::from_utf8(root_prefix.ok_or_else(|| "archive-shared-root-missing".to_string())?)
                .map_err(|_| "archive-entry-path-not-utf8".to_string())?
        }
        ArchiveTreeRootMode::KeepTopLevel => ".".to_string(),
    };
    let (tree_oid, directory_count) = git_tree_oid(&tree);
    let git_tree_sha1 = hex_sha1(&tree_oid);
    let case_collision_groups: Vec<Vec<String>> = case_paths
        .into_values()
        .filter_map(|mut paths| {
            paths.sort();
            paths.dedup();
            (paths.len() > 1).then_some(paths)
        })
        .collect();
    if case_collision_groups.len() > MAX_CASE_COLLISION_GROUPS {
        return Err("archive-case-collision-groups-out-of-bounds".into());
    }
    let matches_expected = expected_git_tree_sha1
        .as_ref()
        .map(|expected| expected == &git_tree_sha1);

    Ok(ArchiveManifest {
        report: ArchiveGitTreeReport {
            version: REPORT_VERSION,
            archive: archive_path.to_string_lossy().into_owned(),
            root_prefix,
            zip_entry_count: archive.len(),
            file_count,
            directory_count,
            uncompressed_bytes,
            git_tree_sha1,
            expected_git_tree_sha1,
            matches_expected,
            case_collision_groups,
        },
        root_mode,
        entries,
    })
}

fn compare_manifests(
    subset: &ArchiveManifest,
    superset: &ArchiveManifest,
) -> Result<ArchiveContentInclusionReport, String> {
    if subset.root_mode != superset.root_mode {
        return Err("archive-root-mode-mismatch".into());
    }
    if !subset.report.case_collision_groups.is_empty()
        || !superset.report.case_collision_groups.is_empty()
    {
        return Err("archive-case-collision-ambiguous".into());
    }

    let mut matching_file_count = 0usize;
    let mut missing_file_count = 0usize;
    let mut changed_file_count = 0usize;
    let mut missing_paths = Vec::new();
    let mut changed_paths = Vec::new();
    for (path, subset_evidence) in &subset.entries {
        match superset.entries.get(path) {
            None => {
                missing_file_count += 1;
                push_bounded(&mut missing_paths, path);
            }
            Some(superset_evidence) if superset_evidence == subset_evidence => {
                matching_file_count += 1;
            }
            Some(_) => {
                changed_file_count += 1;
                push_bounded(&mut changed_paths, path);
            }
        }
    }

    let mut additional_file_count = 0usize;
    let mut additional_paths = Vec::new();
    for path in superset.entries.keys() {
        if !subset.entries.contains_key(path) {
            additional_file_count += 1;
            push_bounded(&mut additional_paths, path);
        }
    }

    let subset_content_included = missing_file_count == 0 && changed_file_count == 0;
    let archives_identical = subset_content_included && additional_file_count == 0;
    let paths_truncated = missing_file_count > missing_paths.len()
        || changed_file_count > changed_paths.len()
        || additional_file_count > additional_paths.len();
    let subset_manifest_digest = manifest_sha256(subset);
    let superset_manifest_digest = manifest_sha256(superset);
    let fingerprint = comparison_fingerprint(
        subset.root_mode,
        &subset_manifest_digest,
        &superset_manifest_digest,
    );

    Ok(ArchiveContentInclusionReport {
        version: COMPARISON_REPORT_VERSION,
        schema_kind: COMPARISON_SCHEMA_KIND,
        subset_archive: subset.report.archive.clone(),
        superset_archive: superset.report.archive.clone(),
        root_mode: subset.root_mode.label().to_string(),
        subset_root_prefix: subset.report.root_prefix.clone(),
        superset_root_prefix: superset.report.root_prefix.clone(),
        subset_file_count: subset.report.file_count,
        superset_file_count: superset.report.file_count,
        subset_uncompressed_bytes: subset.report.uncompressed_bytes,
        superset_uncompressed_bytes: superset.report.uncompressed_bytes,
        matching_file_count,
        missing_file_count,
        changed_file_count,
        additional_file_count,
        subset_content_included,
        archives_identical,
        missing_paths,
        changed_paths,
        additional_paths,
        paths_truncated,
        subset_manifest_sha256: hex_sha256(&subset_manifest_digest),
        superset_manifest_sha256: hex_sha256(&superset_manifest_digest),
        comparison_fingerprint_sha256: hex_sha256(&fingerprint),
    })
}

/// Prove that every logical file in `subset_archive_path` is present in `superset_archive_path`.
///
/// Both archives are parsed under the same root mode. File contents are streamed directly from the
/// ZIP readers and never extracted. Ambiguous case/Unicode-normalization collisions fail closed so
/// the result can be used as evidence for later operator-approved cleanup on macOS.
pub fn compare_zip_content_inclusion(
    subset_archive_path: &Path,
    superset_archive_path: &Path,
    root_mode: ArchiveTreeRootMode,
) -> Result<ArchiveContentInclusionReport, String> {
    let subset = inspect_zip_manifest_with_mode(subset_archive_path, None, root_mode)?;
    let superset = inspect_zip_manifest_with_mode(superset_archive_path, None, root_mode)?;
    compare_manifests(&subset, &superset)
}

/// Load each ZIP manifest once and compare every content-feasible unordered pair.
///
/// A relation is emitted only when every subset path, normalized Git mode, uncompressed length,
/// and SHA-256 matches the superset. Invalid or case-ambiguous archives remain explicit issues and
/// make `evidence_complete` false. This function performs no extraction or filesystem mutation.
pub fn audit_zip_content_inclusion_batch(
    archive_paths: &[PathBuf],
    root_mode: ArchiveTreeRootMode,
    options: ArchiveContentBatchOptions,
    generated_at_ms: u64,
) -> Result<ArchiveContentInclusionBatchReport, String> {
    if archive_paths.len() < 2 {
        return Err("archive-batch-requires-two-archives".into());
    }
    if options.max_archives < 2
        || options.max_archives > MAX_BATCH_ARCHIVES
        || options.max_pairs == 0
        || options.max_pairs > MAX_BATCH_PAIRS
        || archive_paths.len() > options.max_archives
    {
        return Err("archive-batch-limit-invalid".into());
    }

    let mut canonical_paths = Vec::with_capacity(archive_paths.len());
    let mut seen = BTreeSet::new();
    for path in archive_paths {
        let canonical = std::fs::canonicalize(path)
            .map_err(|_| "archive-batch-path-unavailable".to_string())?;
        let metadata = std::fs::symlink_metadata(&canonical)
            .map_err(|_| "archive-batch-path-unavailable".to_string())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err("archive-batch-path-not-regular-file".into());
        }
        let display = canonical
            .to_str()
            .ok_or_else(|| "archive-batch-path-not-utf8".to_string())?
            .to_string();
        if !seen.insert(display.clone()) {
            return Err("archive-batch-path-duplicate".into());
        }
        canonical_paths.push((canonical, display, metadata.len()));
    }
    canonical_paths.sort_by(|left, right| left.1.cmp(&right.1));

    let mut loaded = Vec::new();
    let mut issues = Vec::new();
    for (path, display, compressed_bytes) in canonical_paths {
        if compressed_bytes > MAX_COMPRESSED_ARCHIVE_BYTES {
            issues.push(ArchiveContentBatchIssue {
                archive_id: unavailable_archive_id(&display, compressed_bytes),
                archive: display,
                reason: "archive-compressed-size-out-of-bounds".into(),
            });
            continue;
        }
        let raw_digest = match raw_file_sha256(&path) {
            Ok(digest) => digest,
            Err(reason) => {
                issues.push(ArchiveContentBatchIssue {
                    archive_id: unavailable_archive_id(&display, compressed_bytes),
                    archive: display,
                    reason,
                });
                continue;
            }
        };
        match inspect_zip_manifest_with_mode(&path, None, root_mode) {
            Ok(manifest) if manifest.report.case_collision_groups.is_empty() => {
                let digest = manifest_sha256(&manifest);
                let archive = ArchiveContentBatchArchive {
                    archive_id: archive_id(&display, &raw_digest, Some(&digest), compressed_bytes),
                    archive: display,
                    compressed_bytes,
                    raw_archive_sha256: hex_sha256(&raw_digest),
                    file_count: manifest.report.file_count,
                    uncompressed_bytes: manifest.report.uncompressed_bytes,
                    root_prefix: manifest.report.root_prefix.clone(),
                    manifest_sha256: hex_sha256(&digest),
                };
                loaded.push(LoadedBatchArchive { manifest, archive });
            }
            Ok(_) => issues.push(ArchiveContentBatchIssue {
                archive_id: archive_id(&display, &raw_digest, None, compressed_bytes),
                archive: display,
                reason: "archive-case-collision-ambiguous".into(),
            }),
            Err(reason) => issues.push(ArchiveContentBatchIssue {
                archive_id: archive_id(&display, &raw_digest, None, compressed_bytes),
                archive: display,
                reason,
            }),
        }
    }

    let mut pair_comparison_count = 0usize;
    let mut relations = Vec::new();
    for left in 0..loaded.len() {
        for right in (left + 1)..loaded.len() {
            let left_manifest = &loaded[left].manifest;
            let right_manifest = &loaded[right].manifest;
            let left_can_be_subset = left_manifest.report.file_count
                <= right_manifest.report.file_count
                && left_manifest.report.uncompressed_bytes
                    <= right_manifest.report.uncompressed_bytes;
            let right_can_be_subset = right_manifest.report.file_count
                <= left_manifest.report.file_count
                && right_manifest.report.uncompressed_bytes
                    <= left_manifest.report.uncompressed_bytes;
            let (subset, superset) = match (left_can_be_subset, right_can_be_subset) {
                (true, false) => (left_manifest, right_manifest),
                (false, true) => (right_manifest, left_manifest),
                (true, true) => (left_manifest, right_manifest),
                (false, false) => continue,
            };
            pair_comparison_count += 1;
            if pair_comparison_count > options.max_pairs {
                return Err("archive-batch-pair-limit".into());
            }
            let comparison = compare_manifests(subset, superset)?;
            if comparison.subset_content_included {
                relations.push(comparison);
            }
        }
    }
    relations.sort_by(|left, right| {
        left.subset_archive
            .cmp(&right.subset_archive)
            .then_with(|| left.superset_archive.cmp(&right.superset_archive))
    });

    let archive_bytes: BTreeMap<&str, u64> = loaded
        .iter()
        .map(|item| (item.archive.archive.as_str(), item.archive.compressed_bytes))
        .collect();
    let reclaim_review_candidates: BTreeSet<&str> = relations
        .iter()
        .map(|relation| relation.subset_archive.as_str())
        .collect();
    let reclaim_review_compressed_bytes =
        reclaim_review_candidates.iter().fold(0u64, |total, path| {
            total.saturating_add(archive_bytes.get(path).copied().unwrap_or_default())
        });
    let identical_relation_count = relations
        .iter()
        .filter(|relation| relation.archives_identical)
        .count();
    let strict_inclusion_relation_count = relations.len() - identical_relation_count;
    let archives: Vec<_> = loaded.into_iter().map(|item| item.archive).collect();
    let fingerprint = batch_fingerprint(root_mode, &archives, &issues, &relations);

    Ok(ArchiveContentInclusionBatchReport {
        version: BATCH_REPORT_VERSION,
        schema_kind: BATCH_SCHEMA_KIND,
        generated_at_ms,
        root_mode: root_mode.label().into(),
        archive_input_count: archive_paths.len(),
        archive_valid_count: archives.len(),
        issue_count: issues.len(),
        pair_comparison_count,
        inclusion_relation_count: relations.len(),
        strict_inclusion_relation_count,
        identical_relation_count,
        reclaim_review_candidate_count: reclaim_review_candidates.len(),
        reclaim_review_compressed_bytes,
        evidence_complete: issues.is_empty(),
        batch_fingerprint_sha256: fingerprint,
        archives,
        issues,
        relations,
        notices: vec![
            "read-only; ZIP entries streamed once per archive and never extracted".into(),
            "inclusion requires exact path, normalized Git mode, uncompressed length, and SHA-256"
                .into(),
            "reclaim-review candidates are not deletion approval".into(),
            "container metadata, signatures, acquisition context, and production lineage require separate review"
                .into(),
            "filename dates and filesystem timestamps are not used as content-inclusion evidence"
                .into(),
        ],
    })
}

fn valid_lower_hex64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn naruon_archive_identity(
    archive: &ArchiveContentBatchArchive,
) -> Result<NaruonArchiveContentIdentity, String> {
    if !valid_lower_hex64(&archive.archive_id)
        || !valid_lower_hex64(&archive.raw_archive_sha256)
        || !valid_lower_hex64(&archive.manifest_sha256)
    {
        return Err("naruon-archive-content-identity-invalid".into());
    }
    Ok(NaruonArchiveContentIdentity {
        archive_id: archive.archive_id.clone(),
        raw_archive_sha256: archive.raw_archive_sha256.clone(),
        manifest_sha256: archive.manifest_sha256.clone(),
        compressed_bytes: archive.compressed_bytes,
        file_count: archive.file_count,
        uncompressed_bytes: archive.uncompressed_bytes,
    })
}

fn naruon_lineage_fingerprint(
    report: &ArchiveContentInclusionBatchReport,
    relations: &[NaruonArchiveInclusionRelation],
    issues: &[NaruonArchiveInclusionIssue],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"disksage.naruon.archive-content-inclusion\0v1\0");
    hasher.update(report.generated_at_ms.to_le_bytes());
    update_len_prefixed(&mut hasher, report.batch_fingerprint_sha256.as_bytes());
    hasher.update((relations.len() as u64).to_le_bytes());
    for relation in relations {
        update_len_prefixed(&mut hasher, relation.subset.archive_id.as_bytes());
        update_len_prefixed(&mut hasher, relation.superset.archive_id.as_bytes());
        update_len_prefixed(
            &mut hasher,
            relation.comparison_fingerprint_sha256.as_bytes(),
        );
    }
    hasher.update((issues.len() as u64).to_le_bytes());
    for issue in issues {
        update_len_prefixed(&mut hasher, issue.archive_id.as_bytes());
        update_len_prefixed(&mut hasher, issue.reason.as_bytes());
    }
    hex_sha256(&hasher.finalize().into())
}

/// Export a bounded, path-free archive inclusion contract for a Naruon receiver.
///
/// Raw ZIP SHA-256 distinguishes different containers while the manifest SHA-256 binds logical
/// uncompressed content. This export contains no local path or filename and grants no cleanup or
/// cloud-write authority.
pub fn export_naruon_archive_inclusion_lineage(
    report: &ArchiveContentInclusionBatchReport,
) -> Result<NaruonArchiveInclusionLineageEnvelope, String> {
    if report.version != BATCH_REPORT_VERSION
        || report.schema_kind != BATCH_SCHEMA_KIND
        || report.archive_input_count != report.archive_valid_count + report.issue_count
        || report.archive_valid_count != report.archives.len()
        || report.issue_count != report.issues.len()
        || report.inclusion_relation_count != report.relations.len()
        || !valid_lower_hex64(&report.batch_fingerprint_sha256)
    {
        return Err("naruon-archive-batch-shape-invalid".into());
    }
    let archives: BTreeMap<&str, &ArchiveContentBatchArchive> = report
        .archives
        .iter()
        .map(|archive| (archive.archive.as_str(), archive))
        .collect();
    let mut relations = Vec::with_capacity(report.relations.len());
    for relation in &report.relations {
        if !relation.subset_content_included
            || !valid_lower_hex64(&relation.comparison_fingerprint_sha256)
        {
            return Err("naruon-archive-relation-invalid".into());
        }
        let subset = archives
            .get(relation.subset_archive.as_str())
            .ok_or_else(|| "naruon-archive-relation-binding-missing".to_string())?;
        let superset = archives
            .get(relation.superset_archive.as_str())
            .ok_or_else(|| "naruon-archive-relation-binding-missing".to_string())?;
        relations.push(NaruonArchiveInclusionRelation {
            subset: naruon_archive_identity(subset)?,
            superset: naruon_archive_identity(superset)?,
            comparison_fingerprint_sha256: relation.comparison_fingerprint_sha256.clone(),
            archives_identical: relation.archives_identical,
            additional_file_count: relation.additional_file_count,
        });
    }
    let issues = report
        .issues
        .iter()
        .map(|issue| {
            if !valid_lower_hex64(&issue.archive_id)
                || issue.reason.is_empty()
                || issue.reason.chars().any(char::is_control)
            {
                return Err("naruon-archive-issue-invalid".to_string());
            }
            Ok(NaruonArchiveInclusionIssue {
                archive_id: issue.archive_id.clone(),
                reason: issue.reason.clone(),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let lineage_fingerprint_sha256 = naruon_lineage_fingerprint(report, &relations, &issues);

    Ok(NaruonArchiveInclusionLineageEnvelope {
        schema_version: 1,
        schema_kind: "disksage.naruon.archive-content-inclusion",
        generated_at_ms: report.generated_at_ms,
        batch_fingerprint_sha256: report.batch_fingerprint_sha256.clone(),
        lineage_fingerprint_sha256,
        root_mode: report.root_mode.clone(),
        evidence_complete: report.evidence_complete,
        relation_count: relations.len(),
        reclaim_review_candidate_count: report.reclaim_review_candidate_count,
        reclaim_review_compressed_bytes: report.reclaim_review_compressed_bytes,
        relations,
        issues,
        content_evidence: vec![
            "raw-archive-sha256".into(),
            "validated-logical-path".into(),
            "normalized-git-mode".into(),
            "uncompressed-byte-length".into(),
            "uncompressed-content-sha256".into(),
        ],
        local_paths_included: false,
        filename_dates_used: false,
        filesystem_timestamps_used: false,
        deletion_authorized: false,
        cloud_write_executed: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn fixture(entries: &[(&str, &[u8], u32)]) -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("fixture.zip");
        let file = File::create(&archive_path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        archive
            .add_directory("repo/", SimpleFileOptions::default())
            .unwrap();
        for (path, contents, mode) in entries {
            archive
                .start_file(
                    format!("repo/{path}"),
                    SimpleFileOptions::default().unix_permissions(*mode),
                )
                .unwrap();
            archive.write_all(contents).unwrap();
        }
        archive.finish().unwrap();
        temp
    }

    fn generic_fixture(
        entries: &[(&str, &[u8], u32, zip::CompressionMethod)],
    ) -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("fixture.zip");
        let file = File::create(&archive_path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        for (path, contents, mode, compression) in entries {
            archive
                .start_file(
                    path,
                    SimpleFileOptions::default()
                        .compression_method(*compression)
                        .unix_permissions(*mode),
                )
                .unwrap();
            archive.write_all(contents).unwrap();
        }
        archive.finish().unwrap();
        temp
    }

    #[test]
    fn single_file_matches_known_git_tree_object() {
        let temp = fixture(&[("a.txt", b"hello\n", 0o100644)]);
        let report = inspect_zip_git_tree(
            &temp.path().join("fixture.zip"),
            Some("2E81171448EB9F2EE3821E3D447AA6B2FE3DDBA1"),
        )
        .unwrap();
        assert_eq!(report.root_prefix, "repo");
        assert_eq!(report.file_count, 1);
        assert_eq!(report.directory_count, 0);
        assert_eq!(report.uncompressed_bytes, 6);
        assert_eq!(
            report.git_tree_sha1,
            "2e81171448eb9f2ee3821e3d447aa6b2fe3ddba1"
        );
        assert_eq!(report.matches_expected, Some(true));
    }

    #[test]
    fn case_collisions_remain_distinct_and_are_reported_without_extraction() {
        let temp = fixture(&[
            (".Jules/palette.md", b"upper", 0o100644),
            (".jules/palette.md", b"lower", 0o100644),
            ("Caf\u{e9}.md", b"composed", 0o100644),
            ("Cafe\u{301}.md", b"decomposed", 0o100644),
        ]);
        let report = inspect_zip_git_tree(&temp.path().join("fixture.zip"), None).unwrap();
        assert_eq!(report.file_count, 4);
        assert_eq!(report.directory_count, 2);
        assert_eq!(
            report.case_collision_groups,
            [
                vec![
                    ".Jules/palette.md".to_string(),
                    ".jules/palette.md".to_string()
                ],
                vec!["Cafe\u{301}.md".to_string(), "Caf\u{e9}.md".to_string()]
            ]
        );
        assert_eq!(report.matches_expected, None);
    }

    #[test]
    fn generic_multi_root_zip_requires_explicit_keep_top_level_mode() {
        let temp = generic_fixture(&[
            (
                "audio/clip.txt",
                b"clip\n",
                0o100644,
                zip::CompressionMethod::Stored,
            ),
            (
                "notes.md",
                b"notes\n",
                0o100644,
                zip::CompressionMethod::Stored,
            ),
        ]);
        let path = temp.path().join("fixture.zip");

        assert_eq!(
            inspect_zip_git_tree(&path, None).unwrap_err(),
            "archive-shared-root-mismatch"
        );
        let report =
            inspect_zip_git_tree_with_mode(&path, None, ArchiveTreeRootMode::KeepTopLevel).unwrap();
        assert_eq!(report.root_prefix, ".");
        assert_eq!(report.file_count, 2);
        assert_eq!(report.directory_count, 1);
    }

    #[test]
    fn logical_tree_ignores_zip_order_and_compression() {
        let stored = generic_fixture(&[
            (
                "a.txt",
                b"alpha\n",
                0o100644,
                zip::CompressionMethod::Stored,
            ),
            (
                "nested/b.txt",
                b"beta\n",
                0o100755,
                zip::CompressionMethod::Stored,
            ),
        ]);
        let deflated = generic_fixture(&[
            (
                "nested/b.txt",
                b"beta\n",
                0o100755,
                zip::CompressionMethod::Deflated,
            ),
            (
                "a.txt",
                b"alpha\n",
                0o100644,
                zip::CompressionMethod::Deflated,
            ),
        ]);

        let stored_report = inspect_zip_git_tree_with_mode(
            &stored.path().join("fixture.zip"),
            None,
            ArchiveTreeRootMode::KeepTopLevel,
        )
        .unwrap();
        let deflated_report = inspect_zip_git_tree_with_mode(
            &deflated.path().join("fixture.zip"),
            None,
            ArchiveTreeRootMode::KeepTopLevel,
        )
        .unwrap();

        assert_eq!(stored_report.git_tree_sha1, deflated_report.git_tree_sha1);
        assert_eq!(stored_report.file_count, deflated_report.file_count);
        assert_eq!(
            stored_report.uncompressed_bytes,
            deflated_report.uncompressed_bytes
        );
    }

    #[test]
    fn content_inclusion_proves_every_subset_entry_by_path_mode_size_and_sha256() {
        let subset = generic_fixture(&[
            (
                "a.txt",
                b"alpha\n",
                0o100644,
                zip::CompressionMethod::Stored,
            ),
            (
                "nested/b.txt",
                b"beta\n",
                0o100755,
                zip::CompressionMethod::Deflated,
            ),
        ]);
        let superset = generic_fixture(&[
            (
                "extra/c.txt",
                b"gamma\n",
                0o100644,
                zip::CompressionMethod::Deflated,
            ),
            (
                "nested/b.txt",
                b"beta\n",
                0o100755,
                zip::CompressionMethod::Stored,
            ),
            (
                "a.txt",
                b"alpha\n",
                0o100644,
                zip::CompressionMethod::Deflated,
            ),
        ]);

        let report = compare_zip_content_inclusion(
            &subset.path().join("fixture.zip"),
            &superset.path().join("fixture.zip"),
            ArchiveTreeRootMode::KeepTopLevel,
        )
        .unwrap();

        assert!(report.subset_content_included);
        assert_eq!(report.schema_kind, "disksage.archive-content-inclusion");
        assert!(!report.archives_identical);
        assert_eq!(report.matching_file_count, 2);
        assert_eq!(report.missing_file_count, 0);
        assert_eq!(report.changed_file_count, 0);
        assert_eq!(report.additional_file_count, 1);
        assert_eq!(report.additional_paths, ["extra/c.txt"]);
        assert_eq!(report.subset_manifest_sha256.len(), 64);
        assert_eq!(report.superset_manifest_sha256.len(), 64);
        assert_eq!(report.comparison_fingerprint_sha256.len(), 64);
        assert!(!report.paths_truncated);
    }

    #[test]
    fn content_inclusion_reports_changed_and_missing_entries_fail_closed() {
        let subset = generic_fixture(&[
            (
                "changed.txt",
                b"original",
                0o100644,
                zip::CompressionMethod::Stored,
            ),
            (
                "mode.txt",
                b"same bytes",
                0o100644,
                zip::CompressionMethod::Stored,
            ),
            (
                "missing.txt",
                b"required",
                0o100644,
                zip::CompressionMethod::Stored,
            ),
        ]);
        let superset = generic_fixture(&[
            (
                "changed.txt",
                b"different",
                0o100644,
                zip::CompressionMethod::Stored,
            ),
            (
                "mode.txt",
                b"same bytes",
                0o100755,
                zip::CompressionMethod::Stored,
            ),
        ]);

        let report = compare_zip_content_inclusion(
            &subset.path().join("fixture.zip"),
            &superset.path().join("fixture.zip"),
            ArchiveTreeRootMode::KeepTopLevel,
        )
        .unwrap();

        assert!(!report.subset_content_included);
        assert_eq!(report.matching_file_count, 0);
        assert_eq!(report.changed_paths, ["changed.txt", "mode.txt"]);
        assert_eq!(report.missing_paths, ["missing.txt"]);
    }

    #[test]
    fn content_inclusion_rejects_case_or_normalization_ambiguous_archives() {
        let ambiguous = generic_fixture(&[
            (
                "Cafe\u{301}.md",
                b"decomposed",
                0o100644,
                zip::CompressionMethod::Stored,
            ),
            (
                "Caf\u{e9}.md",
                b"composed",
                0o100644,
                zip::CompressionMethod::Stored,
            ),
        ]);
        let superset = generic_fixture(&[(
            "Caf\u{e9}.md",
            b"composed",
            0o100644,
            zip::CompressionMethod::Stored,
        )]);

        assert_eq!(
            compare_zip_content_inclusion(
                &ambiguous.path().join("fixture.zip"),
                &superset.path().join("fixture.zip"),
                ArchiveTreeRootMode::KeepTopLevel,
            )
            .unwrap_err(),
            "archive-case-collision-ambiguous"
        );
    }

    #[test]
    fn batch_loads_each_archive_once_and_finds_strict_inclusion() {
        let subset = generic_fixture(&[(
            "a.txt",
            b"alpha\n",
            0o100644,
            zip::CompressionMethod::Stored,
        )]);
        let superset = generic_fixture(&[
            (
                "a.txt",
                b"alpha\n",
                0o100644,
                zip::CompressionMethod::Deflated,
            ),
            ("b.txt", b"beta\n", 0o100644, zip::CompressionMethod::Stored),
        ]);
        let paths = vec![
            subset.path().join("fixture.zip"),
            superset.path().join("fixture.zip"),
        ];

        let report = audit_zip_content_inclusion_batch(
            &paths,
            ArchiveTreeRootMode::KeepTopLevel,
            ArchiveContentBatchOptions::default(),
            42,
        )
        .unwrap();

        assert_eq!(report.schema_kind, BATCH_SCHEMA_KIND);
        assert_eq!(report.generated_at_ms, 42);
        assert_eq!(report.archive_valid_count, 2);
        assert_eq!(report.pair_comparison_count, 1);
        assert_eq!(report.inclusion_relation_count, 1);
        assert_eq!(report.strict_inclusion_relation_count, 1);
        assert_eq!(report.identical_relation_count, 0);
        assert_eq!(report.reclaim_review_candidate_count, 1);
        assert!(report.reclaim_review_compressed_bytes > 0);
        assert!(report.evidence_complete);
        assert_eq!(report.batch_fingerprint_sha256.len(), 64);
        assert!(report.relations[0].subset_content_included);
        assert!(report
            .archives
            .iter()
            .all(|archive| archive.raw_archive_sha256.len() == 64));

        let naruon = export_naruon_archive_inclusion_lineage(&report).unwrap();
        let encoded = serde_json::to_string(&naruon).unwrap();
        assert_eq!(
            naruon.schema_kind,
            "disksage.naruon.archive-content-inclusion"
        );
        assert_eq!(naruon.relation_count, 1);
        assert_eq!(naruon.lineage_fingerprint_sha256.len(), 64);
        assert!(!naruon.local_paths_included);
        assert!(!naruon.filename_dates_used);
        assert!(!naruon.filesystem_timestamps_used);
        assert!(!naruon.deletion_authorized);
        assert!(!naruon.cloud_write_executed);
        assert!(!encoded.contains("fixture.zip"));
        assert!(!encoded.contains(subset.path().to_string_lossy().as_ref()));
        assert!(!encoded.contains(superset.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn batch_deduplicates_identical_reclaim_candidates_and_stabilizes_fingerprint() {
        let first = generic_fixture(&[(
            "same.txt",
            b"same\n",
            0o100644,
            zip::CompressionMethod::Stored,
        )]);
        let second = generic_fixture(&[(
            "same.txt",
            b"same\n",
            0o100644,
            zip::CompressionMethod::Deflated,
        )]);
        let third = generic_fixture(&[(
            "same.txt",
            b"same\n",
            0o100644,
            zip::CompressionMethod::Stored,
        )]);
        let paths = vec![
            first.path().join("fixture.zip"),
            second.path().join("fixture.zip"),
            third.path().join("fixture.zip"),
        ];

        let first_report = audit_zip_content_inclusion_batch(
            &paths,
            ArchiveTreeRootMode::KeepTopLevel,
            ArchiveContentBatchOptions::default(),
            1,
        )
        .unwrap();
        let second_report = audit_zip_content_inclusion_batch(
            &paths,
            ArchiveTreeRootMode::KeepTopLevel,
            ArchiveContentBatchOptions::default(),
            2,
        )
        .unwrap();

        assert_eq!(first_report.pair_comparison_count, 3);
        assert_eq!(first_report.identical_relation_count, 3);
        assert_eq!(first_report.reclaim_review_candidate_count, 2);
        let naruon = export_naruon_archive_inclusion_lineage(&first_report).unwrap();
        assert!(naruon
            .relations
            .iter()
            .all(|relation| relation.subset.archive_id != relation.superset.archive_id));
        assert_eq!(
            first_report.batch_fingerprint_sha256,
            second_report.batch_fingerprint_sha256
        );
    }

    #[test]
    fn batch_keeps_invalid_zip_as_explicit_evidence_gap() {
        let invalid = tempfile::tempdir().unwrap();
        let invalid_path = invalid.path().join("invalid.zip");
        std::fs::write(&invalid_path, b"not a zip").unwrap();
        let subset = generic_fixture(&[("a.txt", b"a", 0o100644, zip::CompressionMethod::Stored)]);
        let superset = generic_fixture(&[
            ("a.txt", b"a", 0o100644, zip::CompressionMethod::Stored),
            ("b.txt", b"b", 0o100644, zip::CompressionMethod::Stored),
        ]);

        let report = audit_zip_content_inclusion_batch(
            &[
                invalid_path,
                subset.path().join("fixture.zip"),
                superset.path().join("fixture.zip"),
            ],
            ArchiveTreeRootMode::KeepTopLevel,
            ArchiveContentBatchOptions::default(),
            3,
        )
        .unwrap();

        assert_eq!(report.archive_input_count, 3);
        assert_eq!(report.archive_valid_count, 2);
        assert_eq!(report.issue_count, 1);
        assert!(!report.evidence_complete);
        assert_eq!(report.inclusion_relation_count, 1);
        assert_eq!(report.issues[0].reason, "archive-central-directory-invalid");
        let naruon = export_naruon_archive_inclusion_lineage(&report).unwrap();
        assert!(!naruon.evidence_complete);
        assert_eq!(naruon.issues.len(), 1);
        assert_eq!(naruon.issues[0].reason, "archive-central-directory-invalid");
    }

    #[test]
    fn batch_limits_fail_closed_before_partial_success_is_reported() {
        let first = generic_fixture(&[(
            "same.txt",
            b"same",
            0o100644,
            zip::CompressionMethod::Stored,
        )]);
        let second = generic_fixture(&[(
            "same.txt",
            b"same",
            0o100644,
            zip::CompressionMethod::Stored,
        )]);
        let third = generic_fixture(&[(
            "same.txt",
            b"same",
            0o100644,
            zip::CompressionMethod::Stored,
        )]);
        let paths = vec![
            first.path().join("fixture.zip"),
            second.path().join("fixture.zip"),
            third.path().join("fixture.zip"),
        ];

        assert_eq!(
            audit_zip_content_inclusion_batch(
                &paths,
                ArchiveTreeRootMode::KeepTopLevel,
                ArchiveContentBatchOptions {
                    max_archives: 2,
                    max_pairs: 10,
                },
                0,
            )
            .unwrap_err(),
            "archive-batch-limit-invalid"
        );
        assert_eq!(
            audit_zip_content_inclusion_batch(
                &paths,
                ArchiveTreeRootMode::KeepTopLevel,
                ArchiveContentBatchOptions {
                    max_archives: 3,
                    max_pairs: 2,
                },
                0,
            )
            .unwrap_err(),
            "archive-batch-pair-limit"
        );
    }

    #[test]
    fn parser_rejects_unsafe_paths_and_invalid_expected_hashes() {
        assert!(zip_path_components(b"repo/../secret", false).is_err());
        assert!(zip_path_components(b"/repo/file", false).is_err());
        assert!(zip_path_components(b"repo\\file", false).is_err());
        assert_eq!(
            validate_expected_tree(Some("not-a-tree")).unwrap_err(),
            "expected-git-tree-sha1-invalid"
        );
    }
}
