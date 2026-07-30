//! Redacted, read-only export of verified duplicate-audit evidence for Naruon.
//!
//! The private report contains local paths, filenames, content digests, timestamps, and file
//! identities. This contract exports only opaque path bindings, source-stability timestamps, and
//! checked arithmetic. It neither chooses a canonical copy nor authorizes a discard.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use sha2::{Digest, Sha256};

use crate::duplicate_audit::{
    duplicate_audit_report_integrity_valid, DuplicateAuditOptionsSnapshot, DuplicateAuditReport,
};

pub const NARUON_DUPLICATE_AUDIT_LINEAGE_SCHEMA_VERSION: u32 = 1;
pub const NARUON_DUPLICATE_AUDIT_LINEAGE_SCHEMA_KIND: &str = "disksage.duplicate-audit-lineage";

const SOURCE_REF_DOMAIN: &[u8] = b"disksage-naruon-duplicate-audit-source-ref-v1\0";
const LINEAGE_ID_DOMAIN: &[u8] = b"disksage-naruon-duplicate-audit-lineage-id-v1\0";

const CONTENT_DIGEST_ALGORITHMS: [&str; 3] = ["blake3", "sha256", "quickxor"];
const EVIDENCE_PRECEDENCE: [&str; 4] = [
    "embedded_metadata",
    "explicit_filename_date",
    "filesystem_created_at",
    "filesystem_modified_at",
];
const REDACTED_FIELDS: [&str; 5] = [
    "source-root",
    "source-and-relative-paths",
    "file-names",
    "content-digests",
    "device-and-inode",
];

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaruonDuplicateAuditMemberLineage {
    pub source_path_fingerprint: String,
    pub source_ref: String,
    pub logical_bytes: u64,
    pub allocated_bytes: u64,
    pub filesystem_created_ms: Option<u64>,
    pub filesystem_modified_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaruonDuplicateAuditGroupLineage {
    pub group_fingerprint: String,
    pub logical_bytes_per_file: u64,
    pub path_count: usize,
    pub unique_file_count: usize,
    pub hardlink_alias_count: usize,
    pub reclaimable_logical_bytes: u64,
    pub reclaimable_allocated_upper_bound_bytes: u64,
    pub unique_allocated_bytes: Vec<u64>,
    pub members: Vec<NaruonDuplicateAuditMemberLineage>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaruonDuplicateAuditProductionTimeLineage {
    pub assigned: bool,
    pub selected_value_ms: Option<u64>,
    pub selected_source: Option<String>,
    pub evidence_precedence: Vec<String>,
    pub filename_date_used_as_production_time: bool,
    pub filesystem_times_used_only_for_source_stability: bool,
    pub embedded_metadata_review_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaruonDuplicateAuditRedactionLineage {
    pub source_root_redacted: bool,
    pub source_paths_redacted: bool,
    pub file_names_redacted: bool,
    pub content_digests_redacted: bool,
    pub file_identities_redacted: bool,
    pub redacted_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NaruonDuplicateAuditLineageEnvelope {
    pub schema_version: u32,
    pub schema_kind: String,
    pub lineage_id: String,
    pub exported_at_ms: u64,
    pub observed_at_ms: u64,
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
    pub content_digest_algorithms: Vec<String>,
    pub groups: Vec<NaruonDuplicateAuditGroupLineage>,
    pub automatic_discard_allowed: bool,
    pub human_context_review_required: bool,
    pub mutation_performed: bool,
    pub production_time: NaruonDuplicateAuditProductionTimeLineage,
    pub redaction: NaruonDuplicateAuditRedactionLineage,
}

fn encode_hex(bytes: impl IntoIterator<Item = u8>) -> String {
    let mut encoded = String::with_capacity(64);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").expect("writing a digest to String cannot fail");
    }
    encoded
}

fn opaque_source_ref(
    report_fingerprint: &str,
    group_fingerprint: &str,
    path_fingerprint: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(SOURCE_REF_DOMAIN);
    hasher.update(report_fingerprint.as_bytes());
    hasher.update(&[0]);
    hasher.update(group_fingerprint.as_bytes());
    hasher.update(&[0]);
    hasher.update(path_fingerprint.as_bytes());
    encode_hex(hasher.finalize())
}

/// Compute the cross-language v1 envelope binding used by DiskSage and Naruon.
///
/// The input is the lexicographically keyed compact JSON object with `lineage_id` removed. All
/// exported strings are ASCII and all numbers are unsigned integers, so Rust and Python produce
/// identical bytes without locale, floating-point, or Unicode-normalization ambiguity.
pub fn duplicate_audit_lineage_id(envelope: &NaruonDuplicateAuditLineageEnvelope) -> String {
    let mut canonical = serde_json::to_value(envelope)
        .expect("serializing the duplicate-audit lineage envelope cannot fail");
    canonical
        .as_object_mut()
        .expect("the duplicate-audit lineage envelope is an object")
        .remove("lineage_id");
    let canonical = serde_json::to_vec(&canonical)
        .expect("serializing the canonical duplicate-audit lineage object cannot fail");
    let mut hasher = Sha256::new();
    hasher.update(LINEAGE_ID_DOMAIN);
    hasher.update(canonical);
    encode_hex(hasher.finalize())
}

/// Export a path-free, digest-free duplicate evidence envelope.
pub fn export_naruon_duplicate_audit_lineage(
    report: &DuplicateAuditReport,
    exported_at_ms: u64,
) -> Result<NaruonDuplicateAuditLineageEnvelope, String> {
    if !duplicate_audit_report_integrity_valid(report) {
        return Err("naruon-duplicate-audit-lineage-integrity-invalid".into());
    }
    if exported_at_ms < report.observed_at_ms
        || !report.evidence_complete
        || !report.context_metadata_complete
        || report.evidence_gap_count != 0
        || !report.issue_counts.is_empty()
        || report.groups.is_empty()
        || report.automatic_discard_allowed
        || !report.human_context_review_required
        || report.mutation_performed
    {
        return Err("naruon-duplicate-audit-lineage-evidence-incomplete".into());
    }

    let mut group_fingerprints = BTreeSet::new();
    let mut source_refs = BTreeSet::new();
    let mut groups = Vec::with_capacity(report.groups.len());
    for private_group in &report.groups {
        if !group_fingerprints.insert(private_group.group_fingerprint.as_str()) {
            return Err("naruon-duplicate-audit-lineage-group-duplicate".into());
        }
        let mut allocated_by_identity = BTreeMap::new();
        let mut members = Vec::with_capacity(private_group.files.len());
        for file in &private_group.files {
            if file.filesystem_created_ms.is_none() || file.filesystem_modified_ms.is_none() {
                return Err("naruon-duplicate-audit-lineage-context-metadata-incomplete".into());
            }
            let source_ref = opaque_source_ref(
                &report.report_fingerprint,
                &private_group.group_fingerprint,
                &file.path_fingerprint,
            );
            if !source_refs.insert(source_ref.clone()) {
                return Err("naruon-duplicate-audit-lineage-source-duplicate".into());
            }
            allocated_by_identity
                .entry((file.device, file.inode))
                .or_insert(file.allocated_bytes);
            members.push(NaruonDuplicateAuditMemberLineage {
                source_path_fingerprint: file.path_fingerprint.clone(),
                source_ref,
                logical_bytes: file.logical_bytes,
                allocated_bytes: file.allocated_bytes,
                filesystem_created_ms: file.filesystem_created_ms,
                filesystem_modified_ms: file.filesystem_modified_ms,
            });
        }
        members.sort_by(|left, right| {
            left.source_path_fingerprint
                .cmp(&right.source_path_fingerprint)
        });
        let mut unique_allocated_bytes: Vec<_> = allocated_by_identity.into_values().collect();
        unique_allocated_bytes.sort_unstable();
        groups.push(NaruonDuplicateAuditGroupLineage {
            group_fingerprint: private_group.group_fingerprint.clone(),
            logical_bytes_per_file: private_group.logical_bytes_per_file,
            path_count: private_group.path_count,
            unique_file_count: private_group.unique_file_count,
            hardlink_alias_count: private_group.hardlink_alias_count,
            reclaimable_logical_bytes: private_group.reclaimable_logical_bytes,
            reclaimable_allocated_upper_bound_bytes: private_group
                .reclaimable_allocated_upper_bound_bytes,
            unique_allocated_bytes,
            members,
        });
    }
    groups.sort_by(|left, right| left.group_fingerprint.cmp(&right.group_fingerprint));

    let mut envelope = NaruonDuplicateAuditLineageEnvelope {
        schema_version: NARUON_DUPLICATE_AUDIT_LINEAGE_SCHEMA_VERSION,
        schema_kind: NARUON_DUPLICATE_AUDIT_LINEAGE_SCHEMA_KIND.into(),
        lineage_id: String::new(),
        exported_at_ms,
        observed_at_ms: report.observed_at_ms,
        source_scope_fingerprint: report.source_scope_fingerprint.clone(),
        report_fingerprint: report.report_fingerprint.clone(),
        options: report.options.clone(),
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
        reclaimable_allocated_upper_bound_bytes: report.reclaimable_allocated_upper_bound_bytes,
        evidence_complete: true,
        context_metadata_complete: true,
        evidence_gap_count: 0,
        issue_counts: BTreeMap::new(),
        content_digest_algorithms: CONTENT_DIGEST_ALGORITHMS.map(str::to_string).to_vec(),
        groups,
        automatic_discard_allowed: false,
        human_context_review_required: true,
        mutation_performed: false,
        production_time: NaruonDuplicateAuditProductionTimeLineage {
            assigned: false,
            selected_value_ms: None,
            selected_source: None,
            evidence_precedence: EVIDENCE_PRECEDENCE.map(str::to_string).to_vec(),
            filename_date_used_as_production_time: false,
            filesystem_times_used_only_for_source_stability: true,
            embedded_metadata_review_required: true,
        },
        redaction: NaruonDuplicateAuditRedactionLineage {
            source_root_redacted: true,
            source_paths_redacted: true,
            file_names_redacted: true,
            content_digests_redacted: true,
            file_identities_redacted: true,
            redacted_fields: REDACTED_FIELDS.map(str::to_string).to_vec(),
        },
    };
    envelope.lineage_id = duplicate_audit_lineage_id(&envelope);
    Ok(envelope)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::duplicate_audit::{audit_duplicates, DuplicateAuditOptions};

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
    fn exports_redacted_bound_lineage_without_authorizing_discard() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("private-one.bin"), b"same payload").unwrap();
        std::fs::write(temp.path().join("private-two.bin"), b"same payload").unwrap();
        let report = audit_duplicates(temp.path(), &options(), 100).unwrap();
        let envelope = export_naruon_duplicate_audit_lineage(&report, 101).unwrap();
        let encoded = serde_json::to_string(&envelope).unwrap();

        assert_eq!(envelope.lineage_id, duplicate_audit_lineage_id(&envelope));
        assert_eq!(envelope.duplicate_group_count, 1);
        assert!(!envelope.automatic_discard_allowed);
        assert!(envelope.human_context_review_required);
        assert!(!envelope.mutation_performed);
        assert!(envelope.production_time.embedded_metadata_review_required);
        assert!(!encoded.contains("private-one.bin"));
        assert!(!encoded.contains(&report.source_root));
        assert!(!encoded.contains(&report.groups[0].content_digests.sha256));
        assert!(!encoded.contains("\"device\""));
        assert!(!encoded.contains("\"inode\""));
    }

    #[test]
    fn rejects_tampered_or_incomplete_private_evidence() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("one.bin"), b"same payload").unwrap();
        std::fs::write(temp.path().join("two.bin"), b"same payload").unwrap();
        let mut report = audit_duplicates(temp.path(), &options(), 100).unwrap();
        report.duplicate_path_count += 1;
        assert_eq!(
            export_naruon_duplicate_audit_lineage(&report, 101).unwrap_err(),
            "naruon-duplicate-audit-lineage-integrity-invalid"
        );

        let mut report = audit_duplicates(temp.path(), &options(), 100).unwrap();
        report.groups[0].files[0].filesystem_created_ms = None;
        assert_eq!(
            export_naruon_duplicate_audit_lineage(&report, 101).unwrap_err(),
            "naruon-duplicate-audit-lineage-context-metadata-incomplete"
        );
    }
}
