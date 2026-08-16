//! Bounded, read-only evidence for split ZIP archive sets.
//!
//! A `.zip.partNNN` file is not independently useful for cloud offload. This module groups sibling
//! parts, records internal gaps and duplicate indices, and produces a path-redacted summary. Even a
//! contiguous local sequence remains terminal-unverified without an authoritative manifest, so no
//! result from this module authorizes automatic deletion.

use crate::duplicate_audit::bound_read_root::{BoundEntryKind, BoundReadRoot};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

pub const MULTIPART_AUDIT_SCHEMA_VERSION: u32 = 1;
pub const DEFAULT_MAX_ENTRIES: usize = 200_000;
pub const MAX_SCAN_DEPTH: usize = 64;
const MAX_RECORDED_ISSUE_KINDS: usize = 32;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, PartialOrd, Ord,
)]
#[serde(rename_all = "kebab-case")]
pub enum MultipartSetState {
    MissingParts,
    DuplicatePartIndex,
    ContiguousTerminalUnverified,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MultipartPartObservation {
    pub relative_path: String,
    pub part_index: u32,
    pub bytes: u64,
    pub modified_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MultipartArchiveSetAudit {
    pub set_fingerprint: String,
    pub relative_directory: String,
    pub base_name: String,
    pub state: MultipartSetState,
    pub member_count: usize,
    pub member_bytes: u64,
    pub present_parts: Vec<u32>,
    pub missing_parts: Vec<u32>,
    pub duplicate_part_indices: Vec<u32>,
    pub highest_observed_part: u32,
    pub complete_reassembly_possible: Option<bool>,
    pub requires_human_review: bool,
    pub automatic_discard_allowed: bool,
    pub members: Vec<MultipartPartObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MultipartArchiveAuditReport {
    pub schema_version: u32,
    pub observed_at_ms: u64,
    pub source_root: String,
    pub source_scope_fingerprint: String,
    pub evidence_complete: bool,
    pub entries_seen: usize,
    pub issue_counts: BTreeMap<String, u64>,
    pub set_count: usize,
    pub part_count: usize,
    pub part_bytes: u64,
    pub incomplete_set_count: usize,
    pub ambiguous_set_count: usize,
    pub terminal_unverified_set_count: usize,
    pub discard_review_bytes: u64,
    pub audit_fingerprint: String,
    pub mutation_performed: bool,
    pub sets: Vec<MultipartArchiveSetAudit>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MultipartArchiveSetSummary {
    pub set_fingerprint: String,
    pub state: MultipartSetState,
    pub member_count: usize,
    pub member_bytes: u64,
    pub present_parts: Vec<u32>,
    pub missing_parts: Vec<u32>,
    pub duplicate_part_indices: Vec<u32>,
    pub highest_observed_part: u32,
    pub complete_reassembly_possible: Option<bool>,
    pub requires_human_review: bool,
    pub automatic_discard_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MultipartArchiveAuditSummary {
    pub schema_version: u32,
    pub output_mode: String,
    pub observed_at_ms: u64,
    pub source_scope_fingerprint: String,
    pub evidence_complete: bool,
    pub entries_seen: usize,
    pub issue_counts: BTreeMap<String, u64>,
    pub set_count: usize,
    pub part_count: usize,
    pub part_bytes: u64,
    pub incomplete_set_count: usize,
    pub ambiguous_set_count: usize,
    pub terminal_unverified_set_count: usize,
    pub discard_review_bytes: u64,
    pub audit_fingerprint: String,
    pub mutation_performed: bool,
    pub human_discard_approval_required: bool,
    pub automatic_discard_allowed: bool,
    pub notices: Vec<String>,
    pub redacted_from_summary: Vec<String>,
    pub sets: Vec<MultipartArchiveSetSummary>,
}

#[derive(Debug, Clone)]
struct RawObservation {
    relative_path: String,
    base_name: String,
    part_index: u32,
    bytes: u64,
    modified_ms: u64,
}

fn normalized(value: &str) -> String {
    value.nfc().collect()
}

pub fn parse_multipart_archive_name(name: &str) -> Option<(String, u32)> {
    let normalized_name = name.to_ascii_lowercase();
    let (base, part) = normalized_name.rsplit_once(".part")?;
    if !base.ends_with(".zip") || part.len() != 3 || !part.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Some((base.to_string(), part.parse().ok()?))
}

fn valid_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

fn source_scope_fingerprint(source_root: &str) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-multipart-source-scope-v1\0");
    hasher.update(normalized(source_root).as_bytes());
    hasher.finalize().to_hex().to_string()
}

fn set_fingerprint(
    source_root: &str,
    relative_directory: &str,
    base_name: &str,
    present_parts: &[u32],
    missing_parts: &[u32],
    duplicate_part_indices: &[u32],
    members: &[MultipartPartObservation],
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-multipart-archive-set-v1\0");
    for value in [
        normalized(source_root),
        normalized(relative_directory),
        normalized(base_name),
    ] {
        hasher.update(value.as_bytes());
        hasher.update(&[0]);
    }
    for (tag, values) in [
        (b'p', present_parts),
        (b'm', missing_parts),
        (b'd', duplicate_part_indices),
    ] {
        hasher.update(&[tag]);
        for value in values {
            hasher.update(&value.to_le_bytes());
        }
    }
    for member in members {
        hasher.update(normalized(&member.relative_path).as_bytes());
        hasher.update(&[0]);
        hasher.update(&member.part_index.to_le_bytes());
        hasher.update(&member.bytes.to_le_bytes());
        hasher.update(&member.modified_ms.to_le_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn audit_fingerprint(
    source_root: &str,
    evidence_complete: bool,
    entries_seen: usize,
    issue_counts: &BTreeMap<String, u64>,
    sets: &[MultipartArchiveSetAudit],
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-multipart-archive-audit-v1\0");
    hasher.update(normalized(source_root).as_bytes());
    hasher.update(&[0, evidence_complete as u8]);
    hasher.update(&(entries_seen as u64).to_le_bytes());
    for (reason, count) in issue_counts {
        hasher.update(reason.as_bytes());
        hasher.update(&[0]);
        hasher.update(&count.to_le_bytes());
    }
    for set in sets {
        hasher.update(set.set_fingerprint.as_bytes());
        hasher.update(&[0]);
    }
    hasher.finalize().to_hex().to_string()
}

fn increment_issue(issues: &mut BTreeMap<String, u64>, reason: &str) {
    if issues.contains_key(reason) || issues.len() < MAX_RECORDED_ISSUE_KINDS {
        *issues.entry(reason.to_string()).or_default() += 1;
    } else {
        *issues
            .entry("additional-issue-kinds-truncated".into())
            .or_default() += 1;
    }
}

fn build_report(
    source_root: String,
    observed_at_ms: u64,
    entries_seen: usize,
    evidence_complete: bool,
    issue_counts: BTreeMap<String, u64>,
    observations: Vec<RawObservation>,
) -> MultipartArchiveAuditReport {
    let mut groups: BTreeMap<(String, String), Vec<RawObservation>> = BTreeMap::new();
    for observation in observations {
        let relative = Path::new(&observation.relative_path);
        let directory = relative
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(|parent| normalized(&parent.to_string_lossy()))
            .unwrap_or_else(|| ".".into());
        groups
            .entry((directory, normalized(&observation.base_name)))
            .or_default()
            .push(observation);
    }

    let mut sets = Vec::with_capacity(groups.len());
    for ((relative_directory, base_name), mut raw_members) in groups {
        raw_members.sort_by(|left, right| {
            left.part_index
                .cmp(&right.part_index)
                .then_with(|| left.relative_path.cmp(&right.relative_path))
        });
        let mut by_part: BTreeMap<u32, usize> = BTreeMap::new();
        for member in &raw_members {
            *by_part.entry(member.part_index).or_default() += 1;
        }
        let present_parts = by_part.keys().copied().collect::<Vec<_>>();
        let highest_observed_part = present_parts.last().copied().unwrap_or(0);
        let missing_parts = (0..=highest_observed_part)
            .filter(|part| !by_part.contains_key(part))
            .collect::<Vec<_>>();
        let duplicate_part_indices = by_part
            .iter()
            .filter_map(|(part, count)| (*count > 1).then_some(*part))
            .collect::<Vec<_>>();
        let state = if !duplicate_part_indices.is_empty() {
            MultipartSetState::DuplicatePartIndex
        } else if !missing_parts.is_empty() {
            MultipartSetState::MissingParts
        } else {
            MultipartSetState::ContiguousTerminalUnverified
        };
        let complete_reassembly_possible = match state {
            MultipartSetState::MissingParts | MultipartSetState::DuplicatePartIndex => Some(false),
            MultipartSetState::ContiguousTerminalUnverified => None,
        };
        let members = raw_members
            .into_iter()
            .map(|member| MultipartPartObservation {
                relative_path: member.relative_path,
                part_index: member.part_index,
                bytes: member.bytes,
                modified_ms: member.modified_ms,
            })
            .collect::<Vec<_>>();
        let member_bytes = members
            .iter()
            .fold(0u64, |total, member| total.saturating_add(member.bytes));
        let fingerprint = set_fingerprint(
            &source_root,
            &relative_directory,
            &base_name,
            &present_parts,
            &missing_parts,
            &duplicate_part_indices,
            &members,
        );
        sets.push(MultipartArchiveSetAudit {
            set_fingerprint: fingerprint,
            relative_directory,
            base_name,
            state,
            member_count: members.len(),
            member_bytes,
            present_parts,
            missing_parts,
            duplicate_part_indices,
            highest_observed_part,
            complete_reassembly_possible,
            requires_human_review: true,
            automatic_discard_allowed: false,
            members,
        });
    }
    sets.sort_by(|left, right| left.set_fingerprint.cmp(&right.set_fingerprint));

    let part_count = sets.iter().map(|set| set.member_count).sum();
    let part_bytes = sets
        .iter()
        .fold(0u64, |total, set| total.saturating_add(set.member_bytes));
    let incomplete_set_count = sets
        .iter()
        .filter(|set| set.state == MultipartSetState::MissingParts)
        .count();
    let ambiguous_set_count = sets
        .iter()
        .filter(|set| set.state == MultipartSetState::DuplicatePartIndex)
        .count();
    let terminal_unverified_set_count = sets
        .iter()
        .filter(|set| set.state == MultipartSetState::ContiguousTerminalUnverified)
        .count();
    let discard_review_bytes = sets.iter().fold(0u64, |total, set| {
        if set.complete_reassembly_possible == Some(false) {
            total.saturating_add(set.member_bytes)
        } else {
            total
        }
    });
    let fingerprint = audit_fingerprint(
        &source_root,
        evidence_complete,
        entries_seen,
        &issue_counts,
        &sets,
    );

    MultipartArchiveAuditReport {
        schema_version: MULTIPART_AUDIT_SCHEMA_VERSION,
        observed_at_ms,
        source_scope_fingerprint: source_scope_fingerprint(&source_root),
        source_root,
        evidence_complete,
        entries_seen,
        issue_counts,
        set_count: sets.len(),
        part_count,
        part_bytes,
        incomplete_set_count,
        ambiguous_set_count,
        terminal_unverified_set_count,
        discard_review_bytes,
        audit_fingerprint: fingerprint,
        mutation_performed: false,
        sets,
    }
}

pub fn summarize_multipart_audit(
    report: &MultipartArchiveAuditReport,
) -> MultipartArchiveAuditSummary {
    MultipartArchiveAuditSummary {
        schema_version: report.schema_version,
        output_mode: "multipart-archive-audit-summary".into(),
        observed_at_ms: report.observed_at_ms,
        source_scope_fingerprint: report.source_scope_fingerprint.clone(),
        evidence_complete: report.evidence_complete,
        entries_seen: report.entries_seen,
        issue_counts: report.issue_counts.clone(),
        set_count: report.set_count,
        part_count: report.part_count,
        part_bytes: report.part_bytes,
        incomplete_set_count: report.incomplete_set_count,
        ambiguous_set_count: report.ambiguous_set_count,
        terminal_unverified_set_count: report.terminal_unverified_set_count,
        discard_review_bytes: report.discard_review_bytes,
        audit_fingerprint: report.audit_fingerprint.clone(),
        mutation_performed: false,
        human_discard_approval_required: report.discard_review_bytes > 0,
        automatic_discard_allowed: false,
        notices: vec![
            "read-only-dry-run".into(),
            "missing-parts-prove-complete-reassembly-unavailable-from-local-set".into(),
            "contiguous-parts-do-not-prove-terminal-part".into(),
            "partial-content-recovery-may-still-be-possible".into(),
            "fresh-audit-and-explicit-human-approval-required-before-discard".into(),
        ],
        redacted_from_summary: vec![
            "absolute-source-root".into(),
            "relative-directory".into(),
            "archive-base-name".into(),
            "member-relative-paths".into(),
            "member-modification-times".into(),
        ],
        sets: report
            .sets
            .iter()
            .map(|set| MultipartArchiveSetSummary {
                set_fingerprint: set.set_fingerprint.clone(),
                state: set.state,
                member_count: set.member_count,
                member_bytes: set.member_bytes,
                present_parts: set.present_parts.clone(),
                missing_parts: set.missing_parts.clone(),
                duplicate_part_indices: set.duplicate_part_indices.clone(),
                highest_observed_part: set.highest_observed_part,
                complete_reassembly_possible: set.complete_reassembly_possible,
                requires_human_review: set.requires_human_review,
                automatic_discard_allowed: false,
            })
            .collect(),
    }
}

fn modified_ms(metadata: &std::fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as u64)
}

/// Recursively collect split-ZIP evidence without following symlinks.
///
/// Traversal is entry- and depth-bounded. Any read/stat/depth failure marks the report incomplete;
/// incomplete evidence remains useful for diagnosis but can never justify automatic discard.
#[cfg(not(coverage))]
pub fn collect_multipart_archive_audit(
    source_root: &Path,
    observed_at_ms: u64,
    max_entries: usize,
) -> Result<MultipartArchiveAuditReport, String> {
    if !source_root.is_absolute() {
        return Err("multipart-audit-root-must-be-absolute".into());
    }
    let supplied_root_metadata = std::fs::symlink_metadata(source_root)
        .map_err(|_| "multipart-audit-root-unavailable".to_string())?;
    if !supplied_root_metadata.is_dir() || supplied_root_metadata.file_type().is_symlink() {
        return Err("multipart-audit-root-unsafe".into());
    }
    let root_guard = BoundReadRoot::open(source_root)
        .ok_or_else(|| "multipart-audit-root-unsafe".to_string())?;
    let canonical_root = root_guard
        .canonical_path()
        .ok_or_else(|| "multipart-audit-root-unsafe".to_string())?;
    let max_entries = max_entries.clamp(1, DEFAULT_MAX_ENTRIES);
    let mut evidence_complete = true;
    let mut issue_counts = BTreeMap::new();
    let mut entries_seen = 0usize;
    let mut observations = Vec::new();
    let mut pending = vec![(PathBuf::new(), 0usize)];

    while let Some((directory, depth)) = pending.pop() {
        let mut names = match root_guard.read_dir_names(&directory) {
            Ok(names) => names,
            Err(_) => {
                evidence_complete = false;
                increment_issue(&mut issue_counts, "directory-read-failed");
                continue;
            }
        };
        names.sort();
        for name in names {
            if entries_seen >= max_entries {
                evidence_complete = false;
                increment_issue(&mut issue_counts, "entry-limit-reached");
                pending.clear();
                break;
            }
            entries_seen += 1;
            let relative = if directory.as_os_str().is_empty() {
                PathBuf::from(&name)
            } else {
                directory.join(&name)
            };
            if !valid_relative_path(&relative) {
                evidence_complete = false;
                increment_issue(&mut issue_counts, "relative-path-invalid");
                continue;
            }
            let kind = match root_guard.entry_kind(&relative) {
                Ok(kind) => kind,
                Err(_) => {
                    evidence_complete = false;
                    increment_issue(&mut issue_counts, "entry-stat-failed");
                    continue;
                }
            };
            match kind {
                BoundEntryKind::Symlink => continue,
                BoundEntryKind::Directory => {
                    if depth >= MAX_SCAN_DEPTH {
                        evidence_complete = false;
                        increment_issue(&mut issue_counts, "depth-limit-reached");
                    } else {
                        pending.push((relative, depth + 1));
                    }
                    continue;
                }
                BoundEntryKind::Other => continue,
                BoundEntryKind::File => {}
            }
            let Some(name) = name.to_str() else {
                evidence_complete = false;
                increment_issue(&mut issue_counts, "multipart-name-not-unicode");
                continue;
            };
            let Some((base_name, part_index)) = parse_multipart_archive_name(name) else {
                continue;
            };
            let file = match root_guard.open_file(&relative) {
                Ok(file) => file,
                Err(_) => {
                    evidence_complete = false;
                    increment_issue(&mut issue_counts, "entry-open-failed");
                    continue;
                }
            };
            let metadata = match file.metadata() {
                Ok(metadata) if metadata.is_file() => metadata,
                _ => {
                    evidence_complete = false;
                    increment_issue(&mut issue_counts, "entry-stat-failed");
                    continue;
                }
            };
            let Some(modified_ms) = modified_ms(&metadata) else {
                evidence_complete = false;
                increment_issue(&mut issue_counts, "modified-time-unavailable");
                continue;
            };
            observations.push(RawObservation {
                relative_path: normalized(&relative.to_string_lossy()),
                base_name,
                part_index,
                bytes: metadata.len(),
                modified_ms,
            });
        }
    }

    if root_guard.canonical_path().as_ref() != Some(&canonical_root) {
        return Err("multipart-audit-root-unsafe".into());
    }
    Ok(build_report(
        normalized(&canonical_root.to_string_lossy()),
        observed_at_ms,
        entries_seen,
        evidence_complete,
        issue_counts,
        observations,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(path: &str, bytes: u64, modified_ms: u64) -> RawObservation {
        let name = Path::new(path).file_name().unwrap().to_string_lossy();
        let (base_name, part_index) = parse_multipart_archive_name(&name).unwrap();
        RawObservation {
            relative_path: path.into(),
            base_name,
            part_index,
            bytes,
            modified_ms,
        }
    }

    fn report(observations: Vec<RawObservation>) -> MultipartArchiveAuditReport {
        build_report(
            "/source".into(),
            10,
            observations.len(),
            true,
            BTreeMap::new(),
            observations,
        )
    }

    #[test]
    fn multipart_name_parser_is_strict_and_case_insensitive() {
        assert_eq!(
            parse_multipart_archive_name("Bundle.ZIP.PART004"),
            Some(("bundle.zip".into(), 4))
        );
        for invalid in [
            "bundle.zip.part04",
            "bundle.zip.part0000",
            "bundle.tar.part000",
            "bundle.zip.partabc",
            "bundle.zip",
        ] {
            assert_eq!(parse_multipart_archive_name(invalid), None, "{invalid}");
        }
    }

    #[test]
    fn groups_a_set_and_proves_internal_missing_parts() {
        let report = report(vec![
            observation("bundle.zip.part000", 100, 1),
            observation("bundle.zip.part001", 100, 2),
            observation("bundle.zip.part003", 80, 3),
            observation("bundle.zip.part004", 50, 4),
        ]);
        assert_eq!(report.set_count, 1);
        assert_eq!(report.part_count, 4);
        assert_eq!(report.part_bytes, 330);
        assert_eq!(report.incomplete_set_count, 1);
        assert_eq!(report.discard_review_bytes, 330);
        let set = &report.sets[0];
        assert_eq!(set.present_parts, [0, 1, 3, 4]);
        assert_eq!(set.missing_parts, [2]);
        assert_eq!(set.state, MultipartSetState::MissingParts);
        assert_eq!(set.complete_reassembly_possible, Some(false));
        assert!(!set.automatic_discard_allowed);
    }

    #[test]
    fn duplicate_part_indices_fail_closed() {
        let report = report(vec![
            observation("a/bundle.zip.part000", 100, 1),
            observation("a/BUNDLE.ZIP.PART000", 100, 1),
        ]);
        let set = &report.sets[0];
        assert_eq!(set.duplicate_part_indices, [0]);
        assert_eq!(set.state, MultipartSetState::DuplicatePartIndex);
        assert_eq!(set.complete_reassembly_possible, Some(false));
    }

    #[test]
    fn contiguous_parts_do_not_invent_a_terminal_boundary() {
        let report = report(vec![
            observation("bundle.zip.part000", 100, 1),
            observation("bundle.zip.part001", 50, 2),
        ]);
        let set = &report.sets[0];
        assert!(set.missing_parts.is_empty());
        assert_eq!(set.state, MultipartSetState::ContiguousTerminalUnverified);
        assert_eq!(set.complete_reassembly_possible, None);
        assert_eq!(report.discard_review_bytes, 0);
    }

    #[test]
    fn fingerprints_are_order_stable_and_bind_member_state() {
        let first = report(vec![
            observation("bundle.zip.part001", 50, 2),
            observation("bundle.zip.part000", 100, 1),
        ]);
        let second = report(vec![
            observation("bundle.zip.part000", 100, 1),
            observation("bundle.zip.part001", 50, 2),
        ]);
        assert_eq!(first.audit_fingerprint, second.audit_fingerprint);
        assert_eq!(
            first.sets[0].set_fingerprint,
            second.sets[0].set_fingerprint
        );
        let changed = report(vec![
            observation("bundle.zip.part000", 101, 1),
            observation("bundle.zip.part001", 50, 2),
        ]);
        assert_ne!(first.audit_fingerprint, changed.audit_fingerprint);
    }

    #[test]
    fn public_summary_redacts_source_and_member_names() {
        let report = report(vec![
            observation("private/client.zip.part000", 100, 1),
            observation("private/client.zip.part002", 50, 2),
        ]);
        let encoded = serde_json::to_string(&summarize_multipart_audit(&report)).unwrap();
        for private in ["/source", "private", "client.zip", "part000"] {
            assert!(!encoded.contains(private), "{private}");
        }
        assert!(encoded.contains(&report.audit_fingerprint));
        assert!(encoded.contains(&report.sets[0].set_fingerprint));
        assert!(encoded.contains("\"missing_parts\":[1]"));
    }

    #[cfg(not(coverage))]
    #[test]
    fn collector_skips_symlinks_and_reports_real_parts() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("bundle.zip.part000"), b"first").unwrap();
        std::fs::write(temp.path().join("bundle.zip.part002"), b"last").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            temp.path().join("bundle.zip.part000"),
            temp.path().join("copy.zip.part000"),
        )
        .unwrap();

        let report = collect_multipart_archive_audit(temp.path(), 10, 100).unwrap();
        assert!(report.evidence_complete);
        assert_eq!(report.set_count, 1);
        assert_eq!(report.part_count, 2);
        assert_eq!(report.sets[0].missing_parts, [1]);
    }

    #[test]
    fn issue_kind_collection_is_bounded() {
        let mut issues = BTreeMap::new();
        for index in 0..100 {
            increment_issue(&mut issues, &format!("issue-{index}"));
        }
        assert!(issues.len() <= MAX_RECORDED_ISSUE_KINDS + 1);
        assert!(issues.contains_key("additional-issue-kinds-truncated"));
    }
}
